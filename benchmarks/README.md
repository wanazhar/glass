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

`GLASS_SCORECARD_TEMPERATURE` is `warm` by default; use `cold` to reload the
fixture between iterations. `GLASS_SCORECARD_PROFILE` records the caller's
profile label. The report follows `report-schema.json` and records exact task
outcomes, wrong actions, per-scenario latency and CDP request counts, compact
context bytes, binary size, environment/tool versions, and process memory.
`resources.runner` is the Glass runner process only. Chrome RSS is the complete process tree
rooted at the owned browser PID; the two figures are deliberately disjoint.

Use `GLASS_SCORECARD_TARGET_MODE=wrong` as a harness self-test. It deliberately
chooses the wrong duplicate-label target, which must produce `wrong_action`, a
failed hard gate, and a non-zero failure count. Competitor adapters follow
`adapters/README.md` and keep their dependencies outside the Glass repository.

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
