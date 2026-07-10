# Repository Guidelines

## Project Structure & Module Organization

This is a Rust 2024 binary crate. `src/main.rs` initializes logging and dispatches the application. Runtime code is grouped by concern:

- `src/browser/`: Chrome lifecycle, raw CDP/WebSocket calls, DOM/accessibility parsing, mouse events, and profiles.
- `src/cli/`: Clap argument definitions in `args.rs` and shared dispatch/session orchestration in `runner.rs`.
- `src/mcp/`: JSON-RPC/MCP stdio server.
- `src/tui/`: Ratatui terminal interface.
- `Cargo.toml` and `Cargo.lock`: dependency and reproducible-build metadata.

There is currently no `tests/`, `examples/`, or `benches/` directory. Focused unit tests live beside the module under test; use `tests/` for end-to-end behavior.

## Build, Test, and Development Commands

- `cargo build` compiles the debug binary.
- `cargo run -- --help` lists the implemented CLI options and subcommands.
- `cargo run -- install-chromium` downloads the managed Chromium build; browser flows otherwise require Chrome/Chromium and use CDP port `9222` by default.
- `cargo run -- "navigate to https://example.com"` runs a one-shot browser prompt; subcommands include `navigate`, `click`, `type`, `screenshot`, `text`, `dom`, `scroll`, and `evaluate`.
- `cargo run -- profiles` and `cargo run -- delete-profile NAME` manage profiles; `cargo run -- --mcp` starts the real MCP server over stdio; `cargo run` starts the TUI.
- `cargo test` runs the current unit tests; coverage has no enforced threshold.
- `cargo fmt --all -- --check` verifies formatting; `cargo clippy --all-targets --all-features -- -D warnings` checks lint cleanliness.

## Coding Style & Naming Conventions

Use `rustfmt` defaults, four-space indentation, `snake_case` for functions/modules, `UpperCamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Prefer typed structs and `Result`-based error propagation. Use `tracing` for diagnostics and reserve `println!` for intentional CLI output.

## Testing Guidelines

Name tests after the behavior they verify, such as `parse_accessibility_tree_handles_empty_nodes`. Cover parser, profile, and command-dispatch logic with deterministic tests; keep Chrome/network smoke tests explicit and isolated.

## Commit & Pull Request Guidelines

No Git history is present in this workspace, so no existing commit convention can be verified. Use short, imperative, focused messages, for example `browser: handle CDP timeouts`. Pull requests should explain the behavior change, list validation commands, link the relevant issue, and include a terminal screenshot or recording for TUI changes.

## Security & Configuration Tips

CDP provides control over the browser. Use `--incognito` for disposable sessions, avoid committing screenshots or profile data, and do not log cookies, page secrets, or evaluated user input. Set `RUST_LOG` when debugging tracing output.
