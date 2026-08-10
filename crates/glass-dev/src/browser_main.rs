use clap::{CommandFactory, FromArgMatches};

type MainResult = Result<(), Box<dyn std::error::Error>>;

fn main() -> MainResult {
    #[cfg(windows)]
    return std::thread::Builder::new()
        .name("glass-browser-main".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| run().map_err(|error| error.to_string()))?
        .join()
        .map_err(|_| std::io::Error::other("Glass browser main thread panicked"))?
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
        .block_on(glass_browser::cli::runner::dispatch_browser(
            configured_cli(),
        ))
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
