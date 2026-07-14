# Benchmarking Glass

Glass measures the local client and a local Chrome/Chromium instance. It is not
a cross-machine leaderboard: record the host OS/architecture, Chrome build,
iteration count, and whether the browser was already warm whenever comparing
runs.

## Task-success scorecard

The versioned local corpus in `scenarios/v1.json` covers duplicate labels,
overlays, reflow, delayed content, SPA navigation, forms, popups, frames,
dialogs, downloads, and recovery after a failed action. Run it against an
optimized Glass binary and retain the JSON output:

```sh
cargo build --release
GLASS_BINARY_PATH=target/release/glass \
  GLASS_SCORECARD_ITERATIONS=100 \
  cargo run --release --example scorecard > scorecard.json
```

The v1 corpus defines a warm, single-session run; it does not label fixture
reloads as cold browser starts. `GLASS_SCORECARD_PROFILE` records the caller's
profile label. The report follows `report-schema.json` and records exact task
outcomes, wrong actions, per-scenario latency and CDP request counts, compact
context bytes, binary size, environment/tool versions, and process memory.
`resources.runner` is the Glass runner process only. Chrome RSS is the complete
process tree rooted at the owned browser PID; the two figures are deliberately
disjoint. Peak values cover the measured post-startup workflow; `startup_ms`
is reported separately.

Use `GLASS_SCORECARD_TARGET_MODE=wrong` as a harness self-test. It deliberately
chooses the wrong duplicate-label target, which must produce `wrong_action`, a
failed hard gate, and a non-zero failure count. Competitor adapters follow
`adapters/README.md` and keep their dependencies outside the Glass repository.

### Competitive acceptance

The release comparison is pinned by `acceptance-v1.json`. The runner builds
Glass, installs competitor adapters into a temporary npm prefix, drives all
three runnable adapters with one explicitly selected Chromium executable, and
retains raw reports, stderr logs, environment metadata, and the gate decision:

```sh
CHROME_PATH=/absolute/path/to/chromium \
  node benchmarks/run-acceptance.mjs
```

The default release run uses 100 iterations and writes to
`benchmarks/results/compare-018/`. `GLASS_SCORECARD_ITERATIONS` may shorten a
diagnostic run, but such a run is not release evidence. A failed gate exits
with status 2 after writing the report; `GLASS_ACCEPTANCE_ALLOW_FAILURE=1` is
only for inspecting known local failures.

The mature agent baseline is Microsoft's released `@playwright/mcp@0.0.78`,
called over stdio MCP with `--isolated` and `--executable-path`. Playwright is
pinned independently at `1.61.1`. Neither dependency enters `Cargo.toml` or the
checkout. Codex browser automation is explicitly unsupported because this
harness has no callable, versioned black-box invocation contract for it.

Missing or incomplete adapters and mismatched controls block comparison.
Correctness and safety gates require Glass itself to have zero wrong actions
and perfect deterministic task success; comparator failures remain visible in
the published outcome summaries without becoming Glass failures. Glass must
also meet or exceed every comparator's task-success rate. The declared
efficiency gate currently compares peak RSS only between Glass and the direct
Playwright adapter because both report the primary non-browser runner process
while excluding Chrome under the exact versioned scope identity
`primary-non-browser-runner-process-rss-v1`. Playwright MCP excludes its separate client process,
so its RSS scope is explicitly incomparable and cannot create an efficiency
win. Without at least one strict comparable-scope win, a Glass resource-budget
failure, full release validation, or real-browser platform-matrix evidence,
best-in-class language remains blocked.
The latter evidence files use `{ "schema_version": 1, "git_revision": "...",
"passed": true }` and are supplied through `GLASS_RELEASE_VALIDATION_REPORT`
and `GLASS_PLATFORM_MATRIX_REPORT`; the runner copies them into `raw/` and
rejects evidence for another revision. Even when every boolean gate passes,
the retained comparison still needs interpretation before a leadership claim.
The remaining ratified thresholds use the same envelope plus a `metrics`
object and `GLASS_RATIFIED_GATES_REPORT`; absent or malformed metrics fail
closed. Every command has a deadline and bounded file capture. Setup and
adapter failures still produce `environment.json` and `acceptance.json`.
All prerequisite files follow `prerequisite-evidence-schema.json`, identify
their producer and run URL, and link each required check or platform row to its
raw result. The runner additionally enforces the exact release-check and target
sets in `acceptance-v1.json`; a top-level `passed` assertion alone is rejected.

