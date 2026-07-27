use super::*;
use crate::browser::cdp::CdpEventWithParams;
use crate::browser::dom::AxNode;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

fn test_session(cdp: CdpClient) -> BrowserSession {
    cdp.set_active_target_route(
        Some("test-target".to_string()),
        None,
        Some("test-frame".to_string()),
        None,
    );
    BrowserSession {
        cdp,
        chrome: None,
        disposable_profile: None,
        launched_incognito_context_id: None,
        profile: "test".to_string(),
        interaction_mode: InteractionMode::Fast,
        user_agent_original: Mutex::new(None),
        polite_last_request: Mutex::new(None),
        mouse: MouseEngine::new(),
        pointer: Mutex::new(None),
        page_revision: Arc::new(AtomicU64::new(1)),
        observation_cache: Mutex::new(None),
        accessibility_cache: Mutex::new(None),
        observation_context: Arc::new(Mutex::new(None)),
        network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
        diagnostic_leases: Arc::new(Mutex::new(DiagnosticLeaseState::default())),
        download_scope: Arc::new(Mutex::new(())),
        topology: Arc::new(Mutex::new(TopologyRegistry {
            active_target_id: Some("test-target".to_string()),
            active_frame_id: Some("test-frame".to_string()),
            ..TopologyRegistry::default()
        })),
        popup_click_scope: Mutex::new(()),
        upload_root: std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
        policy: BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap(),
        policy_interception: None,
        audit_log: std::sync::Mutex::new(VecDeque::new()),
        audit_sequence: AtomicU64::new(1),
        audit_enabled: false,
    }
}

#[test]
fn attach_options_reject_launch_only_configuration() {
    let attached = SessionOptions {
        attach: true,
        ..SessionOptions::default()
    };
    assert!(attached.validate().is_ok());

    let mut incognito = attached.clone();
    incognito.incognito = true;
    assert!(
        incognito
            .validate()
            .unwrap_err()
            .to_string()
            .contains("--incognito")
    );

    let mut profile = attached.clone();
    profile.profile = "work".to_string();
    assert!(
        profile
            .validate()
            .unwrap_err()
            .to_string()
            .contains("--profile")
    );

    let mut chrome_path = attached.clone();
    chrome_path.chrome_path = Some(PathBuf::from("/tmp/chrome"));
    assert!(
        chrome_path
            .validate()
            .unwrap_err()
            .to_string()
            .contains("--chrome-path")
    );

    let mut headed = attached;
    headed.headed = true;
    assert!(
        headed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("--headed")
    );
}

#[test]
fn target_id_must_not_be_empty() {
    let options = SessionOptions {
        target_id: Some("   ".to_string()),
        ..SessionOptions::default()
    };

    assert!(
        options
            .validate()
            .unwrap_err()
            .to_string()
            .contains("target ID")
    );
}

#[test]
fn disposable_incognito_directories_are_unique_and_removed() {
    let first = DisposableProfileDir::create().unwrap();
    let first_path = first.path().to_path_buf();
    let second = DisposableProfileDir::create().unwrap();
    let second_path = second.path().to_path_buf();

    assert_ne!(first_path, second_path);
    assert!(first_path.is_dir());
    assert!(second_path.is_dir());

    drop(first);
    drop(second);
    assert!(!first_path.exists());
    assert!(!second_path.exists());
}

#[test]
fn disposable_cleanup_removes_only_provably_abandoned_profiles() {
    let root = std::env::temp_dir().join(format!(
        "glass-disposable-cleanup-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let active = root.join("incognito-active");
    let dead = root.join("incognito-dead");
    let malformed = root.join("incognito-malformed");
    for path in [&active, &dead, &malformed] {
        std::fs::create_dir(path).unwrap();
    }
    let active_owner = DisposableProfileOwner {
        pid: std::process::id(),
        process_start: process_start_identity(std::process::id()).unwrap(),
    };
    std::fs::write(
        active.join(DISPOSABLE_OWNER_FILE),
        serde_json::to_vec(&active_owner).unwrap(),
    )
    .unwrap();
    let dead_owner = DisposableProfileOwner {
        pid: u32::MAX,
        process_start: 1,
    };
    std::fs::write(
        dead.join(DISPOSABLE_OWNER_FILE),
        serde_json::to_vec(&dead_owner).unwrap(),
    )
    .unwrap();
    std::fs::write(malformed.join(DISPOSABLE_OWNER_FILE), b"not-json").unwrap();

    DisposableProfileDir::cleanup_abandoned(&root).unwrap();

    assert!(active.exists());
    assert!(!dead.exists());
    assert!(malformed.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn disposable_cleanup_scans_beyond_one_memory_batch() {
    let root = std::env::temp_dir().join(format!(
        "glass-disposable-batch-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let owner = serde_json::to_vec(&DisposableProfileOwner {
        pid: u32::MAX,
        process_start: 1,
    })
    .unwrap();
    for index in 0..=DISPOSABLE_CLEANUP_BATCH {
        let path = root.join(format!("incognito-{index:04}"));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join(DISPOSABLE_OWNER_FILE), &owner).unwrap();
    }

    DisposableProfileDir::cleanup_abandoned(&root).unwrap();

    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn disposable_profile_is_recovered_after_forced_process_exit() {
    let path_record = std::env::temp_dir().join(format!(
        "glass-crash-profile-path-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "browser::session::tests::disposable_profile_crash_helper",
            "--ignored",
        ])
        .env("GLASS_CRASH_PROFILE_PATH_RECORD", &path_record)
        .status()
        .unwrap();
    assert!(!status.success());
    let abandoned = PathBuf::from(std::fs::read_to_string(&path_record).unwrap());
    assert!(abandoned.exists());

    DisposableProfileDir::cleanup_abandoned(&std::env::temp_dir().join("glass")).unwrap();

    assert!(!abandoned.exists());
    std::fs::remove_file(path_record).unwrap();
}

#[test]
#[ignore = "subprocess helper for forced-exit recovery"]
fn disposable_profile_crash_helper() {
    let Some(path_record) = std::env::var_os("GLASS_CRASH_PROFILE_PATH_RECORD") else {
        return;
    };
    let profile = DisposableProfileDir::create().unwrap();
    std::fs::write(path_record, profile.path().to_string_lossy().as_bytes()).unwrap();
    std::mem::forget(profile);
    std::process::exit(86);
}

async fn observation_server(include_dom: bool) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut saw_runtime = false;
        let mut saw_accessibility = false;
        let mut saw_deep_dom = false;
        let mut saw_flattened = false;

        for _ in 0..if include_dom { 6 } else { 5 } {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            let result = match request["method"].as_str() {
                Some("Page.createIsolatedWorld") => {
                    serde_json::json!({"executionContextId": 71})
                }
                Some("Runtime.evaluate") => {
                    saw_runtime = true;
                    let expression = request["params"]["expression"].as_str().unwrap();
                    assert!(expression.contains("document.body.innerText"));
                    assert!(!expression.contains(".slice(0,"));
                    let text = format!(
                        "{}😀{}",
                        "a".repeat(4_095),
                        "b".repeat(COMPACT_TEXT_MAX_BYTES)
                    );
                    let page_state = serde_json::json!({
                        "url": "https://example.test",
                        "title": "Example",
                        "ready_state": "complete",
                        "text": text,
                        "mutation_revision": 0,
                        "boundaries": {
                            "scanned_elements": 12,
                            "scan_limit": 512,
                            "shadow_roots": 1,
                            "child_frames": 1,
                            "canvases": 1,
                            "truncated": false
                        }
                    });
                    serde_json::json!({
                        "result": {"value": page_state}
                    })
                }
                Some("Accessibility.getFullAXTree") => {
                    saw_accessibility = true;
                    serde_json::json!({"nodes": []})
                }
                Some("DOM.getDocument") => {
                    saw_deep_dom = true;
                    assert_eq!(request["params"], serde_json::json!({"depth": -1}));
                    serde_json::json!({
                        "root": {
                            "nodeId": 1,
                            "nodeName": "#document",
                            "nodeValue": "",
                            "children": []
                        }
                    })
                }
                Some("DOM.getFlattenedDocument") => {
                    saw_flattened = true;
                    // Return empty flattened doc — no shadow content to pierce
                    serde_json::json!({"nodes": []})
                }
                method => panic!("unexpected compact-observation command: {method:?}"),
            };
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }

        assert!(saw_runtime);
        assert!(saw_accessibility);
        assert_eq!(saw_deep_dom, include_dom);
        assert!(saw_flattened);
    });
    (format!("ws://{address}"), server)
}

async fn mutation_race_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut runtime_revision = 0_u64;
        for _ in 0..7 {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            let result = match request["method"].as_str() {
                Some("Page.createIsolatedWorld") => {
                    serde_json::json!({"executionContextId": 72})
                }
                Some("Runtime.evaluate") => {
                    runtime_revision += 1;
                    serde_json::json!({"result": {"value": {
                        "url": "https://race.test",
                        "title": "Race",
                        "ready_state": "complete",
                        "text": "changing",
                        "mutation_revision": runtime_revision,
                        "boundaries": {"scanned_elements": 1, "scan_limit": 512,
                            "shadow_roots": 0, "child_frames": 0, "canvases": 0,
                            "truncated": false}
                    }}})
                }
                Some("Accessibility.getFullAXTree") => serde_json::json!({"nodes": []}),
                method => panic!("unexpected mutation-race command: {method:?}"),
            };
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": request["id"], "result": result
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
    });
    (format!("ws://{address}"), server)
}

