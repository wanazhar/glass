# Glass

Glass is a local-first browser automation tool written in Rust. It drives
Chrome or Chromium through the native Chrome DevTools Protocol (CDP), keeping
the automation client small and avoiding a browser-driver runtime.

## Quick Start

Install Chrome/Chromium, then run:

    cargo run -- "navigate to https://example.com"
    cargo run -- text
    cargo run -- screenshot -o page.png
    cargo run -- dom

Run cargo run for the terminal UI or cargo run -- --mcp to expose the browser
as an MCP stdio server. Use --incognito for a disposable profile,
--profile NAME for persistent Chrome data, and --chrome-path PATH to select
the browser executable.

## Development

    cargo fmt --all -- --check
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture

The local fixture test requires a detectable Chrome/Chromium installation.
Performance measurements are documented in benchmarks/README.md; they
separate cold browser startup from warm CDP command latency.

## Design

All frontends use the same BrowserSession. The CDP connection is a multiplexed
actor that routes responses by request ID and broadcasts browser events.
DOM/accessibility parsing and input actions stay in the browser layer, so CLI,
TUI, and MCP behavior remains consistent.
