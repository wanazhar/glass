---
id: native-engine-browser-001
scope: glass-browser/native-engine/runtime-substrate
status: done
depends-on: [native-engine-browser-000]
---

# BE-01a: typed runtime substrate

## Objective

Establish the first executable BE-01 runtime boundary inside `glass-browser`:
runtime lifecycle separate from document lifecycle, one-shot cancellation,
typed task and microtask queues, bounded privacy-safe trace events, startup
rollback, and terminal close. Route native engine initialization, navigation
commit tasks, and close through that runtime owner.

This is the first BE-01 batch. It does not claim network access, JavaScript
execution, asynchronous Web APIs, an out-of-process content process, OS
sandboxing, or browser parity.

## Context

- `docs/plan/native-engine-browser-profile.md` (`GCWP-0.1`)
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/scheduler.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- `NativeRuntime` owns runtime state, the deterministic scheduler, typed
  microtasks, cancellation, and bounded trace records.
- Runtime traces contain ordering, logical-clock, event-kind, and task-ID
  metadata only; they never contain page source, cookies, form values, or
  evaluated script.
- Cancellation rejects new runtime work explicitly. Close cancels and clears
  pending work, records terminal state, and cannot be reopened.
- Engine startup can roll back to `New` if the first commit fails before the
  engine becomes `Running`.
- Existing native scheduler ordering and public engine behavior remain stable.
- The implementation remains synchronous/deterministic until later BE-01 work
  adds real asynchronous workers and the content-process boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/runtime.rs`
- `crates/glass-browser/src/browser/native_engine/scheduler.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The completed batch used the affected native feature only:

- `cargo check -p glass-browser --features native-engine --lib --locked`
- `cargo fmt --all -- --check`
- `cargo test -p glass-browser --features native-engine --lib native_engine::runtime`
- `cargo test -p glass-browser --features native-engine --test native_engine lifecycle_state_is_terminal_after_close`

The full workspace, all-feature, remote-CI, process-isolation, WPT, real-site,
and production promotion gates remain deferred to their BE/M milestones.

## Completion evidence

The runtime unit batch passed 3/3 tests. The engine lifecycle integration
test passed 1/1 and verified runtime start, commit-task trace, cancellation on
close, and terminal runtime state. The native-feature library check and
formatting check passed. No new dependency, feature default, installable
crate, public transport schema, or CDP path was added.
