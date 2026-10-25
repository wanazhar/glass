#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = glass_browser::browser::native_engine::run_native_content_worker().await {
        if std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").is_some() {
            eprintln!(
                "native content worker exited with diagnostic class: {}",
                error.diagnostic_class()
            );
            if let Some((resource, limit, actual)) = error.diagnostic_limit() {
                eprintln!(
                    "native content worker resource limit: {resource:?} (limit {limit}, actual {actual})"
                );
            }
        }
        std::process::exit(1);
    }
}
