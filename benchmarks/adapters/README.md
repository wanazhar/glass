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
the server's published `browser_run_code_unsafe` MCP tool; this is recorded as
a privileged released tool, not as a safety equivalence with Glass's default
policy. The adapter validates its required tool surface before running. Its
runner RSS covers the MCP server process only; unavailable client and Chrome
process-tree metrics remain `null` with an explicit scope description.
