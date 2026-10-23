use super::*;

#[test]
fn content_process_navigation_budget_matches_bounded_browser_callers() {
    assert_eq!(
        CONTENT_PROCESS_LOAD_TIMEOUT,
        Duration::from_secs(60),
        "the content owner must not time out before a caller's explicit 60s navigation budget"
    );
    assert_eq!(
        CONTENT_PROCESS_ASYNC_EFFECT_TIMEOUT,
        Duration::from_secs(30),
        "increasing document navigation time must not expand independent async network effects"
    );
}

#[test]
fn content_process_error_response_preserves_resource_limit_fields() {
    let response = content_error_response(
        Value::from(17),
        NativeEngineError::limit("CSS style rules", 4_096, 4_097),
    );
    assert_eq!(response["kind"], "error");
    assert_eq!(response["id"], 17);
    assert_eq!(response["error_type"], "resource_limit");
    assert_eq!(response["resource"], "CSS style rules");
    assert_eq!(response["limit"], 4_096);
    assert_eq!(response["actual"], 4_097);
    assert_eq!(
        response["reason"],
        "CSS style rules exceeds limit 4096 (actual 4097)"
    );
}

#[test]
fn parent_cookie_authority_is_selected_by_network_target() {
    for document_url in [
        "https://app.test/page",
        "file:///trusted/page.html",
        "data:text/html,local%20page",
        "fixture://app.test/page",
    ] {
        assert!(
            is_parent_owned_network_target(document_url, "https://api.test/resource"),
            "network target from {document_url} must use the parent cookie authority"
        );
    }

    assert!(is_parent_owned_network_target(
        "https://app.test/page",
        "/resource"
    ));
    assert!(!is_parent_owned_network_target(
        "file:///trusted/page.html",
        "./resource.js"
    ));
    assert!(!is_parent_owned_network_target(
        "fixture://app.test/page",
        "./resource.js"
    ));
}

#[tokio::test]
async fn content_ipc_reader_routes_dialog_decisions_without_stealing_requests() {
    struct ChannelReader {
        receiver: std::sync::mpsc::Receiver<Vec<u8>>,
        pending: VecDeque<u8>,
    }

    impl Read for ChannelReader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if output.is_empty() {
                return Ok(0);
            }
            while self.pending.is_empty() {
                match self.receiver.recv() {
                    Ok(chunk) => self.pending.extend(chunk),
                    Err(_) => return Ok(0),
                }
            }
            let count = output.len().min(self.pending.len());
            for byte in &mut output[..count] {
                *byte = self.pending.pop_front().expect("pending frame byte");
            }
            Ok(count)
        }
    }

    fn send_frame(sender: &std::sync::mpsc::Sender<Vec<u8>>, payload: &[u8]) {
        let mut frame = Vec::new();
        write_sync_frame(&mut frame, payload).unwrap();
        sender.send(frame).unwrap();
    }

    let (input_tx, input_rx) = std::sync::mpsc::channel();
    let reader = ChannelReader {
        receiver: input_rx,
        pending: VecDeque::new(),
    };
    let (request_tx, mut request_rx) = mpsc::channel(1);
    let (dialog_tx, dialog_rx) = std::sync::mpsc::channel();
    let dialog_waiting = Arc::new(AtomicBool::new(false));
    let reader_waiting = Arc::clone(&dialog_waiting);
    let sync_xhr_replies = Arc::new(Mutex::new(BTreeMap::new()));
    let (sync_reply_tx, sync_reply_rx) = std::sync::mpsc::sync_channel(1);
    sync_xhr_replies.lock().unwrap().insert(1, sync_reply_tx);
    let reader_sync_xhr_replies = Arc::clone(&sync_xhr_replies);
    let reader_thread = std::thread::spawn(move || {
        forward_content_ipc_frames(
            reader,
            request_tx,
            reader_waiting,
            dialog_tx,
            reader_sync_xhr_replies,
        );
    });

    let request = serde_json::to_vec(&json!({"kind": "ping", "id": 7})).unwrap();
    send_frame(&input_tx, &request);
    assert_eq!(request_rx.recv().await.unwrap().unwrap(), request);

    dialog_waiting.store(true, Ordering::Release);
    let decision = serde_json::to_vec(&json!({
        "kind": "dialog_decision",
        "request_id": 7,
        "dialog_id": 3,
        "accepted": true,
        "prompt_value": "approved",
    }))
    .unwrap();
    send_frame(&input_tx, &decision);
    let routed = dialog_rx.recv().unwrap().unwrap();
    assert_eq!(routed.request_id, 7);
    assert_eq!(routed.dialog_id, 3);
    assert!(routed.accepted);
    assert_eq!(routed.prompt_value.as_deref(), Some("approved"));

    let unknown_xhr_response =
        serde_json::to_vec(&json!({"kind": "parent_sync_xhr_response", "sync_id": 99})).unwrap();
    send_frame(&input_tx, &unknown_xhr_response);
    assert!(request_rx.recv().await.unwrap().is_err());
    assert!(
        sync_reply_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_err()
    );
    assert!(sync_xhr_replies.lock().unwrap().is_empty());

    drop(input_tx);
    reader_thread.join().unwrap();
}

