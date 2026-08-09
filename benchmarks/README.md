# Benchmarking Glass

Glass measures the local client and a local Chrome/Chromium instance. It is not
a cross-machine leaderboard: record the host OS/architecture, Chrome build,
iteration count, and whether the browser was already warm whenever comparing
runs.

## Terminal live renderer

The browser-free terminal benchmark isolates PNG decode and true-color ANSI
half-block sampling. It alternates two 320x180 frames so terminal diff work is
not optimized away:

```sh
GLASS_LIVE_BENCH_ITERATIONS=100 \
  cargo run -p glass-browser --release --example terminal_live_benchmark
```

A 2026-08-08 diagnostic run on Linux 6.17 aarch64 with Rust 1.97.0 measured
0.693 ms/frame at 40x12 cells (data), 0.836 ms/frame at 80x24 (balanced), and
0.983 ms/frame at 120x36 (smooth). These figures establish ample local render
headroom for the 3/6/12 FPS capture profiles; SSH throughput and PNG size still
govern end-to-end performance and must be evaluated on the user's link.

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

Measure the MCP schema footprint independently of browser startup:

```sh
GLASS_BINARY_PATH=target/debug/glass node benchmarks/schema-scoreboard.mjs
```

The report records tool count, serialized schema bytes, and a conservative
four-bytes-per-token estimate. The external workflow corpus is versioned in
[`external-corpus-plan.json`](external-corpus-plan.json); it supplements rather
than replaces the deterministic fixture gate.

The release comparison is pinned by `acceptance-v1.json`. The runner builds
Glass, installs competitor adapters into a temporary npm prefix, drives all
four required adapters with one explicitly selected Chromium executable, and
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
Release and platform evidence use the exact shapes in
`prerequisite-evidence-schema.json` and are supplied through
`GLASS_RELEASE_VALIDATION_REPORT` and `GLASS_PLATFORM_MATRIX_REPORT`; the
runner copies them into `raw/` and rejects evidence for another revision.
The repository's `scripts/compare-018.sh` generates local evidence when these
variables are absent and then invokes the real runner; it never creates a
passing stub. Even when every boolean gate passes,
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

### Glass MCP adapter

The `glass-scorecard.mjs` adapter drives Glass through its own MCP server
interface (`glass --mcp`), exercising the same JSON-RPC stdio transport that
external agent clients use. It is a zero-dependency Node.js script that treats
Glass as a black-box MCP server, validating the public tool surface before
running every v1 corpus scenario.

**Prerequisites:** Rust toolchain (`cargo`, `rustc`), a release build of Glass,
Chrome/Chromium, and Node.js ≥ 18. Glass must be built first:

```sh
cargo build --release
```

**Running standalone:**

```sh
GLASS_BINARY_PATH=target/release/glass \
  CHROME_PATH=/absolute/path/to/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  node benchmarks/adapters/glass-scorecard.mjs > glass-mcp-scorecard.json
```

The adapter reads `GLASS_BINARY_PATH` (defaults to `target/release/glass`) and
`CHROME_PATH` (required). It launches Glass with `--mcp --incognito
--interaction fast --profile scorecard` and communicates over newline-delimited
JSON-RPC. The required MCP tools are `navigate`, `click`, `evaluate`, `fillForm`,
`getText`, `observe`, `clickExpectPopup`, `listFrames`, `selectFrame`,
`acceptDialog`, `download`, and `wait`.

**Checkpoint support:** When run through the acceptance runner, the adapter
receives `GLASS_SCORECARD_CHECKPOINT_PATH` and writes revision-bound partial
evidence after every iteration using the same checkpoint schema as the other MCP
adapters.

**E2E gate:** Set `GLASS_E2E=1` to enable opt-in Glass-through-its-own-MCP
validation as part of the acceptance run. The acceptance runner distinguishes
this adapter (`tool.name: "glass-mcp"`) from the native Rust scorecard
(`tool.name: "glass"`).

### Adapter comparison

| Adapter | `tool.name` | Transport | Required tools | Checkpoint | RSS scope |
|---------|-------------|-----------|----------------|------------|-----------|
| Rust scorecard | `glass` | Direct Rust API | — | — | Primary runner process |
| Glass MCP | `glass-mcp` | JSON-RPC stdio | 12 MCP tools | Yes | MCP server process |
| Playwright | `playwright` | Direct Playwright API | — | — | Primary runner process |
| Playwright MCP | `playwright-mcp` | JSON-RPC stdio | 6 MCP tools | Yes | MCP server process |
| Agent Browser | `agent-browser` | JSON-RPC stdio | 5 MCP tools | Yes | MCP server process |
| Codex Browser | `codex-browser` | — | — | — | Unsupported |