async fn diagnostic_cleanup_server() -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut methods = Vec::new();
        for _ in 0..6 {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            methods.push(request["method"].as_str().unwrap().to_string());
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        methods
    });
    (format!("ws://{address}"), server)
}

async fn download_bridge_server(
    delay_page_allow: bool,
    request_count: usize,
    error_indices: Vec<usize>,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut requests = Vec::new();
        for index in 0..request_count {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            if delay_page_allow && index == 1 {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            let response = if error_indices.contains(&index) {
                serde_json::json!({
                    "id": request["id"],
                    "error": {"code": -32000, "message": "diagnostic failure"}
                })
            } else {
                serde_json::json!({"id": request["id"], "result": {}})
            };
            websocket
                .send(Message::Text(response.to_string().into()))
                .await
                .unwrap();
            requests.push(request);
        }
        requests
    });
    (format!("ws://{address}"), server)
}

async fn download_context_server(
    context_id: &str,
    behavior_requests: usize,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let context_id = context_id.to_string();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let lookup = websocket.next().await.unwrap().unwrap();
        let lookup: Value = match lookup {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected text CDP request"),
        };
        assert_eq!(lookup["method"], "Target.getTargetInfo");
        assert_eq!(lookup["params"]["targetId"], "selected-target");
        websocket
            .send(Message::Text(
                serde_json::json!({
                    "id": lookup["id"],
                    "result": {"targetInfo": {
                        "targetId": "selected-target",
                        "browserContextId": context_id
                    }}
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let mut requests = Vec::new();
        for _ in 0..behavior_requests {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            requests.push(request);
        }
        requests
    });
    (format!("ws://{address}"), server)
}

async fn final_popup_server(
    topology: Arc<Mutex<TopologyRegistry>>,
    move_first_query: bool,
    move_every_query: bool,
    query_delay: Option<Duration>,
) -> (String, tokio::task::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut queries = 0;
        while let Ok(Some(Ok(request))) =
            tokio::time::timeout(Duration::from_millis(250), websocket.next()).await
        {
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                Message::Close(_) => break,
                _ => continue,
            };
            assert_eq!(request["method"], "Target.getTargets");
            queries += 1;
            if move_every_query || move_first_query && queries == 1 {
                topology.lock().await.sequence += 1;
            }
            if let Some(delay) = query_delay {
                tokio::time::sleep(delay).await;
            }
            if websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": request["id"],
                        "result": {"targetInfos": [
                            {"type": "page", "targetId": "original"},
                            {"type": "page", "targetId": "popup", "openerId": "original"}
                        ]}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
        queries
    });
    (format!("ws://{address}"), server)
}

async fn large_accessibility_server() -> (String, tokio::task::JoinHandle<()>, String) {
    let huge_text = "x".repeat(33 * 1024);
    let tree = serde_json::json!({
        "nodes": [
            {
                "nodeId": "root",
                "role": {"value": "RootWebArea"},
                "name": {"value": huge_text.clone()},
                "description": {"value": huge_text.clone()},
                "value": {"value": huge_text.clone()},
                "childIds": ["save"]
            },
            {
                "nodeId": "save",
                "parentId": "root",
                "backendDOMNodeId": 42,
                "role": {"value": "button"},
                "name": {"value": "Save"},
                "description": {"value": huge_text.clone()},
                "value": {"value": huge_text.clone()},
                "childIds": []
            }
        ]
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let text_for_server = huge_text.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        for _ in 0..6 {
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            let result = match request["method"].as_str() {
                Some("Page.createIsolatedWorld") => {
                    serde_json::json!({"executionContextId": 73})
                }
                Some("Runtime.evaluate") => serde_json::json!({
                    "result": {"value": if request["params"]["expression"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("JSON.stringify")
                    {
                        serde_json::Value::String(
                            serde_json::json!({
                                "url": "https://example.test",
                                "title": "Example",
                                "ready_state": "complete",
                                "text": text_for_server.clone(),
                            })
                            .to_string(),
                        )
                    } else {
                        serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text_for_server.clone(),
                        })
                    }}
                }),
                Some("Accessibility.getFullAXTree") => tree.clone(),
                method => panic!("unexpected compact-observation command: {method:?}"),
            };
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    });
    (format!("ws://{address}"), server, huge_text)
}

