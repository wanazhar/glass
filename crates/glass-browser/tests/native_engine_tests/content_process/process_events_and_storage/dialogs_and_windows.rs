use super::*;

#[test]
fn native_content_process_beforeunload_waits_for_exact_user_decision() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
            let _guard = native_content_process_test_lock().lock().await;
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (next_request_tx, mut next_request_rx) = oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().nth(1), Some("/start"));
                let body = "<script>addEventListener('pagehide', () => globalThis.pagehideRan = 'yes'); addEventListener('unload', () => globalThis.unloadRan = 'yes');</script><button id='activate'>Activate</button><title>Start</title>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();

                let (mut stream, _) = listener.accept().await.unwrap();
                let _ = next_request_tx.send(());
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().nth(1), Some("/next"));
                let body = "<title>Next</title>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });

            let session = Arc::new(
                BrowserRuntimeSession::connect_native_with_modal_dialogs(
                    NativeEngineConfig::default(),
                )
                .await
                .unwrap(),
            );
            let controller = session.native_dialog_controller().unwrap();
            let start_url = format!("http://{address}/start");
            session.navigate(start_url.clone()).await.unwrap();
            session
                .action(SemanticAction::Click {
                    target: "id=activate".into(),
                })
                .await
                .unwrap();
            assert_eq!(
                session
                    .script(native_beforeunload_return_semantics_script())
                    .await
                    .unwrap()
                    .value,
                serde_json::json!([
                    [false, true, true, "42", true],
                    [true, true, true, "", false],
                    [true, true, true, "", false],
                    [false, true, true, "", true],
                    [false, true, true, "false", true],
                    [false, true, true, "preserved", true],
                    [false, true, true, "23", true],
                    true,
                    true,
                    [false, true, true, "7", true],
                    true,
                    [true, false],
                    [false, true],
                ])
            );

            let first_navigation_session = Arc::clone(&session);
            let first_navigation_url = format!("http://{address}/next");
            let first_navigation = tokio::task::spawn_local(async move {
                first_navigation_session
                    .navigate(first_navigation_url)
                    .await
            });
            let first_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                request = &mut next_request_rx => panic!("replacement resource requested before the user decision: {request:?}"),
            };
            assert!(first_prompt.dialog.message.is_empty());
            assert!(first_prompt.dialog.default_value.is_none());
            assert!(!first_navigation.is_finished());
            controller
                .resolve_dialog(
                    &first_prompt.id,
                    NativeDialogResolution {
                        accepted: false,
                        prompt_value: None,
                    },
                )
                .unwrap();
            let outcome = first_navigation.await.unwrap().unwrap();
            assert_eq!(outcome.url, start_url);
            assert_eq!(
                session
                    .script("[globalThis.warningRan || null, globalThis.pagehideRan || null, globalThis.unloadRan || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!(["yes", null, null])
            );
            assert!(tokio::time::timeout(Duration::from_millis(50), &mut next_request_rx)
                .await
                .is_err());

            let second_navigation_session = Arc::clone(&session);
            let second_navigation_url = format!("http://{address}/next");
            let second_navigation = tokio::task::spawn_local(async move {
                second_navigation_session
                    .navigate(second_navigation_url)
                    .await
            });
            let second_prompt = wait_for_modal_dialog(&controller, "beforeunload").await;
            assert_ne!(first_prompt.id, second_prompt.id);
            controller
                .resolve_dialog(
                    &second_prompt.id,
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .unwrap();
            let outcome = second_navigation.await.unwrap().unwrap();
            assert_eq!(outcome.url, format!("http://{address}/next"));
            next_request_rx.await.unwrap();
            assert!(controller.pending_dialog().unwrap().is_none());
            Arc::try_unwrap(session)
                .unwrap_or_else(|_| panic!("native session still has unexpected owners"))
                .close()
                .await
                .unwrap();
            server.await.unwrap();
                })
                .await;
        });
    });
}