The Rust scorecard adapter remains the primary Glass acceptance vehicle because
it reports `primary-non-browser-runner-process-rss-v1` and enables the declared
efficiency gate. The Glass MCP adapter provides independent validation that
Glass's own MCP surface can drive the full v1 adversarial corpus.



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
  GLASS_BENCH_EXPENSIVE_ITERATIONS=50 \
  cargo run --release --example benchmark > benchmark.json
```

`GLASS_BENCH_MODE` labels the intended control envelope without changing the
legacy cold or warm paths. It accepts `coverage` (the default) or
`sequential_controlled` (also accepted as `sequential-controlled`). Both modes
run one bounded operation at a time against the embedded local fixture;
`sequential_controlled` is the explicit mode for matched performance-baseline
runs, while `coverage` is the safe deterministic mode for exercising the full
diagnostic matrix. The report emits the canonical mode in both top-level
`mode` and `provenance.mode`. Unknown mode values fail closed before Chrome
startup.

The benchmark uses the local fixture and reports:

- repeated cold owned-session startup separately from page navigation;
- optional attach-to-existing startup against the benchmark's own verified
  Chrome endpoint;
- the first uncached compact observation after navigation, separately from
  startup and navigation latency;
- warm session reuse through the cached compact observation path;
- bounded semantic bootstrap latency, which is readiness evidence only;
- authoritative full observation latency (the uncached compact observation);
- an explicit post-verification checkpoint after a local fixture action;
- fresh and cached compact observation separately;
- explicit deep-DOM and screenshot capture latency;
- separate alternating fast-mode and human-mode click samples;
- compact, deep-DOM, and screenshot `PageContext` JSON payload byte counts;
- Glass's own RSS where the operating system exposes it, excluding Chrome child
  processes; and
- the supplied Glass binary size, or `null` if no executable is available at
  `GLASS_BINARY_PATH` or `target/release/glass`.

The additive `session_modes` object makes lifecycle comparisons explicit:
`cold_isolated` starts and closes a new owned incognito session for every
sample, while `warm_persistent` reuses one owned incognito session. Each mode
reports startup/session creation, navigation, semantic bootstrap, full
observation, and post-verification as independent checkpoint distributions.
Every repeated distribution includes bounded iteration count, mean, p50, p95,
minimum, and maximum milliseconds. Existing top-level keys and established
`results` entries remain present; the new checkpoint and mode summaries are
additive views over the same samples.

Each timing distribution also carries `statistical_evidence`: its completed
sample count, the bounded maximum sample count, the claim-eligibility
threshold, and a `percentile_scope` that states that p50 and p95 use the
nearest-rank method over all completed samples in that distribution. A
distribution with fewer than 20 completed samples is diagnostic only; it must
not support a performance regression, improvement, or threshold claim. The
report's `sampling` object records the normal, expensive, and page-class
counts, the 100-iteration caps, and this minimum claim count.

The top-level `claim_policy` repeats these bounds for consumers that need a
machine-readable gate: one sample is never claim-eligible, and comparative
claims require two independent eligible runs.

The additive `provenance` object records the canonical mode, UTC timestamp,
fixture and network scope, OS/architecture, bounded viewport dimensions,
iteration caps, and a Chrome/Chromium executable identity with a best-effort
version hint. It never records browser arguments, endpoint URLs, or secrets.
Use `coverage` for broad local correctness/performance diagnostics; use
`sequential_controlled` for a performance baseline only when repeating the
same fixture, Chrome build, viewport, machine, and capped sample counts. A
coverage run is not interchangeable with a sequential-controlled baseline,
even though both retain the cold-isolated and warm-persistent distributions.

Interpret `cold_isolated` as lifecycle/setup evidence: each sample pays for a
new owned incognito browser session. The separate startup distribution includes
process and CDP setup variability; first-observation distributions begin after
navigation and bootstrap. Interpret `warm_persistent` as steady-state reuse
evidence: one session is reused, and cached compact observation can make its
reuse path materially different from authoritative fresh observation.
Do not compare a cold distribution with a warm distribution as a direct
optimization claim unless the lifecycle, fixture, Chrome build, machine, and
sample counts are matched and the relevant distributions are claim-eligible.

The default report is local-only: its `data:text/html` fixture navigation,
semantic bootstrap, inspection, and post-verification measurements do not
contact public sites. The report marks `network_latency.included` as `false`
and identifies public-site latency as out of scope. Do not compare these local
fixture timings with public network latency without a separate, explicitly
labeled experiment.

`GLASS_BENCH_ITERATIONS` and `GLASS_BENCH_EXPENSIVE_ITERATIONS` are positive
sample-count controls capped at 100; their defaults remain 50. This cap keeps
startup churn, output distributions, and response-size measurements bounded.
`GLASS_BENCH_PAGE_CLASS_ITERATIONS` retains its separate cap of 100.

The additive `page_class_latency` matrix measures deterministic local data-URL
fixtures for `normal_static`, `dynamic_listing`, `challenge_interstitial`, and
`empty_unknown`. Each fixture has a bounded advisory `page_class` label from
the stable vocabulary (`normal`, `challenge`, or `empty`). Each summary reports
navigation, advisory semantic bootstrap, and authoritative full-observation
latency independently, with iterations, mean, p50, p95, minimum, and maximum
milliseconds. Navigation is local fixture load time; public-network latency is
not included and must be measured as a separate, explicitly labeled experiment.
The default benchmark never contacts a public website. Set
`GLASS_BENCH_PAGE_CLASS_ITERATIONS` to choose a bounded positive sample count
(capped at 100); it defaults to `GLASS_BENCH_ITERATIONS`.

The `cold_start` result preserves the scalar `chrome_launch_ms` and
`cold_first_observe_ms` fields for existing consumers. Its nested
`cold_owned_session_startup`, `cold_navigation`, `cold_semantic_bootstrap`,
`cold_first_observe`, `cold_full_observation`, and `cold_post_verification`
summaries report bounded repeated-sample distributions; the established
startup and first-observe summaries are also emitted as top-level `results`
operations. Startup timing ends after `BrowserSession::start` establishes CDP,
and navigation is awaited before evidence timers start, so network/navigation
latency is not charged to startup or evidence checkpoints.

The additive `latency_paths` object gives stable names for optimization
comparisons: `cold_owned_startup`, `attach_existing_startup`,
`warm_session_reuse`, `semantic_bootstrap`, and `full_observation`.
`warm_session_reuse` is the cached compact-observation path on an already
started session. `full_observation` is the authoritative uncached compact
observation; it must remain distinct from the advisory
`semantic_bootstrap` path. These entries reuse the corresponding benchmark
samples and do not alter runtime behavior.

Attach startup is skipped by default and is represented with
`status: "skipped"` in both `attach_existing_startup` fields. Set
`GLASS_BENCH_ATTACH=1` to attach sequentially to the benchmark-owned Chrome
endpoint after it has passed Glass's ownership checks:

```sh
GLASS_BENCH_ATTACH=1 \
  GLASS_BENCH_ITERATIONS=50 \
  GLASS_BENCH_EXPENSIVE_ITERATIONS=50 \
  cargo run --release --example benchmark > benchmark-attach.json