#[test]
fn normalizes_urls_without_touching_supported_schemes() {
    assert_eq!(normalize_url("example.com"), "https://example.com");
    assert_eq!(normalize_url(" about:blank "), "about:blank");
    assert_eq!(
        normalize_url("file:///tmp/page.html"),
        "file:///tmp/page.html"
    );
}

#[test]
fn interaction_modes_plan_smooth_or_direct_motion() {
    let mouse = MouseEngine::new();
    let start = Point { x: 10.0, y: 20.0 };
    let end = Point { x: 410.0, y: 220.0 };

    let human = interaction_path(InteractionMode::Human, &mouse, start, end);
    let fast = interaction_path(InteractionMode::Fast, &mouse, start, end);

    assert!(human.len() > 2);
    assert_eq!(human.first(), Some(&start));
    assert_eq!(human.last(), Some(&end));
    assert_eq!(fast, vec![start, end]);
}

#[test]
fn topology_events_never_select_a_popup_and_clear_a_lost_active_target() {
    let mut topology = TopologyRegistry {
        active_target_id: Some("page-1".to_string()),
        active_session_id: Some("session-1".to_string()),
        ..TopologyRegistry::default()
    };
    let popup = CdpEventWithParams {
        method: "Target.targetCreated".to_string(),
        params: serde_json::json!({"targetInfo": {
            "type": "page", "targetId": "popup-1", "url": "about:blank",
            "title": "", "openerId": "page-1"
        }}),
        session_id: None,
    };
    assert!(!apply_topology_event(&mut topology, &popup));
    assert_eq!(topology.active_target_id.as_deref(), Some("page-1"));
    assert_eq!(topology.targets[0].opener_id.as_deref(), Some("page-1"));

    let crashed = CdpEventWithParams {
        method: "Target.targetCrashed".to_string(),
        params: serde_json::json!({"targetId": "page-1"}),
        session_id: None,
    };
    assert!(apply_topology_event(&mut topology, &crashed));
    assert!(topology.active_target_id.is_none());
    assert!(topology.events.len() <= TOPOLOGY_MAX_EVENTS);
}

#[test]
fn frame_collection_is_bounded_and_preserves_parents() {
    let tree = serde_json::json!({
        "frame": {"id":"root", "url":"https://root.test"},
        "childFrames": [{"frame":{"id":"child", "url":"https://child.test"}}]
    });
    let mut frames = Vec::new();
    collect_frames(&tree, None, Some("child"), &mut frames).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1].parent_id.as_deref(), Some("root"));
    assert!(frames[1].active);
}

#[test]
fn keyboard_shortcuts_are_bounded_and_map_modifiers() {
    assert_eq!(
        parse_shortcut("Control+Shift+A").unwrap(),
        (10, "A".to_string())
    );
    assert_eq!(key_code("a"), "KeyA");
    assert!(parse_shortcut("Control+A+B").is_err());
    assert!(validate_key("").is_err());
}

#[test]
fn invalidates_context_only_for_page_or_dom_mutations() {
    assert!(context_event_invalidates_observation(
        "DOM.childNodeInserted"
    ));
    assert!(context_event_invalidates_observation("Page.frameNavigated"));
    assert!(!context_event_invalidates_observation(
        "Network.loadingFinished"
    ));
    assert!(observation_context_invalidates("DOM.documentUpdated"));
    assert!(observation_context_invalidates(
        "Runtime.executionContextDestroyed"
    ));
    assert!(!observation_context_invalidates("DOM.childNodeInserted"));
}

#[test]
fn structured_context_omits_screenshot_until_explicitly_populated() {
    let page = PageInfo {
        url: "https://example.test".to_string(),
        title: "Example".to_string(),
        ready_state: "complete".to_string(),
        target_id: "target-1".to_string(),
        frame_id: "frame-1".to_string(),
    };
    let mut context = PageContext {
        page: page.clone(),
        text: "Example".to_string(),
        dom: None,
        accessibility: CompactAccessibilitySnapshot {
            page,
            revision: 7,
            roots: Vec::new(),
            interactive: Vec::new(),
            truncated: false,
            omitted_count: 0,
            ranking_applied: false,
            completeness: None,
        },
        consistency: ObservationConsistency {
            consistent: true,
            attempts: 1,
            start_revision: 0,
            end_revision: 0,
            start_mutation_revision: 0,
            end_mutation_revision: 0,
        },
        boundaries: ObservationBoundarySummary::default(),
        incomplete: Vec::new(),
        screenshot: None,
    };

    let structured = serde_json::to_value(&context).unwrap();
    assert!(structured.get("dom").is_none());
    assert!(structured.get("screenshot").is_none());
    assert_eq!(structured["accessibility"]["revision"], 7);

    context.screenshot = Some("png-data".to_string());
    let visual = serde_json::to_value(&context).unwrap();
    assert_eq!(visual["screenshot"], "png-data");
}

#[test]
fn revisioned_references_are_parsed_and_validate_their_shape() {
    assert_eq!(
        parse_revisioned_reference("r7:b42").unwrap(),
        Some(RevisionedElementReference {
            revision: 7,
            backend_dom_node_id: 42,
        })
    );
    assert_eq!(parse_revisioned_reference("Save").unwrap(), None);
    assert!(parse_revisioned_reference("r7:b0").is_err());
    assert!(parse_revisioned_reference("r:b42").is_err());
}

