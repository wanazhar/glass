---
id: native-engine-browser-728
scope: glass-browser/public-rust-session-entrypoint
status: done
depends-on: [native-engine-browser-727]
---

# Glass native-engine browser slice 728: native-first public Rust BrowserSession

## Objective

Make `glass_browser::BrowserSession` the native-only default Rust session
entrypoint while retaining the current Chrome/CDP session under the explicit
`CdpBrowserSession` name. The canonical constructor must go directly to the
native backend and must not probe or fall back to CDP.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/browser.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/rust-sdk.md`
- `docs/plan/tasks/native-engine-browser-727.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/session/mod.rs`

## Contract

- With the default `native-engine` feature, `BrowserSession` is the public
  alias for the runtime-neutral session and exposes `start(NativeEngineConfig)`
  plus `start_default()`.
- Both constructors create `BackendStartup::Native` directly, initialize it,
  and never inspect CDP endpoints, launch Chrome, or retry another backend.
- The prior Chrome/CDP struct and its `SessionOptions`-based lifecycle are
  named and exported as `CdpBrowserSession`. Existing internal callers that
  intentionally operate over CDP, examples, and smoke tests must use that
  explicit type.
- Keep `BrowserRuntimeSession` as an equivalent lower-level name for source
  clarity and compatibility. Firefox/Safari endpoint constructors remain
  explicit and do not become fallbacks.
- Document the current native method surface honestly. This slice changes the
  entrypoint, not the native engine's remaining operation parity, security
  certification, or Core Web Profile completion.
- Correct the current Rust SDK claim that native navigation is limited to
  local URL forms; checked-in implementation/tests include external HTTP(S)
  navigation, while full browser conformance remains unfinished.

## Path

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/session/`
- `crates/glass-browser/src/browser/mod.rs`
- `crates/glass-browser/src/lib.rs`
- CDP-backed internal callers, examples, and tests in both workspace crates
- `docs/rust-sdk.md`
- `docs/architecture/browser.md`
- `docs/architecture/native-engine.md`
- `docs/architecture/README.md`
- `docs/architecture/automation.md`
- `docs/architecture/browser-connection.md`
- `docs/architecture/browser-workspace.md`
- `docs/architecture/experience.md`
- `docs/architecture/product-workspace.md`
- `docs/architecture/tui.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/features.md`
- `docs/cli.md`
- `docs/browser-host-rfc.md`
- `docs/experimental-capabilities.md`
- `docs/actions.md`
- `docs/extensions.md`
- `docs/intent-resolution.md`
- `docs/workflows.md`
- `docs/getting-started.md`
- `docs/daemon.md`
- `docs/mobile-remote.md`
- `README.md`
- `crates/glass-browser/README.md`
- `CHANGELOG.md`

## Verification

- Passed on 2026-09-25:
  `cargo check --locked --quiet -p glass-browser -p glass-dev --all-targets`.
- Passed on 2026-09-25:
  `cargo test --locked --quiet -p glass-browser --lib canonical_browser_session_starts_native_without_cdp_fallback`
  (1 passed).
- Passed on 2026-09-25: `cargo fmt --all -- --check`, `git diff --check`, and
  the release-truth, documentation-depth, and TUI-shortcut maintainer gates.
- Live documentation coverage was skipped because `target/debug/glass` was
  absent; `cargo check` does not link that binary. Remote CI was not run.
- Preserve the existing unrelated edit in
  `crates/glass-browser/src/browser/native_engine/javascript.rs`.
- Do not run workspace-wide tests, remote CI, `cargo clean`, or claim issue #40
  complete in this slice.