Run the Playwright adapter from a temporary installation:

```sh
tmp_dir=$(mktemp -d)
npm install --prefix "$tmp_dir" --no-save playwright@1.61.1
NODE_PATH="$tmp_dir/node_modules" \
  CHROME_PATH=/usr/bin/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  node benchmarks/adapters/playwright-scorecard.mjs > playwright-scorecard.json
```

## Core browser workflow

### Popup completion diagnostic

Use the popup-only runner to separate healthy `mouseReleased` acknowledgement
waits from causally verified missing-ack recovery. It never runs the competitive
acceptance corpus:

```sh
CHROME_PATH=/absolute/path/to/chromium \
  GLASS_POPUP_BENCH_ITERATIONS=20 \
  cargo run --release --example popup_benchmark > popup-benchmark.json
```

The report publishes p50/p95/max distributions for each path and checks the
missing-ack recovery expectation of under one second. A path needs at least 20
samples to become claim-eligible; a one-iteration run is diagnostic only.

Build the optimized binary first, then write one JSON report per run:

```sh
cargo build --release
GLASS_BINARY_PATH=target/release/glass \
  GLASS_BENCH_ITERATIONS=50 \
  cargo run --release --example benchmark > benchmark.json
```

The benchmark uses the local fixture and reports:

- one cold owned-session startup (`cold_start_ms`);
- fresh and cached compact observation separately;
- explicit deep-DOM and screenshot capture latency;
- separate alternating fast-mode and human-mode click samples;
- compact, deep-DOM, and screenshot `PageContext` JSON payload byte counts;
- Glass's own RSS where the operating system exposes it, excluding Chrome child
  processes; and
- the supplied Glass binary size, or `null` if no executable is available at
  `GLASS_BINARY_PATH` or `target/release/glass`.

`GLASS_BENCH_ITERATIONS` controls normal operations. Deep DOM, screenshots, and
human clicks use one fifth of that count (at least five samples) because they
are deliberately more expensive. Use p50 and p95, not one average, and avoid
claiming a regression or improvement from a single run.

The payload fields measure serialized `PageContext` JSON only. They do not
include the JSON-RPC envelope or MCP's separate image-content wrapper. The RSS
fields measure the Glass process only; Chrome's multi-process memory footprint
must be measured and reported separately if it is relevant to a comparison.

For size-focused builds, point the same report at the size profile artifact:

```sh
cargo build --profile release-size
GLASS_BINARY_PATH=target/release-size/glass \
  GLASS_BENCH_ITERATIONS=50 \
  cargo run --release --example benchmark > benchmark-size.json
```

## Fair Playwright comparison

Playwright is optional and should be installed outside this repository. Point
both tools at the same Chrome binary, fixture, iteration count, machine, and
warm/cold state:

```sh
tmp_dir=$(mktemp -d)
npm install --prefix "$tmp_dir" --no-save playwright@1.61.1
NODE_PATH="$tmp_dir/node_modules" \
  CHROME_PATH=/usr/bin/chromium-browser \
  GLASS_BENCH_ITERATIONS=50 \
  node benchmarks/playwright.mjs
```

Compare only like-for-like browser operations. Playwright has no equivalent
compact observation cache or built-in human pointer path, so cached-observation
and human-click figures are useful within Glass rather than as identically
named cross-tool measurements. Binary-size comparisons should state whether
Node, Playwright, and browser downloads are included.

## Capture investigations

The focused capture investigation and dedicated drivers remain available for
image-pipeline work:

```sh
GLASS_CAPTURE_ITERATIONS=50 cargo run --release --example capture_benchmark
GLASS_SCREENCAST_FRAMES=120 cargo run --release --example screencast_benchmark
```

See [capture-report.md](capture-report.md) for capture-specific methodology and
results.