#[test]
fn locators_parse_explicit_strategies_without_role_only_fallbacks() {
    assert_eq!(
        Locator::parse("r7:b42").unwrap(),
        Locator::Reference("r7:b42".to_string())
    );
    assert_eq!(
        Locator::parse("name=Save").unwrap(),
        Locator::AccessibleName("Save".to_string())
    );
    assert_eq!(
        Locator::parse("role=button;name=Save").unwrap(),
        Locator::RoleAndName {
            role: "button".to_string(),
            name: "Save".to_string(),
        }
    );
    assert_eq!(Locator::parse("ordinal=2").unwrap(), Locator::Ordinal(2));
    assert_eq!(
        Locator::parse("Save").unwrap(),
        Locator::AccessibleName("Save".to_string())
    );
    assert!(Locator::parse("role=button").is_err());
    assert!(Locator::parse("ordinal=0").is_err());
    assert!(Locator::parse("css=").is_err());
}

#[test]
fn wait_conditions_parse_typed_forms_and_reject_unbounded_values() {
    assert_eq!(
        WaitCondition::parse("lifecycle=load").unwrap(),
        WaitCondition::Lifecycle("complete".to_string())
    );
    assert_eq!(
        WaitCondition::parse("target-visible=name=Save").unwrap(),
        WaitCondition::TargetVisible("name=Save".to_string())
    );
    assert_eq!(
        WaitCondition::parse("network-quiet=250").unwrap(),
        WaitCondition::NetworkQuiet(Duration::from_millis(250))
    );
    assert!(WaitCondition::parse("network-quiet=0").is_err());
    assert!(WaitCondition::parse(&format!("text={}", "x".repeat(4096))).is_err());
    assert!(WaitCondition::parse("lifecycle=forever").is_err());
    assert!(WaitCondition::parse("unknown=value").is_err());
    assert!(validate_wait_deadline(Duration::from_millis(1)).is_ok());
    assert!(validate_wait_deadline(Duration::from_secs(301)).is_err());
}

#[test]
fn ambiguity_candidate_labels_are_utf8_safe_and_bounded() {
    let label = bounded_candidate_label(&"界".repeat(100));
    assert!(label.len() <= CANDIDATE_LABEL_MAX_BYTES);
    assert!(label.ends_with('…'));
    assert!(std::str::from_utf8(label.as_bytes()).is_ok());
}

#[test]
fn action_outcomes_are_compact_and_serializable() {
    let outcome = ActionOutcome {
        action: ActionKind::Click,
        target: Some(ActionTarget {
            label: "button Save".to_string(),
            reference: Some("r9:b42".to_string()),
        }),
        revision: 10,
        target_id: "target-1".to_string(),
        frame_id: "frame-1".to_string(),
        evidence: None,
    };

    let value = serde_json::to_value(outcome).unwrap();
    assert_eq!(value["action"], "click");
    assert_eq!(value["target"]["reference"], "r9:b42");
    assert_eq!(value["revision"], 10);
}

#[test]
fn diagnostics_redact_secrets_and_bound_retention() {
    let redacted =
        redact_diagnostic_url("https://user:pass@example.test/path?token=secret&empty=#fragment");
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("user"));
    assert!(!redacted.contains("pass"));
    assert!(!redacted.contains("fragment"));
    assert!(redacted.contains("token=%5Bredacted%5D"));

    let headers = serde_json::json!({
        "Authorization": "Bearer secret",
        "Cookie": "session=secret",
        "X-Trace": "safe",
        "Accept": "*/*"
    });
    assert_eq!(safe_header_names(&headers), vec!["Accept", "X-Trace"]);

    let event = CdpEventWithParams {
        method: "Network.requestWillBeSent".to_string(),
        session_id: None,
        params: serde_json::json!({
            "requestId": "request-1",
            "request": {
                "method": "POST",
                "url": "https://example.test/api?password=hunter2",
                "headers": headers,
                "postData": "never-retain-this"
            }
        }),
    };
    let mut console = Vec::new();
    let mut network = Vec::new();
    let mut indexes = HashMap::new();
    let mut dropped = 0;
    collect_diagnostic_event(
        &event,
        &mut console,
        &mut network,
        &mut indexes,
        &mut dropped,
    );
    let serialized = serde_json::to_string(&network).unwrap();
    assert!(!serialized.contains("hunter2"));
    assert!(!serialized.contains("never-retain-this"));
    assert!(!serialized.contains("Authorization"));
    assert_eq!(network[0].method, "POST");
    assert_eq!(
        redact_diagnostic_text("Authorization: Bearer top-secret"),
        "[redacted sensitive console entry]"
    );
    let console_event = CdpEventWithParams {
        method: "Runtime.consoleAPICalled".to_string(),
        session_id: None,
        params: serde_json::json!({
            "type": "error",
            "args": [{"value": "hunter2"}]
        }),
    };
    for _ in 0..=MAX_DIAGNOSTIC_EVENTS {
        collect_diagnostic_event(
            &console_event,
            &mut console,
            &mut network,
            &mut indexes,
            &mut dropped,
        );
    }
    assert_eq!(console.len(), MAX_DIAGNOSTIC_EVENTS);
    assert_eq!(console[0].text, "[console arguments redacted]");
    assert_eq!(dropped, 1);
}

#[test]
fn visual_capture_validation_uses_effective_viewport_and_scale() {
    let viewport = visual_viewport_rect(&serde_json::json!({
        "pageX": 15.0,
        "pageY": 25.0,
        "clientWidth": 800.0,
        "clientHeight": 600.0
    }))
    .unwrap();
    assert_eq!(viewport.x, 15.0);
    assert_eq!(viewport.y, 25.0);
    assert_eq!(viewport.width, 800.0);
    assert!(validate_effective_visual_clip(Some(viewport), 2.0).is_ok());
    assert!(
        validate_effective_visual_clip(
            Some(VisualClip {
                x: 0.0,
                y: 0.0,
                width: 8_000.0,
                height: 8_000.0
            }),
            4.0
        )
        .is_err()
    );
    assert_eq!(decoded_base64_len("aGVsbG8=").unwrap(), 5);
    assert!(!visual_clips_match(
        viewport,
        VisualClip {
            x: 16.0,
            ..viewport
        }
    ));
}

#[test]
fn full_snapshot_controls_use_revisioned_backend_references() {
    let roots = vec![AxNode {
        ax_node_id: "button".to_string(),
        backend_dom_node_id: Some(42),
        role: "button".to_string(),
        name: "Save".to_string(),
        description: String::new(),
        value: None,
        children: Vec::new(),
        bounds: None,
        interactive: true,
        input_type: None,
    }];
    let controls = interactive_elements(&roots, 12);
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].reference, "r12:b42");
    assert_eq!(controls[0].backend_dom_node_id, 42);
}

