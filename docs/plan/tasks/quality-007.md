---
id: quality-007
scope: comparative quality and resource measurement
status: done
depends-on: []
---

# Establish the browser automation scorecard

## Objective

Create a reproducible scenario corpus and measurement harness that gates task
success, wrong actions, latency, context bytes, RSS, peak memory, CDP calls, and
binary size before new capabilities are implemented.

## Context

- `docs/architecture/automation.md`
- `docs/plan/analysis/best-in-class-browser.md`
- `benchmarks/README.md`

## Path

- `benchmarks/`
- `examples/`
- `tests/fixtures/`
- `tests/`
- `docs/plan/analysis/best-in-class-browser.md`

## Requirements

- Version deterministic workflows covering duplicate labels, overlays,
  reflow, delayed content, SPA navigation, forms, popups, frames, dialogs,
  downloads, and failure recovery.
- Separate task success from operation latency and record wrong actions as a
  hard failure.
- Record machine, OS, architecture, Rust, Chrome, iteration, session
  temperature, and profile metadata. Corpus v1 is a warm single-session
  comparison; cold mode is not emitted until an equivalent cross-adapter
  browser lifecycle is defined.
- Add comparison adapters without adding competitor dependencies to Glass.
- Ratify or revise initial budgets with evidence.

## Verification

- Harness produces stable machine-readable reports on repeated local runs.
- Fixture outcomes detect a deliberately wrong target implementation.
- Resource measurements clearly separate Glass and Chrome.
- Existing fmt, Clippy, test, and browser smoke checks pass.

## Completion

- Added the versioned `glass-local-v1` corpus with eleven deterministic
  targeting, waiting, navigation, input, topology, diagnostic, download, and
  recovery scenarios.
- Added a machine-readable scorecard report and schema that separate exact
  success, ordinary failure, and wrong actions; the latter always fails the
  hard gate.
- Added monotonic CDP request instrumentation and disjoint Glass-runner versus
  owned-Chrome process-tree RSS sampling.
- Added a dependency-external Playwright adapter using the same corpus and
  explicit `null` values for metrics unavailable through its public API.
- Ratified the initial resource budgets with a recorded optimized local run.
  The current duplicate-label behavior intentionally fails the zero-wrong-
  action gate and becomes the acceptance case for `target-009`.
- Verification passed: scorecard unit tests, live normal and forced-wrong
  scorecard runs, formatting, strict Clippy, all-target tests, and the opt-in
  real-Chrome smoke suite. The Playwright adapter passed Node syntax validation;
  its optional external dependency was not installed in the repository.

## Commit

`test: establish browser automation scorecard`
