---
id: native-engine-browser-132
scope: glass-browser/native-engine/mcp-session
status: completed
depends-on: [native-engine-browser-131]
---

# BE-46: native MCP session routing

## Objective

Make the MCP transport able to run the native browser session for its core
browser tools. Selecting the native runtime must no longer be rejected or
silently create the Chromium BrowserSession.

## Contract

- Native MCP owns a separate BrowserRuntimeSession slot alongside the existing
  Chromium session slot, preserving the public compatibility API for callers
  that provide a BrowserSession store.
- Native MCP starts lazily at about:blank, uses the requested bounded viewport,
  and closes its owned session at stdio EOF.
- navigate, click, type, clear, check, uncheck, select, key, keyDown, keyUp,
  shortcut, scroll, getText, getDOM, observe, evaluate, screenshot,
  listTargets, localStorage, and sessionStorage route through native owners,
  including expected-revision guards where the tool carries one.
- Native MCP never starts Chromium, contacts CDP, or falls through to the
  existing BrowserSession path. A tool outside this slice returns an explicit
  unsupported error.
- Existing offline MCP tools and the Chromium path retain their current
  behavior.

## Deliberate boundary and tradeoffs

The native slot is separate instead of changing the long-lived run_mcp_stream
BrowserSession parameter. This keeps downstream daemon and test callers
source-compatible while making runtime ownership explicit. The first native
MCP projection uses the stable semantic backend result envelope; rich
BrowserSession workflow, knowledge, checkpoint, download, prompt, frame, and
multi-target owners still need native implementations before the native
runtime can become the universal MCP default.

## Paths

- crates/glass-browser/src/mcp/server.rs
- crates/glass-browser/src/cli/runner.rs
- crates/glass-browser/tests/native_engine.rs
- docs/architecture/native-engine.md
- docs/cli.md
- docs/plan/README.md
- docs/plan/analysis/native-engine.md
- GitHub issue #40

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib` passed;
- `cargo check --quiet -p glass-browser --lib` passed;
- `RUST_MIN_STACK=16777216 cargo test --quiet -p glass-browser --features
  native-engine --lib
  mcp::server::tests::native_mcp_routes_core_browser_tools_without_chromium`
  passed (1/1), including navigation, text, input, script, DOM, PNG, storage,
  unsupported-tool, and no-Chromium assertions;
- the existing default-feature MCP initialization test passed (1/1);
- `cargo fmt --all`, `git diff --check`, documentation coverage/depth, and
  release-truth checks passed (782 Markdown files, 345 MCP tools, 93 current
  guides, 19 substantive contracts, 0 current-claim failures);
- the native-feature strict Clippy command still reports the pre-existing
  native-engine `-D warnings` backlog in unrelated modules; the changed MCP
  file contributes no Clippy findings after the local argument/lifetime fixes.

This slice brings core MCP operations through native ownership but does not
promote native certification or claim complete CDP replacement. The focused
native MCP routing test passes with no Chromium session allocated; richer MCP
workflow, profile, download, prompt, frame, checkpoint, and recovery owners
remain later issue #40 work.