#[test]
fn compact_text_cap_is_utf8_safe_and_marks_truncation() {
    let text = "🙂".repeat(COMPACT_TEXT_MAX_BYTES);
    let compact = truncate_visible_text(&text, COMPACT_TEXT_MAX_BYTES);

    assert!(compact.len() <= COMPACT_TEXT_MAX_BYTES);
    assert!(compact.ends_with(TEXT_TRUNCATION_MARKER));
    assert!(compact.is_char_boundary(compact.len()));
}

#[tokio::test]
async fn default_observation_is_compact_and_never_requests_deep_dom() {
    let (url, server) = observation_server(false).await;
    let session = test_session(CdpClient::connect(&url).await.unwrap());

    let context = session.observe().await.unwrap();
    assert!(context.dom.is_none());
    assert!(context.screenshot.is_none());
    assert!(context.text.contains('😀'));
    assert!(context.text.ends_with(TEXT_TRUNCATION_MARKER));
    assert!(context.text.len() <= COMPACT_TEXT_MAX_BYTES);
    assert!(std::str::from_utf8(context.text.as_bytes()).is_ok());
    assert!(context.consistency.consistent);
    assert_eq!(context.consistency.attempts, 1);
    assert_eq!(context.boundaries.shadow_roots, 1);
    assert_eq!(
        context.incomplete,
        vec![
            ObservationIncompleteReason::VisibleText,
            ObservationIncompleteReason::FrameBoundary,
            ObservationIncompleteReason::Canvas,
            ObservationIncompleteReason::ShadowBoundary,
        ]
    );
    let serialized = serde_json::to_value(&context).unwrap();
    assert!(serialized.get("dom").is_none());
    assert!(serialized.get("screenshot").is_none());

    session.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn mutation_race_retries_once_marks_incomplete_and_is_not_cached() {
    let (url, server) = mutation_race_server().await;
    let session = test_session(CdpClient::connect(&url).await.unwrap());

    let context = session.observe().await.unwrap();
    assert!(!context.consistency.consistent);
    assert_eq!(context.consistency.attempts, 2);
    assert!(
        context.consistency.end_mutation_revision > context.consistency.start_mutation_revision
    );
    assert!(
        context
            .incomplete
            .contains(&ObservationIncompleteReason::MutationRace)
    );
    assert!(session.observation_cache.lock().await.is_none());

    session.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn diagnostic_cancellation_disables_every_scoped_domain() {
    let (url, server) = diagnostic_cleanup_server().await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let session = test_session(cdp.clone());
    cdp.set_active_target_route(
        Some("test-target".to_string()),
        Some("diagnostic-session".to_string()),
        Some("test-frame".to_string()),
        None,
    );
    assert!(
        tokio::time::timeout(
            Duration::from_millis(25),
            session.diagnostics(Duration::from_secs(5))
        )
        .await
        .is_err()
    );
    let methods = tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    for method in [
        "Network.enable",
        "Runtime.enable",
        "Log.enable",
        "Log.disable",
        "Runtime.disable",
        "Network.disable",
    ] {
        assert!(
            methods.iter().any(|actual| actual == method),
            "missing {method}"
        );
    }
}

#[test]
fn page_download_bridge_is_scoped_only_to_owned_command_line_incognito() {
    assert!(use_page_download_compatibility(true, true));
    assert!(!use_page_download_compatibility(false, true));
    assert!(!use_page_download_compatibility(true, false));
    assert!(!use_page_download_compatibility(false, false));
}

#[tokio::test]
async fn incognito_download_context_mismatch_fails_before_behavior_mutation() {
    let (url, server) = download_context_server("other-context", 0).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let error = match DownloadBehaviorGuard::acquire_for_incognito(
        cdp.clone(),
        std::env::current_dir().unwrap(),
        "selected-target".to_string(),
        "captured-page-session".to_string(),
        "launched-context".to_string(),
    )
    .await
    {
        Ok(_) => panic!("mismatched context was authorized"),
        Err(error) => error,
    };
    assert_eq!(
        error.downcast_ref::<DownloadError>().unwrap().kind,
        DownloadErrorKind::AuthorizationFailed
    );
    assert!(server.await.unwrap().is_empty());
    cdp.close().await;
}

#[tokio::test]
async fn incognito_download_context_match_reaches_captured_behavior_bridge() {
    let (url, server) = download_context_server("launched-context", 4).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let mut guard = DownloadBehaviorGuard::acquire_for_incognito(
        cdp.clone(),
        std::env::current_dir().unwrap(),
        "selected-target".to_string(),
        "captured-page-session".to_string(),
        "launched-context".to_string(),
    )
    .await
    .unwrap();
    guard.disable().await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(requests[0]["method"], "Browser.setDownloadBehavior");
    assert_eq!(requests[1]["method"], "Page.setDownloadBehavior");
    assert_eq!(requests[1]["sessionId"], "captured-page-session");
    assert_eq!(requests[2]["params"]["behavior"], "deny");
    assert_eq!(requests[3]["params"]["behavior"], "deny");
    cdp.close().await;
}

#[tokio::test]
async fn incognito_download_bridge_allows_and_restores_the_captured_page_route() {
    let (url, server) = download_bridge_server(false, 4, Vec::new()).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let destination = std::env::current_dir().unwrap();
    let mut guard = DownloadBehaviorGuard::acquire(
        cdp.clone(),
        destination.clone(),
        Some("captured-page-session".to_string()),
    )
    .await
    .unwrap();
    guard.disable().await.unwrap();

    let requests = server.await.unwrap();
    assert_eq!(requests[0]["method"], "Browser.setDownloadBehavior");
    assert_eq!(requests[0]["params"]["behavior"], "allow");
    assert_eq!(requests[0]["params"]["eventsEnabled"], true);
    assert_eq!(
        requests[0]["params"]["downloadPath"],
        destination.to_string_lossy().as_ref()
    );
    assert_eq!(requests[1]["method"], "Page.setDownloadBehavior");
    assert_eq!(requests[1]["sessionId"], "captured-page-session");
    assert_eq!(requests[1]["params"]["behavior"], "allow");
    assert_eq!(requests[2]["method"], "Page.setDownloadBehavior");
    assert_eq!(requests[2]["sessionId"], "captured-page-session");
    assert_eq!(requests[2]["params"]["behavior"], "deny");
    assert_eq!(requests[3]["method"], "Browser.setDownloadBehavior");
    assert_eq!(requests[3]["params"]["behavior"], "deny");
    assert_eq!(requests[3]["params"]["eventsEnabled"], false);
    cdp.close().await;
}

#[tokio::test]
async fn cancelled_incognito_download_authorization_restores_both_scopes() {
    let (url, server) = download_bridge_server(true, 4, Vec::new()).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let acquire = DownloadBehaviorGuard::acquire(
        cdp.clone(),
        std::env::current_dir().unwrap(),
        Some("captured-page-session".to_string()),
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), acquire)
            .await
            .is_err()
    );

    let requests = tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(requests[0]["params"]["behavior"], "allow");
    assert_eq!(requests[1]["params"]["behavior"], "allow");
    assert_eq!(requests[2]["params"]["behavior"], "deny");
    assert_eq!(requests[2]["sessionId"], "captured-page-session");
    assert_eq!(requests[3]["params"]["behavior"], "deny");
    cdp.close().await;
}

