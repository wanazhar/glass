---
id: native-engine-browser-008
scope: glass-browser/native-engine/content-process-recovery
status: done
depends-on: [native-engine-browser-007]
---

# BE-01g: child failure classification and deterministic recovery

## Objective

Make content-worker failures observable and recoverable without retrying a
failed action against a different owner. The process channel now classifies
spawn, exit, transport, timeout, protocol, rejection, and invalid-transfer
failures. A poisoned worker is never reused; an external navigation creates a
fresh worker, while a failed action returns a typed error and does not advance
the document revision.

This is reliability hardening, not a claim that the helper is yet an OS
sandbox or a hostile-content boundary.

## Contract

- `NativeWorkerFailureKind` is part of the native error taxonomy and survives
  the native backend mapping as a connection failure with the failure class in
  its diagnostic text.
- Broken pipes and a child that has already exited are distinguished from
  protocol rejection, invalid transfer, and operation deadlines where the
  channel can observe the distinction.
- Load and mutation deadlines terminate the child. Invalid mutation snapshots,
  malformed effects, protocol acknowledgements, and child mutation rejection
  poison the process and cannot publish a success result.
- `NativeEngine::action_async` refuses to reuse a poisoned process. It never
  retries the action through the parent or CDP. `navigate_async` is the
  explicit recovery boundary: it drops the poisoned process, spawns a fresh
  helper, and commits only the new successful document.
- Closing an already-exited content process is idempotent, so a prior crash
  does not turn normal engine shutdown into a second failure.
- Startup failure leaves the engine in `New`; a subsequent initialization can
  retry process creation after the caller repairs the worker installation.

## Tradeoffs and missed behavior

- Recovery is deliberate and navigation-bound rather than automatic action
  replay. This avoids duplicate clicks or hidden state changes, but callers
  must observe the typed failure and navigate/restart before continuing.
- Failure classes improve diagnostics without exposing child stderr, URLs,
  cookies, input text, or response bodies. The current public backend maps
  them to connection failures; a later capability/error schema can expose
  structured classes end-to-end.
- The supervisor is still one bounded helper per engine. There is no restart
  budget, crash-loop breaker, persisted session handoff, multi-process site
  isolation, or cross-platform OS sandbox yet.
- The startup recovery test uses a Unix exit helper. Windows/macOS process
  exit, sandbox launch, crash dumps, and clean-install binary packaging need
  platform-specific gates before production promotion.

## Paths

- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The recovery slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture`

The affected check passed without warnings. The process test filter passed 5/5:
child-owned document limits, child-owned computed style, malformed-document
failure atomicity, child-owned mutation/effects, and startup recovery after a
worker exit. OS sandboxing, cross-platform crash/restart, diagnostics transfer,
script execution, WPT, network security, and browser promotion remain open.
