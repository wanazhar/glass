---
id: native-engine-browser-004
scope: glass-browser/native-engine/content-process-control
status: done
depends-on: [native-engine-browser-003]
---

# BE-01c: process-backed content control channel

## Objective

Introduce the first process-backed control boundary for the native browser
engine without creating a third installable crate. The existing
`glass-browser` package owns a small `glass-native-content-worker` helper and
the native engine requires that helper for external HTTP(S) initialization and
navigation.

This is a control-plane milestone, not a claim of hostile-content safety. The
parent still performs bounded HTTP(S) fetching, UTF-8 decoding, and document
construction. Resource transfer, content execution, OS sandboxing, process
supervision, crash recovery, and browser parity remain later gates.

## Contract

- The helper is a second binary target in `glass-browser`; the workspace stays
  at exactly two installable crates.
- Parent and helper communicate only through stdin/stdout with a bounded
  big-endian length-prefixed JSON protocol. Successful protocol negotiation,
  request IDs, and command acknowledgements are required.
- `ping`, `start`, `commit`, and `close` are explicit typed lifecycle commands.
  Frames are capped at 1 MiB and the helper emits no logs on stdout.
- Async native initialization and external navigation require a live helper
  and a successful `start`/`commit` acknowledgement. Missing helpers, closed
  pipes, malformed frames, mismatched IDs, unsupported protocol versions, and
  rejected commands return typed worker errors; there is no CDP fallback.
- Backend shutdown uses a bounded graceful close acknowledgement and then
  reaps or kills the helper if it does not exit within one second.
- The helper receives no page source, response body, cookie, credential, form
  value, or evaluated script in this slice. The parent-side bounded loader is
  intentionally still the resource owner.

## Tradeoffs and missed behavior

- A helper binary and framed IPC make liveness and ownership observable while
  keeping the build graph inside the two existing crates. They add one binary
  artifact and an IPC protocol that must remain versioned and tested.
- The process boundary currently acknowledges lifecycle/commit control only;
  it does not isolate parsing, layout, painting, JavaScript, or networking.
  Moving bounded resource transfer into the child is the next process slice.
- `kill_on_drop` is the fail-closed emergency path. Normal backend shutdown
  uses the close acknowledgement, but supervisor restart, state recovery,
  OS-specific job/process-group ownership, and sandbox policy are not yet
  implemented.
- Helper discovery uses `GLASS_NATIVE_CONTENT_WORKER` when set, then searches
  the test or installed executable's nearby directory. Packaging and release
  jobs must ship the helper beside the main binary before native HTTP is
  promoted as a default production path.

## Paths

- `crates/glass-browser/Cargo.toml`
- `crates/glass-browser/src/bin/glass_native_content_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The process-backed slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --locked`
- `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine lifecycle_state_is_terminal_after_close -- --nocapture`

The affected check and helper build passed with no warnings. The real helper
process handled bounded external HTTP navigation and graceful backend close;
the integration test passed 1/1 and the lifecycle regression passed 1/1.
Full resource transfer, content-process execution, OS sandboxing, crash
supervision/recovery, cross-platform packaging, WPT, security, and production
promotion gates remain open.