#[tokio::test]
async fn partial_incognito_download_enable_is_typed_and_restores_browser_deny() {
    let (url, server) = download_bridge_server(false, 3, vec![1]).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let error = match DownloadBehaviorGuard::acquire(
        cdp.clone(),
        std::env::current_dir().unwrap(),
        Some("captured-page-session".to_string()),
    )
    .await
    {
        Ok(_) => panic!("partial authorization unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(
        error.downcast_ref::<DownloadError>().unwrap().kind,
        DownloadErrorKind::AuthorizationFailed
    );
    let requests = server.await.unwrap();
    assert_eq!(requests[2]["method"], "Browser.setDownloadBehavior");
    assert_eq!(requests[2]["params"]["behavior"], "deny");
    cdp.close().await;
}

#[tokio::test]
async fn partial_incognito_download_restoration_is_typed_and_still_denies_browser() {
    let (url, server) = download_bridge_server(false, 4, vec![2]).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let mut guard = DownloadBehaviorGuard::acquire(
        cdp.clone(),
        std::env::current_dir().unwrap(),
        Some("captured-page-session".to_string()),
    )
    .await
    .unwrap();
    let error = guard.disable().await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<DownloadError>().unwrap().kind,
        DownloadErrorKind::RestorationFailed
    );
    let requests = server.await.unwrap();
    assert_eq!(requests[2]["method"], "Page.setDownloadBehavior");
    assert_eq!(requests[3]["method"], "Browser.setDownloadBehavior");
    assert_eq!(requests[3]["params"]["behavior"], "deny");
    guard.armed = false;
    cdp.close().await;
}

#[tokio::test]
async fn deep_dom_observation_is_explicit_and_not_cached() {
    let (url, server) = observation_server(true).await;
    let session = test_session(CdpClient::connect(&url).await.unwrap());

    let deep = session.observe_with_dom().await.unwrap();
    assert_eq!(deep.dom.as_ref().unwrap().node_name, "#document");
    assert!(serde_json::to_value(&deep).unwrap().get("dom").is_some());

    let compact = session.observe().await.unwrap();
    assert!(compact.dom.is_none());

    session.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn compact_observation_bounds_accessibility_while_snapshot_remains_full() {
    let (url, server, huge_text) = large_accessibility_server().await;
    let session = test_session(CdpClient::connect(&url).await.unwrap());

    let context = session.observe().await.unwrap();
    let serialized = serde_json::to_string(&context).unwrap();
    assert!(context.accessibility.truncated);
    assert_eq!(context.accessibility.revision, 1);
    assert_eq!(context.accessibility.roots[0].role, "RootWebArea");
    assert_eq!(context.accessibility.interactive[0].reference, "r1:b42");
    assert_eq!(context.accessibility.interactive[0].role, "button");
    assert_eq!(context.accessibility.interactive[0].name, "Save");
    assert!(!serialized.contains(&huge_text));
    assert!(
        serialized.len()
            <= COMPACT_TEXT_MAX_BYTES + crate::browser::dom::COMPACT_AX_TEXT_MAX_BYTES + 2_048
    );

    let cached = {
        let cache = session.observation_cache.lock().await;
        cache.as_ref().unwrap().context.clone()
    };
    let cached_json = serde_json::to_string(&cached.into_page_context()).unwrap();
    assert!(!cached_json.contains(&huge_text));

    let snapshot = session.snapshot().await.unwrap();
    assert_eq!(snapshot.roots[0].name, huge_text);
    assert_eq!(snapshot.interactive[0].reference, "r1:b42");
    assert_eq!(snapshot.interactive[0].description.len(), 33 * 1024);

    session.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn hardened_navigation_intercepts_private_redirects_before_following() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let enable = websocket.next().await.unwrap().unwrap();
        let enable: Value = match enable {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected Fetch.enable"),
        };
        assert_eq!(enable["method"], "Fetch.enable");
        websocket
            .send(Message::Text(
                serde_json::json!({"id": enable["id"], "result": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        websocket
            .send(Message::Text(
                serde_json::json!({
                    "method": "Fetch.requestPaused",
                    "sessionId": "route-1",
                    "params": {
                        "requestId": "redirect-1",
                        "request": {"url": "http://127.0.0.1/private"}
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let blocked = websocket.next().await.unwrap().unwrap();
        let blocked: Value = match blocked {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected Fetch.failRequest"),
        };
        assert_eq!(blocked["method"], "Fetch.failRequest");
        assert_eq!(blocked["params"]["requestId"], "redirect-1");
        websocket
            .send(Message::Text(
                serde_json::json!({"id": blocked["id"], "result": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let disable = websocket.next().await.unwrap().unwrap();
        let disable: Value = match disable {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected Fetch.disable"),
        };
        assert_eq!(disable["method"], "Fetch.disable");
        websocket
            .send(Message::Text(
                serde_json::json!({"id": disable["id"], "result": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    });
    let cdp = CdpClient::connect(&format!("ws://{address}"))
        .await
        .unwrap();
    let policy = BrowserPolicy::hardened(std::env::current_dir().unwrap()).unwrap();
    let interception = PolicyInterception::start(cdp.clone(), policy, "route-1".to_string())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        interception.take_denial().await,
        Some(PolicyError::Denied { .. })
    ));
    interception.shutdown().await;
    cdp.close().await;
    server.await.unwrap();
}

fn popup_test_snapshot() -> PopupTopologySnapshot {
    PopupTopologySnapshot {
        original_target_id: "original".to_string(),
        original_frame_id: "frame".to_string(),
        preexisting_target_ids: HashSet::from(["original".to_string(), "preexisting".to_string()]),
        sequence: 10,
        event_loss_count: 0,
    }
}

fn popup_test_target(id: &str, opener: Option<&str>) -> PageTargetInfo {
    PageTargetInfo {
        id: id.to_string(),
        url: "about:blank".to_string(),
        title: String::new(),
        opener_id: opener.map(str::to_string),
        active: false,
    }
}

fn popup_topology_with(targets: Vec<(&str, Option<&str>, u64)>) -> TopologyRegistry {
    let mut topology = TopologyRegistry::default();
    for (id, opener, sequence) in targets {
        topology.targets.push(popup_test_target(id, opener));
        topology.target_sequences.insert(id.to_string(), sequence);
        topology.sequence = topology.sequence.max(sequence);
    }
    topology
}

#[test]
fn popup_topology_accepts_exactly_one_later_live_matching_opener() {
    let topology = popup_topology_with(vec![
        ("original", None, 1),
        ("preexisting", Some("original"), 9),
        ("popup", Some("original"), 11),
    ]);
    let candidate = assess_popup_topology(&popup_test_snapshot(), &topology, true).unwrap();
    assert_eq!(candidate.target.id, "popup");
    assert_eq!(candidate.observed_sequence, 11);
}

#[tokio::test]
async fn popup_topology_quiet_window_resets_for_a_late_event_then_stabilizes() {
    let snapshot = popup_test_snapshot();
    let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
        "popup",
        Some("original"),
        11,
    )])));
    let candidate = {
        let topology = topology.lock().await;
        assess_popup_topology(&snapshot, &topology, true).unwrap()
    };
    let late_topology = Arc::clone(&topology);
    let late_event = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(15)).await;
        late_topology.lock().await.sequence += 1;
    });

    let started = tokio::time::Instant::now();
    let stable = wait_for_stable_popup_topology(
        &topology,
        &snapshot,
        &candidate,
        started + Duration::from_millis(150),
        Duration::from_millis(30),
    )
    .await
    .unwrap();

    late_event.await.unwrap();
    assert_eq!(stable.target.id, "popup");
    assert!(started.elapsed() >= Duration::from_millis(40));
    assert_eq!(topology.lock().await.sequence, 12);
}

#[tokio::test]
async fn popup_topology_that_never_becomes_quiet_fails_closed_at_deadline() {
    let snapshot = popup_test_snapshot();
    let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
        "popup",
        Some("original"),
        11,
    )])));
    let candidate = {
        let topology = topology.lock().await;
        assess_popup_topology(&snapshot, &topology, true).unwrap()
    };
    let moving_topology = Arc::clone(&topology);
    let movement = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        for _ in 0..20 {
            interval.tick().await;
            moving_topology.lock().await.sequence += 1;
        }
    });
    let started = tokio::time::Instant::now();

    let error = wait_for_stable_popup_topology(
        &topology,
        &snapshot,
        &candidate,
        started + Duration::from_millis(60),
        Duration::from_millis(15),
    )
    .await
    .unwrap_err();

    movement.abort();
    assert_eq!(error.kind, PopupClickErrorKind::TopologyLagged);
    assert!(started.elapsed() >= Duration::from_millis(55));
}

