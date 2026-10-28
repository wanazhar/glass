use clap::{CommandFactory, FromArgMatches};
use glass_browser::cli::args::Cli;
use std::ffi::OsString;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::process::Command;

type MainResult = Result<(), Box<dyn std::error::Error>>;
const RUNTIME_THREAD_STACK_SIZE: usize = 4 * 1024 * 1024;
const MAIN_THREAD_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() {
    let outcome = std::thread::Builder::new()
        .name("glass-main".into())
        .stack_size(MAIN_THREAD_STACK_SIZE)
        .spawn(|| run().map_err(|error| error.to_string()));
    let error = match outcome {
        Ok(thread) => match thread.join() {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error,
            Err(_) => "Glass main thread panicked".to_owned(),
        },
        Err(error) => format!("could not start Glass CLI: {error}"),
    };
    write_cli_error(&error);
    std::process::exit(1);
}

fn write_cli_error(error: &str) {
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    if stderr.is_terminal() {
        let width = glass_tui::crossterm::terminal::size()
            .map(|(width, _)| usize::from(width))
            .unwrap_or(80)
            .clamp(24, 120);
        let color = !glass_tui::theme::no_color() && std::env::var("TERM").as_deref() != Ok("dumb");
        let report = glass_tui::cli::error("Glass", error, width, color);
        let _ = stderr.write_all(report.as_bytes());
    } else {
        let _ = writeln!(stderr, "Error: {error}");
    }
}

fn run() -> MainResult {
    let args = std::env::args_os().collect::<Vec<_>>();
    let command = Cli::command();
    if let Some(core_args) = core_cli_forward_args(&args, &command) {
        let presentation = core_cli_presentation(&args);
        return forward_core_cli(&core_args, presentation);
    }
    let command = command.after_help(
        "Glass Core commands are also available as `glass <core-command> ...` when the name does not conflict with a Glass command. Use `glass core <args...>` as the complete, exact Core CLI escape hatch; `glass core --help` shows Core's own commands and options.\n\nAdd `--json` before or after a Glass command for machine-readable terminal output; pipes remain machine-readable by default.",
    );
    let command = hide_browser_options_from_glass_help(command);
    let (args, json_output) = glass_browser::cli::runner::take_json_flag(args);
    let cli = parse_glass_args(command, args).unwrap_or_else(|error| error.exit());
    glass_browser::cli::runner::configure_json_output(json_output);

    let interactive_tui = uses_interactive_tui(&cli);
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
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

/// Keep Browser-wide flags accepted globally without repeating them in help
/// for Glass-only workspace and agent commands where they have no effect.
fn hide_browser_options_from_glass_help(mut command: clap::Command) -> clap::Command {
    command.build();
    let browser_global_ids = command
        .get_arguments()
        .filter(|argument| argument.is_global_set())
        .map(|argument| argument.get_id().as_str().to_owned())
        .collect::<Vec<_>>();
    const GLASS_ONLY_COMMANDS: &[&str] = &[
        "workspace",
        "project",
        "agent",
        "harness",
        "memory",
        "surfaces",
        "backend",
        "daemon",
        "logs",
        "mcp-config",
    ];
    for name in GLASS_ONLY_COMMANDS {
        command = command.mut_subcommand(name, |subcommand| {
            hide_browser_options_recursively(subcommand, &browser_global_ids)
        });
    }
    command
}

fn hide_browser_options_recursively(
    command: clap::Command,
    browser_global_ids: &[String],
) -> clap::Command {
    command
        .mut_args(|argument| {
            if browser_global_ids
                .iter()
                .any(|id| argument.get_id().as_str() == id)
            {
                argument.hide(true)
            } else {
                argument
            }
        })
        .mut_subcommands(|subcommand| {
            hide_browser_options_recursively(subcommand, browser_global_ids)
        })
}

/// Determine whether argv selects a Core CLI command instead of a Glass route.
/// Existing Glass subcommands always win name collisions; `glass core ...`
/// forwards every argument without interpreting Core's command tree.
fn core_cli_forward_args(
    args: &[OsString],
    glass_command: &clap::Command,
) -> Option<Vec<OsString>> {
    let value_options = [
        "--policy",
        "--policy-allow",
        "--policy-confirm",
        "--policy-confirm-once",
        "--policy-allow-host",
        "--policy-deny-host",
        "--profile",
        "--session",
        "--target-id",
        "--frame-id",
        "--port",
        "--viewport",
        "--interaction",
        "--chrome-path",
        "--browser-runtime",
        "--browser-endpoint",
        "--tui-layout",
        "--tui-transport",
        "--tui-graphics",
        "--tui-rtt-ms",
        "--tui-throughput-mbps",
        "--tui-live",
        "--tui-live-backend",
        "--tui-live-quality",
        "--tui-live-fit",
        "--knowledge-store",
        "--response-mode",
    ];
    let mut command_index = 1;
    while let Some(argument) = args.get(command_index).and_then(|value| value.to_str()) {
        if argument == "--" {
            command_index += 1;
            break;
        }
        if value_options.contains(&argument) {
            command_index += 2;
            continue;
        }
        if argument.starts_with('-') {
            command_index += 1;
            continue;
        }
        break;
    }
    let first = args.get(command_index)?.to_str()?;
    if first == "core" {
        let mut forwarded = args.get(command_index + 1..)?.to_vec();
        if forwarded.is_empty() {
            forwarded.push(OsString::from("--help"));
        }
        return Some(forwarded);
    }
    if matches!(first, "--help" | "-h" | "--version" | "-V")
        || first.starts_with('-')
        || glass_command.find_subcommand(first).is_some()
    {
        return None;
    }
    Some(args[1..].to_vec())
}

/// Parse Glass arguments with Core forwarding guidance added to this binary's
/// help, keeping the standalone `glass-browser` help independent.
fn parse_glass_args(command: clap::Command, args: Vec<OsString>) -> Result<Box<Cli>, clap::Error> {
    std::thread::Builder::new()
        .name("glass-cli-parse".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let matches = command.try_get_matches_from(args)?;
            Cli::from_arg_matches(&matches).map(Box::new)
        })
        .map_err(|error| clap::Error::raw(clap::error::ErrorKind::Io, error.to_string()))?
        .join()
        .map_err(|_| {
            clap::Error::raw(
                clap::error::ErrorKind::Io,
                "Glass CLI argument parser panicked",
            )
        })?
}

