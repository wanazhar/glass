use clap::{CommandFactory, FromArgMatches};
use std::io::IsTerminal;

type MainResult = Result<(), Box<dyn std::error::Error>>;
const RUNTIME_THREAD_STACK_SIZE: usize = 4 * 1024 * 1024;
const MAIN_THREAD_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() -> MainResult {
    let result = std::thread::Builder::new()
        .name("glass-browser-main".into())
        .stack_size(MAIN_THREAD_STACK_SIZE)
        .spawn(|| run().map_err(|error| error.to_string()))?
        .join()
        .map_err(|_| std::io::Error::other("Glass browser main thread panicked"))?
        .map_err(std::io::Error::other);
    if let Err(error) = result {
        if std::io::stderr().is_terminal() {
            let width = glass_tui::crossterm::terminal::size()
                .map(|(width, _)| usize::from(width))
                .unwrap_or(80)
                .clamp(24, 120);
            let color =
                !glass_tui::theme::no_color() && std::env::var("TERM").as_deref() != Ok("dumb");
            let rendered = glass_tui::cli::error("Glass", &error.to_string(), width, color);
            use std::io::Write;
            let _ = std::io::stderr().lock().write_all(rendered.as_bytes());
        } else {
            eprintln!("Error: {error}");
        }
        std::process::exit(1);
    }
    Ok(())
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
        .thread_stack_size(RUNTIME_THREAD_STACK_SIZE)
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
