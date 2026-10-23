type MainResult = Result<(), Box<dyn std::error::Error>>;
const RUNTIME_THREAD_STACK_SIZE: usize = 4 * 1024 * 1024;

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
    let cli =
        glass_browser::cli::args::Cli::parse_process_args().unwrap_or_else(|error| error.exit());

    let interactive_tui = uses_interactive_tui(&cli);
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if interactive_tui {
        // Tracing output on stderr draws over Ratatui's alternate screen. In
        // particular, Chromium startup diagnostics include its profile path
        // and launch arguments. Keep normal command diagnostics on stderr, but
        // make the interactive workspace's terminal exclusively its UI.
        tracing_subscriber::fmt()
            .with_writer(std::io::sink)
            .with_env_filter(log_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(log_filter)
            .init();
    }

    tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(RUNTIME_THREAD_STACK_SIZE)
        .enable_all()
        .build()?
        .block_on(glass_dev::dispatch(*cli))
}

fn uses_interactive_tui(cli: &glass_browser::cli::args::Cli) -> bool {
    cli.prompt.is_none()
        && !cli.mcp
        && (cli.command.is_none()
            || matches!(
                cli.command.as_ref(),
                Some(glass_browser::cli::args::Commands::Tui)
            ))
}

#[cfg(test)]
mod tests {
    use super::uses_interactive_tui;
    use clap::Parser;
    use glass_browser::cli::args::Cli;

    fn parse(args: &'static [&'static str]) -> Box<Cli> {
        std::thread::Builder::new()
            .name("glass-test-cli-parser".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Box::new(Cli::try_parse_from(args).unwrap()))
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn only_interactive_workspace_routes_tracing_away_from_terminal() {
        let tui = parse(&["glass"]);
        let explicit_tui = parse(&["glass", "tui"]);
        let one_shot = parse(&["glass", "navigate", "https://example.test"]);
        let mcp = parse(&["glass", "--mcp"]);

        assert!(uses_interactive_tui(&tui));
        assert!(uses_interactive_tui(&explicit_tui));
        assert!(!uses_interactive_tui(&one_shot));
        assert!(!uses_interactive_tui(&mcp));
    }
}
