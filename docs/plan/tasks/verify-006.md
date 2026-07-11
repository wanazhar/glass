---
id: verify-006
scope: verification
status: pending
depends-on: [lifecycle-002, mcp-004, tui-005]
---

# Performance and regression verification

## Objective

Demonstrate that the new contracts work together and record measurable latency, payload, binary, and memory behavior.

## Context

- `docs/architecture/README.md`
- `docs/architecture/browser.md`
- `docs/architecture/tui.md`
- `docs/plan/analysis/performance-overhaul.md`

## Path

- `benchmarks/`
- `examples/`
- `tests/`
- `README.md`
- `docs/plan/`

## Requirements

- Benchmark cold start, warm compact observation, deep DOM, screenshot, fast click, and human click separately.
- Record agent payload byte counts and Glass process memory when the environment supports it.
- Exercise profile/incognito/attach and frontend behavior through real integrations where possible.
- Update user-facing benchmark guidance without overstating cross-machine results.

## Verification

- `cargo build --release`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`
- `GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture` when Chromium is available.

## Commit

`test: cover compact browser workflows`
