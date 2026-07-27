# Scorecard adapter contract

Competitor adapters live outside the Glass dependency graph. An adapter runs
the versioned fixture scenarios and writes one JSON report matching
`benchmarks/report-schema.json`. It must use the same Chrome executable,
viewport, corpus version, iteration count, and warm profile metadata. A future
cold corpus version must define equivalent browser lifecycle semantics before
any adapter emits it.

The adapter reports generic `resources.runner` data and states whether that
scope covers one runner process or its complete non-browser process tree,
separately from Chrome's process tree. A scenario is successful only when its exact expected state is
observed. Selecting a different actionable element is `wrong_action`, never a
timeout or a partial success. Adapter dependencies must be installed in a
temporary directory and must not be added to `Cargo.toml` or this repository.

`playwright-scorecard.mjs` is the reference external adapter. Its optional
Playwright installation is described in `benchmarks/README.md`. Metrics that
cannot be collected through Playwright's public API are explicitly `null`, not
estimated or silently omitted.

`playwright-mcp-scorecard.mjs` is the released agent-browser adapter. It is a
dependency-free MCP client that invokes a separately installed, exactly pinned
`@playwright/mcp` executable. Complex topology and diagnostic scenarios use
public tools only and reports popup, frame, recovery, and download scenarios as
`unsupported` where version `0.0.78` exposes no typed public primitive or
completed artifact. It therefore does not claim safety equivalence with Glass's
default policy. The adapter validates its required tool surface before running.
Its runner RSS covers the MCP server process only; unavailable client and
Chrome process-tree metrics remain `null` with an explicit scope description.

The acceptance runner gives this released MCP adapter a bounded process ceiling
of two minutes plus 30 seconds per controlled iteration (52 minutes for the
ratified 100-iteration run), while retaining the common per-request deadline
and all controlled-run inputs. The runner supplies
`GLASS_SCORECARD_GIT_REVISION` and `GLASS_SCORECARD_CHECKPOINT_PATH`; after each
iteration the adapter atomically
publishes revision-bound partial evidence there. A timeout retains a valid
checkpoint for diagnosis, but it remains explicitly partial and cannot pass an
acceptance gate. A complete run removes the superseded checkpoint and emits the
unchanged final report on stdout.
Before spawning an adapter, the runner removes any prior checkpoint and binds
the new one to a fresh cryptographic run ID and invocation start time so a
same-revision retry cannot inherit stale progress.

`agent-browser-scorecard.mjs` is the agent-browser comparator adapter. It is a
dependency-free MCP client that invokes a separately installed, exactly pinned
`agent-browser` executable (from npm) in MCP server mode. The adapter uses
`agent-browser mcp --tools all --no-auto-dialog` and validates the required
tool surface (`navigate`, `click`, `fill`, `snapshot`, `evaluate`) before
running. Popup and download scenarios are reported as `unsupported` because
agent-browser's MCP surface has no published typed primitives for causal popup
verification or download-integrity assertions; the matrix fails closed.

**Installation (outside the Glass crate):**

```sh
npm install -g agent-browser@1.3.30
agent-browser install  # download managed Chromium
```

Set `AGENT_BROWSER_COMMAND` and `AGENT_BROWSER_VERSION` in the acceptance
environment. The runner supplies these if the adapter is listed in
`acceptance-v1.json`.

**Running standalone:**

```sh
AGENT_BROWSER_COMMAND=$(which agent-browser) \
AGENT_BROWSER_VERSION=1.3.30 \
CHROME_PATH=/path/to/chromium \
GLASS_SCORECARD_GIT_REVISION=$(git rev-parse HEAD) \
GLASS_SCORECARD_CHECKPOINT_PATH=/tmp/agent-browser-checkpoint.json \
GLASS_SCORECARD_RUN_ID=$(uuidgen) \
GLASS_SCORECARD_STARTED_AT=$(date -Iseconds) \
GLASS_SCORECARD_ITERATIONS=10 \
node benchmarks/adapters/agent-browser-scorecard.mjs
```