#[tokio::test]
async fn popup_event_during_final_query_restarts_quiet_then_succeeds() {
    let snapshot = popup_test_snapshot();
    let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
        "popup",
        Some("original"),
        11,
    )])));
    let candidate = {
        let topology = topology.lock().await;
        assess_popup_topology(&snapshot, &topology, true).unwrap()
    };
    let (url, server) = final_popup_server(Arc::clone(&topology), true, false, None).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let mut session = test_session(cdp.clone());
    session.topology = topology;

    session
        .final_popup_verification(
            &snapshot,
            &candidate,
            tokio::time::Instant::now() + Duration::from_millis(250),
        )
        .await
        .unwrap();
    cdp.close().await;
    assert_eq!(server.await.unwrap(), 2);
}

#[tokio::test]
async fn popup_events_during_every_final_query_fail_at_shared_deadline() {
    let snapshot = popup_test_snapshot();
    let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
        "popup",
        Some("original"),
        11,
    )])));
    let candidate = {
        let topology = topology.lock().await;
        assess_popup_topology(&snapshot, &topology, true).unwrap()
    };
    let (url, server) = final_popup_server(Arc::clone(&topology), true, true, None).await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let mut session = test_session(cdp.clone());
    session.topology = topology;
    let started = tokio::time::Instant::now();

    let error = session
        .final_popup_verification(&snapshot, &candidate, started + Duration::from_millis(120))
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<PopupClickError>().unwrap().kind,
        PopupClickErrorKind::TopologyLagged
    );
    assert!(started.elapsed() >= Duration::from_millis(110));
    cdp.close().await;
    assert!(server.await.unwrap() >= 2);
}

#[tokio::test]
async fn popup_stable_final_query_delayed_past_deadline_fails_typed() {
    let snapshot = popup_test_snapshot();
    let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
        "popup",
        Some("original"),
        11,
    )])));
    let candidate = {
        let topology = topology.lock().await;
        assess_popup_topology(&snapshot, &topology, true).unwrap()
    };
    let (url, server) = final_popup_server(
        Arc::clone(&topology),
        false,
        false,
        Some(Duration::from_millis(50)),
    )
    .await;
    let cdp = CdpClient::connect(&url).await.unwrap();
    let mut session = test_session(cdp.clone());
    session.topology = topology;

    let error = session
        .final_popup_verification(
            &snapshot,
            &candidate,
            tokio::time::Instant::now() + Duration::from_millis(15),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<PopupClickError>().unwrap().kind,
        PopupClickErrorKind::TopologyLagged
    );
    cdp.close().await;
    assert_eq!(server.await.unwrap(), 1);
}

#[test]
fn popup_witness_uses_only_isolated_native_event_state() {
    let source = popup_witness_install_function();
    assert!(source.contains("EventTarget.prototype.addEventListener"));
    assert!(source.contains("EventTarget.prototype.removeEventListener"));
    assert!(source.contains("nativeApply(nativeAdd"));
    assert!(source.contains("event.isTrusted === true"));
    assert!(source.contains("event.currentTarget === element"));
    assert!(!source.contains("Runtime.addBinding"));
    assert!(!source.contains("__glass"));
    assert!(!source.contains("element.addEventListener"));
}

#[test]
fn popup_errors_are_bounded_and_serializable() {
    let error = popup_typed_error(PopupClickErrorKind::PopupMissing, "x".repeat(2_000));
    assert!(error.message.len() <= POPUP_ERROR_MESSAGE_MAX_BYTES);
    assert_eq!(
        serde_json::to_value(error).unwrap()["kind"],
        "popup_missing"
    );
}

