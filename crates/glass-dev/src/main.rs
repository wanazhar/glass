use clap::Parser;

type MainResult = Result<(), Box<dyn std::error::Error>>;

fn main() -> MainResult {
    #[cfg(windows)]
    return std::thread::Builder::new()
        .name("glass-main".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| run().map_err(|error| error.to_string()))?
        .join()
        .map_err(|_| std::io::Error::other("Glass main thread panicked"))?
        .map_err(|error| std::io::Error::other(error).into());

    #[cfg(not(windows))]
    run()
}

fn run() -> MainResult {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(glass_dev::dispatch(glass_browser::cli::args::Cli::parse()))
}
