use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    glass_browser::cli::runner::dispatch(parse_cli()?).await
}

fn parse_cli() -> Result<glass_browser::cli::args::Cli, Box<dyn std::error::Error>> {
    #[cfg(not(windows))]
    return Ok(glass_browser::cli::args::Cli::parse());

    #[cfg(windows)]
    {
        let cli = std::thread::Builder::new()
            .name("glass-cli-parser".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(glass_browser::cli::args::Cli::parse)?
            .join()
            .map_err(|_| std::io::Error::other("Glass CLI parser thread panicked"))?;
        Ok(cli)
    }
}
