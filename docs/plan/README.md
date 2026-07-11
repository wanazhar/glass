# Performance overhaul delivery plan

Status: In progress

This plan implements the agreed goal: make Glass the fastest and smallest practical local CDP automation client while correcting behavior that contradicts its public interface.

## Task order

1. [perf-001](tasks/perf-001.md) — compact observation and CDP hot path.
2. [lifecycle-002](tasks/lifecycle-002.md) — explicit attachment, profile ownership, managed Chromium, and real incognito.
3. [action-003](tasks/action-003.md) — stable references and low-cost reliable input primitives.
4. [mcp-004](tasks/mcp-004.md) — compact persistent MCP and deterministic CLI data flow.
5. [tui-005](tasks/tui-005.md) — responsive worker-based TUI with the existing layout.
6. [verify-006](tasks/verify-006.md) — benchmarks, memory checks, and end-to-end regression coverage.

Each task is developed, tested, reviewed, merged, and locally committed before the next dependent task starts. Commit subjects use Conventional Commits.

## Baseline

The pre-overhaul release binary is 4.1 MB. Existing build, fmt, Clippy, unit, and Chromium smoke checks pass. The acceptance suite records deltas rather than claiming a performance win without a reproducible measurement.

## Shared verification

Every completed task runs its targeted tests plus `cargo fmt --all -- --check` and strict Clippy. After merge, run the full test suite and record any environment-limited checks.
