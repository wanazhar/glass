---
id: verify-006
scope: verification
status: done
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

## Completion

- The release benchmark now measures cold startup, fresh/cached compact
  observation, explicit deep DOM, base64 screenshot capture, and alternating
  fast/human clicks independently.
- Its JSON output records `PageContext` payload bytes, Glass-process RSS when
  supported, binary size, OS/architecture, and iteration counts without
  conflating client memory with Chrome child-process memory.
- Real Chromium smoke coverage now exercises incognito isolation, explicit
  attachment, structured CLI and MCP calls against the same attached page, and
  named-profile local-storage persistence across separate owned MCP sessions.
- Owned-session shutdown now asks Chrome to flush via `Browser.close` before a
  bounded process-level fallback, making named-profile persistence deterministic
  instead of racing a forced child kill.
- The benchmark guide describes comparable-run controls and the release-size
  artifact without treating a local sample as a market-wide result.

## Local verification record (non-comparative)

On 2026-07-11, the local fixture benchmark ran on Linux/aarch64 with ten normal
iterations and five expensive-operation iterations. These values are a delivery
sanity check, not a cross-machine claim or a release target.

| Measurement | Local result |
|---|---:|
| Cold owned-session start | 757.22 ms |
| Fresh compact observation, average | 2.35 ms |
| Cached compact observation, average | 0.005 ms |
| Deep DOM, average | 0.74 ms |
| Screenshot base64, average | 34.97 ms |
| Fast click, average | 14.75 ms |
| Human click, average | 327.83 ms |
| Compact / deep-DOM / screenshot context bytes | 5,485 / 9,149 / 35,421 |
| Glass RSS before / after workload | 3,207,168 / 5,419,008 bytes |
| Release / release-size binary | 4,398,992 / 3,153,792 bytes |
