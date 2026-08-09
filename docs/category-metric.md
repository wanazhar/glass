# Browser automation measurements

This document defines optional measurements for Glass. It is methodology, not
a product guarantee or a permanent comparison with another tool. Results are
specific to the release, fixture set, browser build, host, and iteration count
used to produce them.

## Measurements

### Wrong-target actions

Count cases where the requested action reaches a different actionable element
than the fixture intended. Ambiguous, detached, covered, or moved targets must
fail rather than silently select a different element.

Transport errors, timeouts, and ordinary operation failures are reported as
failures; they are not converted into wrong-target results.

### Glass process memory

Measure peak resident memory for the Glass process separately from Chrome's
process tree. Record the host, build profile, browser version, fixture, warm-up
state, and iteration count with the result.

### Compact observation bytes

Measure the UTF-8 byte size of a compact `observe` response. Report the median
and p95 with JSON-RPC framing either consistently included or consistently
excluded. Record whether the observation was fresh or cached and which
optional fields were enabled.

## Reproduce the local scorecard

Build the current checkout and provide a detectable Chromium executable:

```sh
cargo build --package glass-dev --release --locked
GLASS_BINARY_PATH=target/release/glass \
  GLASS_SCORECARD_ITERATIONS=100 \
  CHROME_PATH=/path/to/chromium \
  cargo run --release --example scorecard
```

The scorecard writes a JSON report using
`benchmarks/report-schema.json`. Treat the report as the source of truth for
that run; do not copy its measurements into general product documentation
without retaining the release and environment details.

## Comparative acceptance

`benchmarks/run-acceptance.mjs` can run the versioned comparison harness when
the required external tools and browser are installed:

```sh
CHROME_PATH=/path/to/chromium node benchmarks/run-acceptance.mjs
```

The harness writes versioned evidence under `benchmarks/results/`. A partial,
missing, or invalid comparison is diagnostic only and must not be presented as
a completed comparison.

## Reporting rules

- Keep release, commit, browser, host, fixture, and iteration metadata with
  every measurement.
- Separate Glass memory from Chrome memory.
- Keep ordinary failures separate from wrong-target actions.
- Do not claim a leaderboard position from one host or one run.
- Do not use benchmark values as API, latency, or resource guarantees.