#[test]
fn content_async_effect_notifications_validate_sequence_and_owner() {
    let frame = |sequence| {
        json!({
            "kind": "async_effects_ready",
            "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            "sequence": sequence,
            "context_id": "target-7",
            "frame_id": "target-7:frame-2",
        })
    };
    let accepted =
        decode_async_effects_ready(&frame(1), 1, Some("target-7"), Some("target-7:frame-2"))
            .unwrap()
            .expect("event-ready frame");
    assert_eq!(accepted.sequence, 1);
    assert_eq!(accepted.context_id, "target-7");
    assert_eq!(accepted.frame_id, "target-7:frame-2");

    assert!(
        decode_async_effects_ready(&frame(2), 1, Some("target-7"), Some("target-7:frame-2"),)
            .is_err()
    );
    assert!(
        decode_async_effects_ready(&frame(1), 1, Some("other-target"), Some("target-7:frame-2"),)
            .is_err()
    );
    let mut unknown_field = frame(1);
    unknown_field["effects"] = json!([]);
    assert!(
        decode_async_effects_ready(
            &unknown_field,
            1,
            Some("target-7"),
            Some("target-7:frame-2"),
        )
        .is_err()
    );
    assert!(
        decode_async_effects_ready(
            &json!({"kind":"pong"}),
            1,
            Some("target-7"),
            Some("target-7:frame-2"),
        )
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn content_async_effect_notification_is_single_flight_until_script_ack() {
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    let mut notification_pending = false;
    let mut next_sequence = 1;
    write_async_effects_ready(
        &mut writer,
        true,
        &mut notification_pending,
        &mut next_sequence,
        "target-7",
        "target-7:frame-2",
    )
    .await
    .unwrap();
    write_async_effects_ready(
        &mut writer,
        true,
        &mut notification_pending,
        &mut next_sequence,
        "target-7",
        "target-7:frame-2",
    )
    .await
    .unwrap();

    let first: Value = serde_json::from_slice(&read_frame(&mut reader).await.unwrap()).unwrap();
    assert_eq!(first["sequence"], 1);
    assert_eq!(next_sequence, 2);
    assert!(notification_pending);
    assert!(
        timeout(Duration::from_millis(5), read_frame(&mut reader))
            .await
            .is_err()
    );

    notification_pending = false;
    write_async_effects_ready(
        &mut writer,
        true,
        &mut notification_pending,
        &mut next_sequence,
        "target-7",
        "target-7:frame-2",
    )
    .await
    .unwrap();
    let second: Value = serde_json::from_slice(&read_frame(&mut reader).await.unwrap()).unwrap();
    assert_eq!(second["sequence"], 2);
    assert_eq!(next_sequence, 3);
}

#[tokio::test]
async fn content_async_effect_output_reader_preserves_bounded_frame_order() {
    let (mut writer, reader) = tokio::io::duplex(128);
    let (sender, mut receiver) = mpsc::channel(MAX_CONTENT_PROCESS_OUTPUT_FRAMES);
    let event_notify = Arc::new(tokio::sync::Notify::new());
    let reader_task = tokio::spawn(forward_content_process_output(
        reader,
        sender,
        Some(Arc::clone(&event_notify)),
    ));

    write_frame(&mut writer, b"first").await.unwrap();
    write_frame(&mut writer, b"second").await.unwrap();
    write_frame(&mut writer, b"third").await.unwrap();
    timeout(Duration::from_secs(1), event_notify.notified())
        .await
        .unwrap();
    assert_eq!(receiver.len(), MAX_CONTENT_PROCESS_OUTPUT_FRAMES);
    assert_eq!(receiver.recv().await.unwrap().unwrap(), b"first");
    assert_eq!(receiver.recv().await.unwrap().unwrap(), b"second");
    assert_eq!(receiver.recv().await.unwrap().unwrap(), b"third");
    drop(receiver);
    drop(writer);
    timeout(Duration::from_secs(1), reader_task)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn content_async_effect_queue_is_bounded_and_observes_queued_messages() {
    let mut worker_messages = VecDeque::new();
    let no_effects = content_worker_effects_pending(
        &worker_messages,
        &VecDeque::new(),
        &VecDeque::new(),
        &VecDeque::new(),
        &VecDeque::new(),
        &VecDeque::new(),
        &VecDeque::new(),
        &VecDeque::new(),
        &[],
    )
    .unwrap();
    assert!(!no_effects);

    worker_messages.push_back(NativeWorkerMessage {
        worker_id: 1,
        data: json!("ready"),
        error: None,
        transfer_ports: Vec::new(),
        object_urls: Vec::new(),
    });
    assert!(
        content_worker_effects_pending(
            &worker_messages,
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &[],
        )
        .unwrap()
    );

    let message = NativeWorkerMessage {
        worker_id: 1,
        data: json!("bounded"),
        error: None,
        transfer_ports: Vec::new(),
        object_urls: Vec::new(),
    };
    worker_messages.clear();
    for _ in 0..=MAX_NATIVE_WORKER_MESSAGES {
        worker_messages.push_back(message.clone());
    }
    assert!(
        content_worker_effects_pending(
            &worker_messages,
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &VecDeque::new(),
            &[],
        )
        .is_err()
    );
}

#[test]
fn fetch_commands_accept_only_the_private_font_and_module_destinations() {
    let command = |destination: Option<&str>| NativeScriptCommand::Fetch {
        request_id: 1,
        worker_id: None,
        href: "https://fonts.test/example.woff2".into(),
        credentials: true,
        credentials_mode: None,
        referrer: Some("about:client".into()),
        referrer_url: Some("https://fonts.test/page".into()),
        referrer_policy: Some("".into()),
        method: "GET".into(),
        headers: BTreeMap::new(),
        body: None,
        body_base64: None,
        content_type: None,
        mode: Some("cors".into()),
        redirect: Some("follow".into()),
        cache: Some("default".into()),
        timeout_ms: None,
        upload_stream_id: None,
        destination: destination.map(str::to_owned),
        module_referrer: None,
        module_type: None,
    };

    let mut font = fetch_commands(&[command(Some("font"))]).unwrap();
    assert!(font.pop_front().is_some_and(|request| request.16));
    let mut ordinary = fetch_commands(&[command(None)]).unwrap();
    assert!(!ordinary.pop_front().unwrap().16);
    assert!(fetch_commands(&[command(Some("image"))]).is_err());
    assert!(fetch_commands(&[command(Some("module"))]).is_err());

    let mut module = command(Some("module"));
    if let NativeScriptCommand::Fetch {
        module_referrer, ..
    } = &mut module
    {
        *module_referrer = Some("https://page.test/app.js".into());
    }
    let mut module_requests = fetch_commands(&[module]).unwrap();
    let module_request = module_requests.pop_front().unwrap();
    assert!(module_request.18);
    assert_eq!(module_request.19, None);

    let mut json_module = command(Some("module"));
    if let NativeScriptCommand::Fetch {
        module_referrer,
        module_type,
        ..
    } = &mut json_module
    {
        *module_referrer = Some("https://page.test/app.js".into());
        *module_type = Some(NativeModuleResourceType::Json);
    }
    assert_eq!(
        fetch_commands(&[json_module])
            .unwrap()
            .pop_front()
            .unwrap()
            .19,
        Some(NativeModuleResourceType::Json)
    );

    let mut invalid_destination = command(None);
    if let NativeScriptCommand::Fetch { module_type, .. } = &mut invalid_destination {
        *module_type = Some(NativeModuleResourceType::Json);
    }
    assert!(fetch_commands(&[invalid_destination]).is_err());
}

#[test]
fn dynamic_import_map_updates_a_local_runtime_without_a_resource_loader() {
    let mut document = NativeDocument::parse(
        "<html><head></head><body></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("local dynamic import-map document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("local-dynamic-import-map")
        .expect("native JavaScript runtime must construct");
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                    const map = document.createElement('script');
                    map.type = 'importmap';
                    map.textContent = '{"imports":{"pkg":"/mapped.js"}}';
                    document.head.appendChild(map);
                    return true;
                })()"#,
            &document,
            "https://example.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("dynamic import map must attach in the local runtime");
    document
        .apply_script_commands(&evaluation.commands)
        .expect("dynamic import-map attachment must commit");
    let sources = document.take_newly_attached_page_script_sources(
        &evaluation.commands,
        super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
        MAX_NATIVE_SCRIPT_BYTES,
    );

    let result = execute_dynamic_page_scripts_without_loader(
        &mut document,
        &runtime,
        sources,
        "https://example.test/",
        &NativeOrigin::Opaque,
        Viewport::default(),
    )
    .expect("local dynamic import map must be registered");
    assert!(result.events.is_empty());
    assert_eq!(
        runtime
            .module_import_map()
            .expect("runtime import map must remain readable")
            .resolve("https://example.test/main.js", "pkg")
            .expect("dynamically mapped bare specifier must resolve"),
        "https://example.test/mapped.js"
    );
}

#[tokio::test]
async fn refresh_health_detects_an_exited_content_worker() {
    let mut process = NativeContentProcess::spawn(
        None,
        &[],
        NativeDialogControlPlane::default(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    process.child.start_kill().unwrap();
    process.child.wait().await.unwrap();

    assert!(!process.refresh_health());
    assert_eq!(
        process.failure_kind(),
        Some(NativeWorkerFailureKind::Exited)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn timed_out_content_load_terminates_and_reaps_the_busy_worker() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = tokio::io::BufReader::new(stream);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                break;
            }
        }
        let mut stream = reader.into_inner();
        let body = b"<!doctype html><script>while (true) {}</script>";
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    });

    let mut process = NativeContentProcess::spawn(
        None,
        &[],
        NativeDialogControlPlane::default(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    process
        .start(
            None,
            &[],
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            "",
            None,
            "",
            "",
            None,
            &NativeEnvironmentOverrides::default(),
            &[],
            false,
        )
        .await
        .unwrap();
    let navigation = NativeNavigationRequest::get(format!("http://{address}/busy"));
    let mut parent_loader =
        NativeResourceLoader::new(&super::super::config::NativeEngineConfig::default()).unwrap();
    let started = tokio::time::Instant::now();
    let result = process
        .load_with_deadline(
            &navigation,
            &NativeEngineLimits::default(),
            Viewport::default(),
            None,
            super::super::resource_loader::NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            &[],
            None,
            &mut parent_loader,
            Duration::from_millis(250),
        )
        .await;

    assert!(
        result.is_err(),
        "busy script must exceed the navigation deadline"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        !process.refresh_health(),
        "timed-out worker must be unhealthy"
    );
    assert_eq!(
        process.failure_kind(),
        Some(NativeWorkerFailureKind::Exited),
        "timed-out worker must be killed and reaped before browser.stop"
    );
    tokio::time::timeout(Duration::from_secs(2), fixture)
        .await
        .expect("local HTTP fixture did not finish")
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn exited_content_worker_releases_a_suspended_modal_dialog() {
    use tokio::io::AsyncBufReadExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;

        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = tokio::io::BufReader::new(stream);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                break;
            }
        }
        let mut stream = reader.into_inner();
        let body = b"<!doctype html><body><script>alert('worker failure');</script></body>";
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    });
    let control = NativeDialogControlPlane::for_modal_owner();
    let mut process =
        NativeContentProcess::spawn(None, &[], control.clone(), Arc::new(Mutex::new(Vec::new())))
            .await
            .unwrap();
    process
        .start(
            None,
            &[],
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            "",
            None,
            "",
            "",
            None,
            &NativeEnvironmentOverrides::default(),
            &[],
            false,
        )
        .await
        .unwrap();
    let worker_pid = process
        .child
        .id()
        .expect("content worker should have a PID");
    let navigation = NativeNavigationRequest::get(format!("http://{address}/dialog"));
    let mut parent_loader =
        NativeResourceLoader::new(&super::super::config::NativeEngineConfig::default()).unwrap();
    let mut pending_load = tokio::spawn(async move {
        process
                .load(
                    &navigation,
                    &NativeEngineLimits::default(),
                    Viewport::default(),
                    None,
                    super::super::resource_loader::NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
                    super::super::browsing_context::NATIVE_CONTEXT_ID,
                    &[],
                    None,
                    &mut parent_loader,
                )
                .await
    });
    let pending = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(pending) = control.pending().unwrap() {
                break pending;
            }
            if pending_load.is_finished() {
                match (&mut pending_load).await {
                    Ok(Ok(_)) => panic!("page load completed without publishing its modal"),
                    Ok(Err(error)) => {
                        panic!("page load failed without publishing its modal: {error}")
                    }
                    Err(error) => panic!("page load task panicked before its modal: {error}"),
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("worker did not publish its modal dialog");
    assert_eq!(pending.dialog.message, "worker failure");
    assert_eq!(
        unsafe { libc::kill(worker_pid as libc::pid_t, libc::SIGKILL) },
        0
    );
    let load_result = tokio::time::timeout(Duration::from_secs(5), pending_load)
        .await
        .expect("worker exit did not release the suspended dialog")
        .expect("content load task panicked");
    let failure = match load_result {
        Err(failure) => failure,
        Ok(_) => panic!("navigation succeeded after its content worker exited"),
    };
    assert!(failure.to_string().contains("content process"));
    assert!(control.pending().unwrap().is_none());
    fixture.await.expect("HTTP dialog fixture task panicked");
}

#[tokio::test]
async fn modal_dialog_wait_pauses_the_content_operation_deadline() {
    use tokio::io::AsyncBufReadExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;

        let bodies: &[&[u8]] = &[
            b"<!doctype html><title>warm</title><p>ready</p>",
            br#"<!doctype html><title>modal-deadline</title><body><script>
                    alert("deadline pause");
                    window.dialogDeadlineResumed = true;
                </script></body>"#,
        ];
        for body in bodies {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let mut stream = reader.into_inner();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        }
    });
    let control = NativeDialogControlPlane::for_modal_owner();
    let mut parent_loader =
        NativeResourceLoader::new(&super::super::config::NativeEngineConfig::default()).unwrap();
    let mut process =
        NativeContentProcess::spawn(None, &[], control.clone(), Arc::new(Mutex::new(Vec::new())))
            .await
            .unwrap();
    process
        .start(
            None,
            &[],
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            "",
            None,
            "",
            "",
            None,
            &NativeEnvironmentOverrides::default(),
            &[],
            false,
        )
        .await
        .unwrap();
    process
        .load(
            &NativeNavigationRequest::get(format!("http://{address}/warm")),
            &NativeEngineLimits::default(),
            Viewport::default(),
            None,
            super::super::resource_loader::NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            super::super::browsing_context::NATIVE_CONTEXT_ID,
            &[],
            None,
            &mut parent_loader,
        )
        .await
        .unwrap();
    process.commit().await.unwrap();
    let page_events = NativePageEventBatch::default();
    let mut pending_evaluation = tokio::spawn(async move {
        let result = process
            .evaluate_with_page_events("confirm('explicit evaluation')", &page_events, None)
            .await;
        (process, result)
    });
    let evaluation_dialog = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(pending) = control.pending().unwrap() {
                break pending;
            }
            if pending_evaluation.is_finished() {
                let (_process, result) = (&mut pending_evaluation)
                    .await
                    .expect("explicit content evaluation task should not panic");
                match result {
                    Ok(result) => panic!(
                        "explicit content evaluation completed without a modal dialog: {:?}",
                        result.value
                    ),
                    Err(error) => {
                        panic!("explicit content evaluation failed without a modal dialog: {error}")
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("explicit content evaluation did not publish its modal dialog");
    assert_eq!(evaluation_dialog.dialog.message, "explicit evaluation");
    control.resolve(&evaluation_dialog.id, true, None).unwrap();
    let (mut process, evaluation) =
        tokio::time::timeout(Duration::from_secs(5), &mut pending_evaluation)
            .await
            .expect("explicit content evaluation did not resume")
            .expect("explicit content evaluation task panicked");
    assert_eq!(evaluation.unwrap().value, serde_json::json!(true));
    let navigation = NativeNavigationRequest::get(format!("http://{address}/dialog"));
    let limits = NativeEngineLimits::default();
    let mut pending_load = tokio::spawn(async move {
        process
                .load_with_deadline(
                    &navigation,
                    &limits,
                    Viewport::default(),
                    None,
                    super::super::resource_loader::NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
                    super::super::browsing_context::NATIVE_CONTEXT_ID,
                    &[],
                    None,
                    &mut parent_loader,
                    Duration::from_secs(20),
                )
                .await
    });
    let pending_dialog = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(pending) = control.pending().unwrap() {
                break pending;
            }
            if pending_load.is_finished() {
                match (&mut pending_load).await {
                    Ok(Ok(_)) => {
                        panic!("page load completed successfully without publishing a modal")
                    }
                    Ok(Err(error)) => {
                        panic!("page load failed without publishing a modal: {error}")
                    }
                    Err(error) => panic!("page load task panicked: {error}"),
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("page load did not publish its modal dialog");
    assert_eq!(pending_dialog.dialog.message, "deadline pause");
    tokio::time::sleep(Duration::from_millis(25_000)).await;
    assert!(
        !pending_load.is_finished(),
        "operation deadline expired while a human decision was pending"
    );
    assert_eq!(
        control
            .pending()
            .unwrap()
            .as_ref()
            .map(|pending| pending.id.as_str()),
        Some(pending_dialog.id.as_str())
    );
    control.resolve(&pending_dialog.id, true, None).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), pending_load)
        .await
        .expect("page load did not resume after accepting its dialog")
        .expect("script task panicked")
        .expect("page load failed after its dialog was accepted");
    assert!(matches!(result, NativeContentLoadResult::Loaded(_)));
    fixture.await.expect("HTTP deadline fixture task panicked");
    assert!(control.pending().unwrap().is_none());
}

#[tokio::test]
async fn content_process_font_destination_uses_the_native_font_loader() {
    #[cfg(not(target_os = "linux"))]
    return;

    let font_bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let root = std::env::temp_dir().join(format!(
        "glass-native-font-destination-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("font test root must be created");
    let font_path = root.join("face.ttf");
    std::fs::write(&font_path, font_bytes).expect("font test fixture must be written");
    std::fs::write(
        root.join("index.html"),
        "<!doctype html><title>Fonts</title>",
    )
    .expect("font test document must exist for rooted-file policy checks");
    let document_url = Url::from_file_path(root.join("index.html"))
        .expect("font test document URL must be created")
        .to_string();
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-destination-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                  const face = new FontFace("File Face", "url(face.ttf)");
                  document.fonts.add(face);
                  face.load();
                  return true;
                })()"#,
            &document,
            &document_url,
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("file FontFace source must evaluate");
    assert!(evaluation.commands.iter().any(|command| matches!(
        command,
        NativeScriptCommand::Fetch {
            destination: Some(destination),
            ..
        } if destination == "font"
    )));

    let limits = NativeEngineLimits::default();
    let mut loader = NativeResourceLoader::for_content_process(
        limits.max_document_bytes,
        None,
        std::slice::from_ref(&root),
    )
    .expect("font test loader must construct");
    let mut service_workers = NativeServiceWorkerRegistry::default();
    let mut websocket_connections = BTreeMap::new();
    let mut fetch_stream_connections = BTreeMap::new();
    let mut fetch_upload_connections = BTreeMap::new();
    let mut event_source_connections = BTreeMap::new();
    let (next, _mutation, _resolved_value) = resolve_script_fetches(
        &document,
        &runtime,
        Some(&mut loader),
        None,
        &mut service_workers,
        &mut websocket_connections,
        &mut fetch_stream_connections,
        &mut fetch_upload_connections,
        &mut event_source_connections,
        &document_url,
        &NativeOrigin::Opaque,
        Viewport::default(),
        false,
        evaluation,
    )
    .await
    .expect("content-process font destination must resolve");
    assert_eq!(next.to_content_wire().font_resources.len(), 1);
    assert_eq!(next.to_content_wire().font_resources[0].family, "File Face");
    std::fs::remove_dir_all(root).expect("font test root must be removed");
}
