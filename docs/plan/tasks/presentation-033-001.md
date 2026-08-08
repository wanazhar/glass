id: presentation-033-001
scope: Glass v0.3.3 connection and presentation policy
status: completed
depends-on: []

## objective

Implement independent layout/transport/graphics/shell dimensions, conservative
probing/overrides, deterministic presentation profiles, full metrics and local
30/60 FPS behavior with latest-state and idle/background throttling.

## context

- `docs/plan/analysis/release-033.md`
- `docs/architecture/connection-presentation.md`
- `docs/architecture/tui.md`

## path

- `crates/glass-browser/src/presentation.rs`
- `crates/glass-browser/src/tui/app.rs`
- `crates/glass-browser/src/cli/args.rs`
- presentation tests and benchmarks
- related architecture/user docs

## verification

- deterministic policy matrix and width/transport independence tests
- local profile, adaptation-order, metric and scheduler tests
- formatting, Clippy and relevant Rust tests

## result

Completed locally on 2026-08-08. Independent connection dimensions feed a
deterministic policy matrix, bounded observatory, adaptive capture scheduler,
and diagnostics with distinct requested/acquired/presented measurements.
