# Category Metric: Wrong-Action Scoreboard

Glass publishes and maintains a public scoreboard of three category-defining
metrics for agent browser automation. These are the numbers Glass uses to
claim best-in-class — and the numbers any competitor can reproduce with the
same fixtures and Chrome build.

## The Three Metrics

### 1. Wrong-Action Count

The number of times an agent selects a *different actionable element* than the
one intended. This is the hardest gate: **Glass must score 0 wrong actions**
on the published v1 corpus of 11 adversarial scenarios.

A wrong action is:
- Clicking the wrong duplicate-label button
- Clicking an overlay-covered element
- Clicking a reflow-displaced element
- Submitting an empty form when a value was required

Timeouts, navigation failures, and transport errors are *failures*, not wrong
actions. Wrong-action tracking is the honesty contract: Glass reports exactly
which element was selected, not whether the page subsequently matched an
expected state.

**Current Glass result:** 0 wrong actions (gate must hold on every release).

### 2. Runner RSS (Memory)

Peak resident set size of the Glass runner process during a warm multi-scenario
workload. This excludes Chrome's process tree — it measures Glass's own memory
footprint.

**Current Glass result:** ~8.9 MB (vs Playwright MCP ~196 MB).

Glass achieves this through:
- Zero retained DOM/AX trees beyond a single cached observation
- Bounded event broadcasting (no unbounded channel growth)
- No embedded browser runtime
- Streamed or moved payloads; never cloned through generic broadcasts

### 3. Compact Observe Bytes

Median and p95 byte size of a single compact `observe` response. This is the
token-cost proxy: every byte Glass emits becomes context tokens the agent pays
for.

**Current:** Tracked per release; target: below 2 KiB median on warm fixtures.

Glass achieves this through:
- Accessibility-only compact projection (no DOM by default)
- 32-control interactive-element limit with relevance ranking
- UTF-8 bounded name/role fields
- Shadow-host path breadcrumbs trimmed to 3 entries × 64 bytes
- Optional form-value inclusion (off by default, policy-gated)

## Reproducing the Scoreboard

Any party can reproduce these numbers:

```sh
cargo build --release
GLASS_BINARY_PATH=target/release/glass \
  GLASS_SCORECARD_ITERATIONS=100 \
  CHROME_PATH=/path/to/chromium \
  cargo run --release --example scorecard
```

The scorecard emits a JSON report matching `benchmarks/report-schema.json`.
Competitor adapters run the same fixtures and report the same schema.

## Competitive Acceptance

`benchmarks/run-acceptance.mjs` pins a versioned comparison contract:

```sh
CHROME_PATH=/path/to/chromium \
  node benchmarks/run-acceptance.mjs
```

This runs all three required adapters (Glass, Playwright, Playwright MCP)
and optionally the agent-browser adapter against the same Chromium binary
and v1 corpus. Results are written to `benchmarks/results/compare-018/`.

## Release Policy

- **Every release that claims progress** publishes an updated scoreboard.
- **"Best in class" language** is blocked until all three metrics pass their
gates with 0 wrong actions on the published corpus.
- **A new corpus version** is introduced with a new schema version and at
least one release cycle of documentation before it gates release language.

## Category Positioning

Glass owns the *wrong-action + RSS + observe-bytes* scoreboard as its category
metric, analogous to:
- How autonomy tools cite WebVoyager or WebArena task success
- How QA frameworks cite pass/fail assertion counts
- How cloud browsers cite concurrent session scaling

Glass's category is **correctness-per-byte**: the fewest wrong actions with
the smallest memory and token footprint. This is a narrow claim, but the one
that matters for agents that pay per token and cannot afford silent wrong
actions.
