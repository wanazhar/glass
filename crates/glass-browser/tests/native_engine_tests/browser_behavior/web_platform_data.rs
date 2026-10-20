use super::*;

#[tokio::test]
async fn native_array_buffer_transfer_detaches_sender_and_preserves_views() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://array-buffer-transfer-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://array-buffer-transfer-worker",
            r#"self.onmessage = event => {
                const buffer = event.data.buffer;
                const view = event.data.view;
                postMessage({
                    buffer: [buffer instanceof ArrayBuffer, buffer.byteLength, Array.from(new Uint8Array(buffer))],
                    view: [view instanceof Uint8Array, view.byteLength, Array.from(view)],
                    shared: view.buffer === buffer,
                });
            }"#,
        )
        .unwrap()
        .with_initial_url("fixture://array-buffer-transfer-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.workerMessages = [];
                    globalThis.worker = new Worker('fixture://array-buffer-transfer-worker');
                    worker.onmessage = event => workerMessages.push(event.data);
                    const buffer = new ArrayBuffer(5);
                    new Uint8Array(buffer).set([10, 20, 30, 40, 50]);
                    const view = new Uint8Array(buffer, 1, 2);
                    worker.postMessage({ buffer, view }, [buffer]);
                    globalThis.senderTransfer = [buffer.byteLength, view.byteLength];
                    let invalidTransferError = '';
                    const untouched = new ArrayBuffer(2);
                    try { worker.postMessage({ value: 1 }, [new Uint8Array(untouched)]); }
                    catch (error) { invalidTransferError = error.name; }
                    const structuredSource = new ArrayBuffer(3);
                    new Uint8Array(structuredSource).set([9, 8, 7]);
                    const structuredResult = structuredClone(
                        { buffer: structuredSource },
                        { transfer: [structuredSource] },
                    );
                    globalThis.structuredCloneTransfer = {
                        source: structuredSource.byteLength,
                        clone: [structuredResult.buffer instanceof ArrayBuffer,
                            Array.from(new Uint8Array(structuredResult.buffer))],
                    };
                    const emptySource = new ArrayBuffer(0);
                    const emptyClone = structuredClone(
                        emptySource,
                        { transfer: [emptySource] },
                    );
                    globalThis.emptyTransfer = [emptySource.byteLength, emptyClone.byteLength];
                    let detachedTransferError = '';
                    try { worker.postMessage({}, [buffer]); }
                    catch (error) { detachedTransferError = error.name; }
                    globalThis.transferErrors = [
                        invalidTransferError,
                        untouched.byteLength,
                        detachedTransferError,
                    ];
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "({ senderTransfer, structuredCloneTransfer, emptyTransfer, transferErrors, workerMessages })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "senderTransfer": [0, 0],
            "structuredCloneTransfer": {
                "source": 0,
                "clone": [true, [9, 8, 7]],
            },
            "emptyTransfer": [0, 0],
            "transferErrors": ["DataCloneError", 2, "DataCloneError"],
            "workerMessages": [{
                "buffer": [true, 5, [10, 20, 30, 40, 50]],
                "view": [true, 2, [20, 30]],
                "shared": true,
            }],
        })
    );
    engine.close_async().await.unwrap();
}
#[test]
fn data_url_and_limits_are_bounded() {
    std::thread::Builder::new()
        .name("native-data-url-limits-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("data URL limits runtime should build");
            runtime.block_on(data_url_and_limits_are_bounded_inner());
        })
        .expect("data URL limits test thread should start")
        .join()
        .expect("data URL limits test thread should complete");
}