/// Replace Glass with Core while passing the human-facing invocation namespace
/// as a one-shot presentation marker; arguments and process streams remain exact.
fn core_cli_presentation(args: &[OsString]) -> Option<&'static str> {
    match args.get(1).and_then(|arg| arg.to_str())? {
        "core" => Some("glass core"),
        _ => Some("glass"),
    }
}

fn forward_core_cli(args: &[OsString], presentation: Option<&str>) -> MainResult {
    let binary = glass_dev::core_runtime::resolve_core_binary(None)
        .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error.to_string()))?;
    let mut command = Command::new(&binary);
    command.args(args).env_remove("GLASS_CORE_CLI_PRESENTATION");
    if let Some(presentation) = presentation {
        command.env("GLASS_CORE_CLI_PRESENTATION", presentation);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(io::Error::other(format!(
            "could not start Glass Core CLI {}: {error}",
            binary.display()
        ))
        .into())
    }
    #[cfg(not(unix))]
    {
        let status = command.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
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
    use super::{core_cli_forward_args, core_cli_presentation, uses_interactive_tui};
    use clap::{CommandFactory, Parser};
    use glass_browser::cli::args::Cli;
    use std::ffi::OsString;

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

    #[test]
    fn core_cli_forwarding_prefers_glass_names_and_has_an_exact_escape_hatch() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let command = Cli::command();
                let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
                assert_eq!(
                    core_cli_forward_args(&args(&["glass", "connect", "--json"]), &command),
                    Some(args(&["connect", "--json"]))
                );
                assert_eq!(
                    core_cli_forward_args(&args(&["glass", "core", "browser", "status"]), &command),
                    Some(args(&["browser", "status"]))
                );
                assert_eq!(
                    core_cli_forward_args(&args(&["glass", "core"]), &command),
                    Some(args(&["--help"]))
                );
                assert_eq!(
                    core_cli_presentation(&args(&["glass", "connect", "status"])),
                    Some("glass")
                );
                assert_eq!(
                    core_cli_presentation(&args(&["glass", "core", "connect", "stop"])),
                    Some("glass core")
                );
                assert_eq!(
                    core_cli_presentation(&args(&["glass", "status", "--json"])),
                    Some("glass")
                );
                assert_eq!(
                    core_cli_presentation(&args(&["glass", "core", "browser", "status"])),
                    Some("glass core")
                );
                assert_eq!(
                    core_cli_forward_args(&args(&["glass", "browser", "status"]), &command),
                    None,
                    "a Glass command wins a top-level name collision"
                );
                assert_eq!(
                    core_cli_forward_args(&args(&["glass", "--help"]), &command),
                    None
                );
            })
            .expect("spawn large Clap graph test")
            .join()
            .expect("forwarding test panicked");
    }
}
