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
    let cli = glass_browser::cli::args::Cli::from_arg_matches(&command.get_matches())?;
    glass_browser::cli::runner::dispatch_browser(cli).await
}
