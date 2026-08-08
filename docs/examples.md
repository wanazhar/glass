# Runnable examples

All examples belong to the `glass-browser` package. Run them from the
repository root with:

```console
cargo run -p glass-browser --example EXAMPLE_NAME
```

Browser-backed examples use Chrome/Chromium discovery. Set `CHROME_PATH` when
the browser is not in a standard location. Benchmark output is evidence only
for the recorded workload, environment, mode, iteration count, and browser.

## Learning examples

| Example | Environment | Run | What it proves |
|---|---|---|---|
| `basic_observe` | Chrome | `cargo run -p glass-browser --example basic_observe` | Start an owned session, collect compact structured context, print JSON, close cleanly |
| `guarded_click` | Chrome and an actionable current page | `cargo run -p glass-browser --example guarded_click` | Observe a revision and dispatch a click guarded by that exact revision |
| `semantic_extract` | Chrome and a current page | `cargo run -p glass-browser --example semantic_extract` | Extract one bounded typed field with semantic provenance |
| `mcp_initialize` | Browser-free | `cargo run -p glass-browser --example mcp_initialize` | Construct the Glass-aware MCP initialization request; it does not start a server |
| `workflow_resume` | Browser-free plus checkpoint path | `cargo run -p glass-browser --example workflow_resume -- checkpoint.json` | Parse and validate a persisted checkpoint before reconciliation |

`basic_observe`, `guarded_click`, and `semantic_extract` start on the active
blank page unless the example itself navigates. Adapt them in an application
by navigating before observation and by selecting a target actually present on
that page. They intentionally avoid hidden fallback behavior.

## Contract scorecards

| Example | Environment | Default workload | Output |
|---|---|---|---|
| `intent_scorecard` | Browser-free | Checked-in intent corpus | Classification/policy validation summary |
| `knowledge_scorecard` | Browser-free | Checked-in knowledge lifecycle corpus | Scope, freshness, quarantine, and advisory-use summary |
| `semantic_scorecard` | Browser-free | Checked-in semantic observation canaries | Canonical round-trip and privacy checks |
| `policy_benchmark` | Browser-free | Deterministic policy decisions | JSON timing/decision report |
| `scorecard` | Chrome | Checked-in browser task corpus, 10 iterations | Correctness, latency, observation bytes, and process memory evidence |
| `workflow_scorecard` | Chrome | Checked-in workflow corpus, 10 iterations | Workflow status, step-state, trace, and timing evidence |

Run browser-free scorecards:

```console
cargo run --release -p glass-browser --example intent_scorecard
cargo run --release -p glass-browser --example knowledge_scorecard
cargo run --release -p glass-browser --example semantic_scorecard
cargo run --release -p glass-browser --example policy_benchmark
```

Run browser-backed scorecards:

```console
GLASS_SCORECARD_ITERATIONS=20 \
  cargo run --release -p glass-browser --example scorecard

GLASS_WORKFLOW_SCORECARD_ITERATIONS=20 \
  cargo run --release -p glass-browser --example workflow_scorecard
```

`scorecard` also accepts `GLASS_SCORECARD_PROFILE`,
`GLASS_SCORECARD_TARGET_MODE`, and `GLASS_BINARY_PATH`. A report does not
support a comparative claim unless the compared runs use the same fixture,
mode, viewport, iteration policy, browser identity, and machine load controls.

## Browser performance benchmarks

| Example | Purpose | Main controls |
|---|---|---|
| `benchmark` | Cold/warm browser operations and controlled Playwright comparison inputs | `GLASS_BENCH_ITERATIONS`, `GLASS_BENCH_MODE`, `GLASS_BENCH_PAGE_CLASS_ITERATIONS`, `GLASS_BENCH_EXPENSIVE_ITERATIONS`, `GLASS_BENCH_ATTACH`, `GLASS_BENCH_REPORT`, `GLASS_BINARY_PATH` |
| `capture_benchmark` | Screenshot transport, encoding, allocation, optional Chrome trace | `GLASS_CAPTURE_ITERATIONS`, `GLASS_CAPTURE_WARMUP`, `GLASS_CAPTURE_MODE`, `GLASS_CAPTURE_TRACE_ITERATIONS`, `GLASS_CAPTURE_CHROME_TRACE`, `GLASS_CAPTURE_SKIP_MICROBENCH`, `GLASS_CDP_PORT` |
| `popup_benchmark` | Causally verified popup operation | `GLASS_POPUP_BENCH_ITERATIONS`, `GLASS_POPUP_BENCH_ARTIFACT` |
| `screencast_benchmark` | Bounded CDP screencast delivery | `GLASS_SCREENCAST_FRAMES`, `GLASS_SCREENCAST_WARMUP`, `GLASS_SCREENCAST_FORMAT`, `GLASS_SCREENCAST_QUALITY` |

Examples:

```console
GLASS_BENCH_ITERATIONS=50 \
  cargo run --release -p glass-browser --example benchmark

GLASS_CAPTURE_ITERATIONS=100 GLASS_CAPTURE_MODE=viewport \
  cargo run --release -p glass-browser --example capture_benchmark

GLASS_POPUP_BENCH_ITERATIONS=50 \
  cargo run --release -p glass-browser --example popup_benchmark

GLASS_SCREENCAST_FRAMES=120 \
  cargo run --release -p glass-browser --example screencast_benchmark
```

The benchmark rejects unsupported modes and out-of-range iteration counts. Do
not remove warmup, isolation, browser identity, or sample-count fields from a
published report.

## Browser-free resource benchmarks

| Example | Purpose | Run |
|---|---|---|
| `semantic_resource_benchmark` | Maximum-width 64-input Web IR task compilation, latency distribution, process peak RSS | `GLASS_SEMANTIC_BENCH_ITERATIONS=1000 cargo run --release -p glass-browser --example semantic_resource_benchmark` |
| `terminal_live_benchmark` | ANSI half-block rendering throughput, bytes, and retained latest-frame state | `GLASS_LIVE_BENCH_ITERATIONS=100 cargo run --release -p glass-browser --example terminal_live_benchmark` |

The semantic benchmark includes fixture and Rust runtime memory in its process
peak. The terminal benchmark measures the browser-free renderer, not SSH or
Chrome capture bandwidth.

## Environment variables

All benchmark iteration variables must be positive and within each example's
hard maximum. `CHROME_PATH` selects Chrome/Chromium for examples that require
it. `GLASS_BINARY_PATH` identifies a Glass executable only where the report
explicitly measures or invokes the binary. Output-path variables must point to
an authorized local destination; reports may contain environment and timing
metadata and should be reviewed before sharing.

See [benchmark methodology](../benchmarks/README.md),
[browser automation measurements](category-metric.md), and
[semantic resource budgets](architecture/semantic-resource-budgets.md).
