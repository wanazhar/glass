//! Helper process used by the native engine. Keep it next to `glass`
//! and `glass-browser`. The startup path matches the `glass-browser`
//! `glass-native-content-worker` binary so both packages ship the same helper.
const NATIVE_CONTENT_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

fn spawn_native_content_worker<T: Send + 'static>(
    run: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<T>> {
    std::thread::Builder::new()
        .name("glass-native-content-worker".to_owned())
        .stack_size(NATIVE_CONTENT_WORKER_STACK_BYTES)
        .spawn(run)
}

fn report_worker_error(error: glass_browser::browser::native_engine::NativeEngineError) {
    if std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").is_some() {
        eprintln!(
            "native content worker exited with diagnostic class: {}",
            error.diagnostic_class()
        );
        if let Some(operation_class) = error.diagnostic_operation_class() {
            eprintln!("native content worker operation class: {operation_class}");
        }
        if let Some(reason_class) = error.diagnostic_reason_class() {
            eprintln!("native content worker reason class: {reason_class}");
        }
        if let Some((resource, limit, actual)) = error.diagnostic_limit() {
            eprintln!(
                "native content worker resource limit: {resource:?} (limit {limit}, actual {actual})"
            );
        }
    }
}

fn main() {
    let worker = spawn_native_content_worker(|| {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => {
                eprintln!("native content worker runtime could not be initialized");
                std::process::exit(1);
            }
        };
        runtime.block_on(glass_browser::browser::native_engine::run_native_content_worker())
    });

    let result = match worker {
        Ok(worker) => match worker.join() {
            Ok(result) => result,
            Err(_) => {
                eprintln!("native content worker thread terminated unexpectedly");
                std::process::exit(1);
            }
        },
        Err(_) => {
            eprintln!("native content worker thread could not be started");
            std::process::exit(1);
        }
    };

    if let Err(error) = result {
        report_worker_error(error);
        std::process::exit(1);
    }
}
