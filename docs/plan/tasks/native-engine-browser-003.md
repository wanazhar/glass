---
id: native-engine-browser-003
scope: glass-browser/native-engine/runtime-worker
status: done
depends-on: [native-engine-browser-002]
---

# BE-01b: asynchronous runtime worker boundary

## Objective

Move asynchronous native initialization and navigation commit scheduling onto
a typed Tokio worker while retaining one runtime state owner. The worker must
be observable, cancellation-aware, bounded, and explicit about a crashed
channel. This is the first executable BE-01b slice; it is not yet the
production content-process or OS-sandbox boundary.

## Contract

- `NativeRuntimeShared` protects the single `NativeRuntime` state used by
  synchronous engine projections and the asynchronous worker.
- `NativeRuntimeWorker` serializes typed start, rollback, close, schedule,
  readiness, clock, microtask, cancellation, and trace commands through a
  bounded channel.
- Async native initialization and navigation commits use the worker. Existing
  synchronous engine construction/tests retain the deterministic direct path.
- Worker command failures never fall back to Chromium/CDP or silently retry;
  a closed worker channel returns a typed runtime-worker error.
- Worker traces remain the existing bounded privacy-safe records. No page
  source, response body, cookie, form value, or evaluated script crosses the
  worker command boundary.
- The worker remains in-process. Process isolation, IPC, OS sandboxing,
  supervisor restart, cross-process quotas, and crash recovery are later
  BE-01 gates.

## Tradeoffs and missed behavior

- A Tokio task and shared mutex provide a real async ownership boundary with
  low implementation/build cost, but they do not protect the process from
  hostile page code or native dependency failure.
- The synchronous engine API remains available for deterministic local tests;
  this preserves compatibility but means the final engine still needs one
  unified event-loop contract across every page operation.
- A crashed worker is reported and stops the operation. Automatic restart is
  intentionally deferred until page-state recovery and process supervision
  are specified, so callers cannot observe a false success.

## Paths

- `crates/glass-browser/src/browser/native_engine/worker.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --locked`
- `cargo test -p glass-browser --features native-engine --lib native_engine::worker`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine lifecycle_state_is_terminal_after_close -- --nocapture`

The worker unit boundary passed `2/2`; the worker-backed external HTTP
navigation passed `1/1`; the synchronous lifecycle regression passed `1/1`;
formatting and the affected library check passed with no warnings. Full
process isolation, cross-platform, WPT, security, crash-recovery, and
production promotion gates remain open.

