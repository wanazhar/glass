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
    cargo run -- observe

Run cargo run for the terminal UI or cargo run -- --mcp to expose the browser
as an MCP stdio server. Use --incognito for a disposable profile,
--profile NAME for persistent Chrome data, and --chrome-path PATH to select
the browser executable. Observation is DOM/accessibility-first; screenshots
are explicit through screenshot or observe --screenshot. Clicks use smooth
pointer motion by default; pass --interaction fast for low-latency automation.
Glass never adopts an occupied CDP endpoint implicitly: pass --attach (and
--target-id when the endpoint has multiple page targets) to use an existing
Chrome instance. Attach mode deliberately rejects incognito, headed, custom
Chrome-path, and named-profile options because the existing Chrome owns those
settings. `install-chromium` installs a managed Chrome for Testing build that
Glass resolves before a system browser.
The TUI refreshes the structured observation after navigation and page-changing
actions instead of silently taking images. Library callers use `observe()` for
the cached structured context and the explicitly named
`observe_with_screenshot()` only when pixels are needed.

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
