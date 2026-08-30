---
id: native-engine-008
scope: glass-browser/native-engine/runtime-cli
status: in-progress
depends-on: [native-engine-007]
---

# Native runtime and CLI integration

## Objective

Close the native engine's Phase 2 user-facing integration gap without changing
the default runtime:

- add a feature-gated `BrowserRuntime::Native` value;
- add an explicit Rust `BrowserRuntimeSession::connect_native` constructor
  over `NativeEngineConfig`;
- add `--browser-runtime native` to native-feature builds of the one-shot CLI;
- support only local native navigation plus the existing bounded
  navigate/click/type/text/observe/targets command path; and
- fail closed for external endpoints, remote URLs, script/evaluate, MCP, TUI,
  profiles, and other Chromium-only options.

Native runtime construction remains explicit and never participates in
automatic backend selection or fallback. Default builds must not gain the
native feature or the native CLI value.

## Context

- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/cli.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-007.md`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/cli/args.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`BrowserRuntime::Native` exists only when the `native-engine` Cargo feature is
enabled. Rust callers construct it with `BrowserRuntimeSession::connect_native`
and provide the native configuration; no endpoint is contacted. Native CLI
mode constructs the default local configuration and accepts only
`about:blank`, bounded `data:text/html`, and configured fixture URLs available
to the process. It accepts semantic native locators (`ref`, `id`, `role`,
`name`, and `text`), not CSS selectors.

The CLI must reject `--browser-endpoint` and external-browser lifecycle flags
for native mode. Script/evaluate, MCP, TUI, persistent profiles, screenshots,
storage, downloads, prompts, and remote navigation remain denied by the
existing backend profile or runtime validation. Native mode is never selected
when the runtime is omitted and never silently falls back to another backend.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