```

This opt-in mode does not require or connect to an externally managed browser.
It reports attach startup only; navigation and observation remain measured
separately. The warm semantic bootstrap operation is labeled
`semantic_bootstrap_warm`; it reports bounded readiness evidence only and is
not an action-success measurement.

The top-level `warm_startup_diagnostics` value and the nested
`cold_startup_diagnostics` value expose the bounded startup phase timings
captured by `BrowserSession`; they contain timing metadata only and remain
separate from navigation and evidence latency.
Cached compact observation remains labeled `observe_compact_cached` in the
general results.

`GLASS_BENCH_ITERATIONS` controls normal and warm operations. The expensive
operations, including repeated cold startup and first-observe samples, use
`GLASS_BENCH_EXPENSIVE_ITERATIONS` (the normal count by default). Set either
variable to a smaller positive value only for a diagnostic run; such a run
must not be used as release evidence. The hard cap is 100 iterations for each
general or expensive distribution (and separately for page-class samples).
For a performance claim, require at least 20 completed samples in every
distribution being claimed, inspect p50 and p95 together, and collect at least
two independent eligible runs under the same machine, Chrome build, fixture,
and lifecycle. One short run, one average, or a cold/warm mismatch is
insufficient evidence.

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

## Coverage runs are not performance baselines

Live-site coverage and local benchmark measurements answer different
questions. A manifest run may contain many sites (including a parallel
coverage batch) to expose compatibility, policy, redirect, challenge, and
readiness outcomes. Its aggregate throughput or wall time is not a comparable
performance baseline: sites have different content and network paths, startup
and robots delays vary, and parallel scheduling changes contention. In
particular, no single 50-site run proves a performance regression or
improvement.

Use each smoke result's per-site provenance (`requestedUrl`, `finalUrl`,
`sameOrigin`, `redirectCount`, `redirectEvidence`, `pageState`,
`classification`, `startupDiagnostics`, and `steps`) for diagnosis. For a
benchmark claim, select one controlled mode, match the machine, Chrome build,
fixture, viewport, and sample bounds, and report independent claim-eligible
runs. Do not combine cold startup, warm reuse, controlled
attach, or public-network timings into one unlabeled number.

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

## Public Scoreboard & Competitive Evidence

### Wrong-Action Adversarial Suite (B.5)

The v1 scenario corpus is a **public adversarial benchmark** for agent browser
automation. It is designed to catch:

- **Duplicate-label ambiguity** — two elements sharing an accessible name
- **Overlay blocking** — a modal or banner covering the intended target
- **Reflow displacement** — an element that moves on interaction
- **Delayed content** — DOM mutations after navigation
- **SPA navigation** — client-side routing without full page loads
- **Form submission** — typed input through accessibility-driven form fill
- **Popup causality** — verified popup opening from a click
- **Frame interaction** — clicking inside an iframe
- **Dialog handling** — JavaScript `alert`/`confirm`/`prompt`
- **Download integrity** — file download with content verification
- **Failure recovery** — continuing after a targeting failure

Glass's gate is **zero wrong actions**. Competitors may score lower; their
failures are published, not hidden.

### Token / Context Scoreboard (B.4)

Glass publishes median and p95 compact observe bytes alongside the competitive
acceptance results. This is the token-cost proxy: every byte becomes context
tokens the agent pays for.

Measurement methodology:
- Serialised `observe()` response JSON only (no JSON-RPC envelope, no MCP
  content wrapper)
- Warm session: page already loaded with cached observation available
- Same fixture, same Chrome build, same viewport as acceptance run

### Public Scorecard Package (B.3)

The scorecard is published as a versioned, reproducible package:

| Artifact | Location | Description |
|----------|----------|-------------|
| Scenario definitions | `crates/glass-browser/benchmarks/scenarios/v1.json` | Versioned corpus with expected outcomes |
| Report schema | `benchmarks/report-schema.json` | JSON Schema for scorecard reports |
| Methodology | This document | Reproducibility requirements |
| Fixture | `crates/glass-browser/tests/fixtures/scorecard.html` | Standalone HTML fixture |

Third parties can re-run and reproduce the structure by building Glass from
source and running the scorecard example. No credentials or external services
are required.

### Representative Corpus Plan (B.6)

The v1 corpus uses a deterministic fixture HTML file. A future v2 corpus will
add representative external workflows (login-free public pages) as a
complement to the fixture-based adversarial suite. The v2 corpus:

- Will **not** replace v1's deterministic fixture
- Will serve as a real-world complement, not a release hard gate
- Will document cold vs warm lifecycle explicitly
- Will use only login-free public pages

This plan is documented here for transparency; the methodology and scenario
selection are not yet stabilised.

## MCP Schema Budget

The current 0.3.3 `glass` source binary negotiates 133 browser, semantic, and
Development Runtime tools. The reproducible probe measures the serialized
`tools` array at 43,018 UTF-8 bytes, or 10,755 tokens using the documented
four-bytes-per-token estimate. JSON-RPC framing is excluded.

```console
cargo build --workspace --all-features --locked
GLASS_BINARY_PATH=target/debug/glass node benchmarks/schema-scoreboard.mjs
```

This value belongs to the measured checkout; it is not a fixed product promise
or a comparison with another server. The report includes each input schema's
byte size so growth can be attributed. Both current source-line executables
advertise this same registry; their effective capability agreements still
determine which optional tools are usable.

The review ceiling is 64 KiB. Changes must retain bounded inputs, stable typed
verbs, explicit heavy payloads, capability negotiation, and the before/after
report. Tool count alone is insufficient because schema sizes vary widely.

See [docs/mcp-schema-budget.md](../docs/mcp-schema-budget.md) for the full
methodology, current high-cost schemas, client rules, and rejection criteria.