#[test]
fn popup_timing_evidence_is_explicitly_serializable() {
    let evidence = PopupVerificationEvidence {
        trusted_click_witness: true,
        release_acknowledged: false,
        release_ack_wait_ms: 500.5,
        topology_sequence_before_release: 1,
        popup_observed_sequence: 2,
        attached: true,
        ready_state: "complete".to_string(),
    };
    let value = serde_json::to_value(evidence).unwrap();
    assert_eq!(value["release_ack_wait_ms"], 500.5);
}

#[tokio::test]
async fn cancelled_popup_attach_detaches_a_late_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let attach = websocket.next().await.unwrap().unwrap();
        let attach: Value = match attach {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected text CDP request"),
        };
        assert_eq!(attach["method"], "Target.attachToTarget");
        tokio::time::sleep(Duration::from_millis(50)).await;
        websocket
            .send(Message::Text(
                serde_json::json!({
                    "id": attach["id"],
                    "result": {"sessionId": "late-popup-session"}
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let detach = tokio::time::timeout(Duration::from_secs(1), websocket.next())
            .await
            .expect("late attachment was not detached")
            .unwrap()
            .unwrap();
        let detach: Value = match detach {
            Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
            _ => panic!("expected text CDP request"),
        };
        assert_eq!(detach["method"], "Target.detachFromTarget");
        assert_eq!(detach["params"]["sessionId"], "late-popup-session");
        websocket
            .send(Message::Text(
                serde_json::json!({"id": detach["id"], "result": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    });
    let cdp = CdpClient::connect(&format!("ws://{address}"))
        .await
        .unwrap();
    let session = test_session(cdp.clone());
    {
        let attach = session.attach_popup("popup");
        tokio::pin!(attach);
        tokio::select! {
            _ = &mut attach => panic!("attach unexpectedly completed"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }
    server.await.unwrap();
    cdp.close().await;
}

#[test]
fn popup_topology_rejects_missing_witness_preexisting_and_unrelated_targets() {
    let topology = popup_topology_with(vec![("unrelated", None, 11)]);
    assert_eq!(
        assess_popup_topology(&popup_test_snapshot(), &topology, false)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::WitnessMissing
    );
    assert_eq!(
        assess_popup_topology(
            &popup_test_snapshot(),
            &popup_topology_with(vec![("preexisting", Some("original"), 12)]),
            true,
        )
        .unwrap_err()
        .kind,
        PopupClickErrorKind::PopupMissing
    );
    assert_eq!(
        assess_popup_topology(&popup_test_snapshot(), &topology, true)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::PopupOpenerMismatch
    );
}

#[test]
fn popup_topology_rejects_wrong_opener_ambiguity_lag_and_destroyed_target() {
    let snapshot = popup_test_snapshot();
    let wrong = popup_topology_with(vec![("popup", Some("other"), 11)]);
    assert_eq!(
        assess_popup_topology(&snapshot, &wrong, true)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::PopupOpenerMismatch
    );
    let ambiguous = popup_topology_with(vec![
        ("popup-1", Some("original"), 11),
        ("popup-2", Some("original"), 12),
    ]);
    assert_eq!(
        assess_popup_topology(&snapshot, &ambiguous, true)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::PopupAmbiguous
    );
    let mut lagged = popup_topology_with(vec![("popup", Some("original"), 11)]);
    lagged.event_loss_count = 1;
    assert_eq!(
        assess_popup_topology(&snapshot, &lagged, true)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::TopologyLagged
    );
    let mut destroyed = TopologyRegistry::default();
    destroyed.destroyed_targets.push_back(DestroyedPageTarget {
        target: popup_test_target("popup", Some("original")),
        observed_sequence: 11,
    });
    assert_eq!(
        assess_popup_topology(&snapshot, &destroyed, true)
            .unwrap_err()
            .kind,
        PopupClickErrorKind::PopupDestroyed
    );
}

#[tokio::test]
async fn popup_readiness_maps_protocol_failure_to_typed_unreadable_error() {
    let protocol_error: crate::browser::cdp::CdpError = serde_json::from_value(serde_json::json!({
        "code": -32000,
        "message": "target closed"
    }))
    .unwrap();
    let error = popup_verification_call(async { Err(protocol_error) }, "readiness")
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<PopupClickError>().unwrap().kind,
        PopupClickErrorKind::PopupUnreadable
    );
}

#[cfg(feature = "visual-compare")]
fn comparison_png(width: u32, height: u32, pixels: &[u8]) -> String {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    STANDARD.encode(encoded)
}

#[cfg(feature = "visual-compare")]
#[test]
fn compares_equal_sized_pngs_with_exact_difference_bounds() {
    let first = comparison_png(2, 2, &[0; 16]);
    let mut changed_pixels = [0; 16];
    changed_pixels[4..8].copy_from_slice(&[255, 0, 0, 255]);
    let second = comparison_png(2, 2, &changed_pixels);

    let comparison = compare_png_visuals(&first, &second).unwrap();
    assert_eq!(comparison.changed_pixels, 1);
    assert_eq!(comparison.changed_ratio, 0.25);
    let bounds = comparison.difference_box.unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (1.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn topology_error_maps_kind_to_correct_recovery_hint() {
    use super::TopologyErrorKind;
    use super::TopologyRecoveryHint;

    let cases: &[(TopologyErrorKind, TopologyRecoveryHint)] = &[
        (
            TopologyErrorKind::NoTargetSelected,
            TopologyRecoveryHint::ListTargets,
        ),
        (
            TopologyErrorKind::StaleTarget,
            TopologyRecoveryHint::ListTargets,
        ),
        (
            TopologyErrorKind::StaleFrame,
            TopologyRecoveryHint::ListFrames,
        ),
        (
            TopologyErrorKind::NoSuchFrame,
            TopologyRecoveryHint::ListFrames,
        ),
        (
            TopologyErrorKind::NoPageSession,
            TopologyRecoveryHint::Reconnect,
        ),
        (
            TopologyErrorKind::BudgetExceeded,
            TopologyRecoveryHint::ReObserve,
        ),
        (
            TopologyErrorKind::RoutingLost,
            TopologyRecoveryHint::Reconnect,
        ),
    ];

    for (kind, expected_hint) in cases {
        let error = super::TopologyError::new(*kind, "test");
        assert_eq!(error.kind, *kind);
        assert_eq!(error.recovery, *expected_hint);
        assert!(!error.message.is_empty());
    }
}

#[test]
fn topology_error_display_includes_kind_and_recovery() {
    let error = super::TopologyError::new(
        super::TopologyErrorKind::NoTargetSelected,
        "no active target",
    );
    let display = error.to_string();
    assert!(display.contains("NoTargetSelected"));
    assert!(display.contains("no active target"));
    assert!(display.contains("ListTargets"));
}
