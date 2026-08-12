id: correctness-036-001
scope: glass-dev runtime correctness
status: ready
depends-on: []

## Objective

Complete issue #36 pillars 1-5: worker ownership, recoverable daemon operations,
verified-not-settled task policy, Pi doctor/setup/status, and semantic docs
truth.

## Context

- `docs/plan/analysis/release-036.md`
- `docs/pi-sdk-runtime.md`
- `docs/task-dag.md`
- `docs/daemon.md`

## Path

- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/daemon.rs`
- `crates/glass-dev/src/tasks.rs`
- `crates/glass-dev/src/pi_runtime.rs`
- `crates/glass-dev/src/cli.rs`
- related tests and documentation

## Verification

Focused unit/integration tests must prove terminal worker join, attributed
panic, repeated cancellation cleanup, operation reconnect/cancel/reconcile,
non-settle implementation defaults, Pi readiness/setup failure recovery, and
forbidden obsolete documentation phrases.
