use clap::{CommandFactory, FromArgMatches};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    glass_browser::cli::runner::dispatch_browser(parse_cli()?).await
}

fn parse_cli() -> Result<glass_browser::cli::args::Cli, Box<dyn std::error::Error>> {
    #[cfg(windows)]
    return std::thread::Builder::new()
        .name("glass-browser-cli-parser".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(configured_cli)?
        .join()
        .map_err(|_| std::io::Error::other("Glass browser CLI parser thread panicked"))
        .map_err(Into::into);

    #[cfg(not(windows))]
    Ok(configured_cli())
}

fn configured_cli() -> glass_browser::cli::args::Cli {
    let mut command = glass_browser::cli::args::Cli::command();
    command = command
        .name("glass-browser")
        .bin_name("glass-browser")
        .about("Glass local semantic browser control plane")
        .mut_subcommands(|subcommand| {
            if matches!(subcommand.get_name(), "project" | "agent") {
                subcommand.hide(true)
            } else {
                subcommand
            }
        });
    glass_browser::cli::args::Cli::from_arg_matches(&command.get_matches())
        .unwrap_or_else(|error| error.exit())
}
