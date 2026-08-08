---
id: semantic-resource-033-002
scope: semantic and agent resource efficiency
status: completed
depends-on: [semantic-core-033-001]
release: 0.3.2
---

# Optimize and audit semantic core resource use

## Objective

Implement the accepted resource budgets, fix every confirmed correctness bug
found along the touched paths, preserve public contracts, and record measured
browser-free plus live-browser evidence.

## Context

- `docs/architecture/semantic-resource-budgets.md`
- `docs/architecture/semantic-core-hardening.md`
- `docs/semantic-execution.md`
- `docs/development-runtime.md`

## Path

- `crates/glass-browser/src/task_compiler.rs`
- `crates/glass-browser/src/browser/session/task.rs`
- `crates/glass-browser/src/development/agent.rs`
- `crates/glass-browser/src/cli/runner.rs`
- focused tests, benchmark evidence, and contract documentation

## Verification

- focused compiler, binding, gateway, and private-file tests;
- repeatable browser-free resource benchmark;
- workspace tests, Clippy, rustdoc, release build, security validators;
- all-feature native Chromium smoke suite;
- clean conventional local commits and no remote mutation.

## Result

Completed on 2026-08-08. The compiler now builds reachability and normalized
field indexes once, live binding uses unique iterator scans, the agent gateway
retains descriptors and streams JSON size/digest evidence, and all private and
project source reads are handle-bound and capped. The complete evidence is in
`docs/plan/analysis/semantic-resource-audit.md`.
