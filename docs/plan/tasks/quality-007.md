---
id: quality-007
scope: comparative quality and resource measurement
status: pending
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
- Record machine, OS, architecture, Rust, Chrome, iteration, warm/cold, and
  profile metadata.
- Add comparison adapters without adding competitor dependencies to Glass.
- Ratify or revise initial budgets with evidence.

## Verification

- Harness produces stable machine-readable reports on repeated local runs.
- Fixture outcomes detect a deliberately wrong target implementation.
- Resource measurements clearly separate Glass and Chrome.
- Existing fmt, Clippy, test, and browser smoke checks pass.
