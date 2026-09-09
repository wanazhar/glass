#[tokio::main(flavor = "current_thread")]
async fn main() {
    if glass_browser::browser::native_engine::run_native_content_worker()
        .await
        .is_err()
    {
        std::process::exit(1);
    }
}
