# Glass documentation

## User guides

- [Installation and operations](installation.md) — build, browser discovery,
  profiles, attach mode, logging, and production deployment.
- [CLI reference](cli.md) — global options, commands, targets, and output.
- [Actions and revisions](actions.md) — guarded actions, typed outcomes, and
  bounded verification evidence.
- [Workflow definitions](workflows.md) — versioned declarative workflows,
  typed inputs, budgets, and pre-execution validation.
- [Workflow authoring](workflow-authoring.md) — YAML compilation, diagnostics,
  safe input inference, preview, diff, and the semantic recorder boundary.
- [Action contract](action-contract.md) — the canonical `0.1.18` execution
  envelope, failure phases, and explicit recovery policies.
- [Schema compatibility](schema-compatibility.md) — additive cross-interface
  field and versioning rules.
- [MCP integration](mcp.md) — stdio configuration, tools, session behavior,
  and security.
- [Local daemon](daemon.md) — Unix-socket lifecycle, status, and per-client
  MCP session boundaries.
- [Extensions](extensions.md) — metadata-only extension manifests and
  permission boundaries.
- [Semantic observations](semantic-observation.md) — bounded page and region
  state, levels, expansion, revisions, diffs, and privacy boundaries.
- [Intent resolution](intent-resolution.md) — bounded candidate resolution,
  confidence policies, guarded execution, semantic workflow steps, and TUI
  review.
- [Persistent knowledge](knowledge.md) — scoped records, freshness,
  lifecycle management, privacy boundaries, CLI/MCP operations, and fixtures.
- [Persistent knowledge contract](schema/knowledge-v1.schema.json) — the
  machine-readable v1 record and snapshot schema.
- [MCP schema budget](mcp-schema-budget.md) — tool inventory, schema sizes,
  and design principles for keeping the surface lean.
- [Logged-in session ergonomics](profile-ergonomics.md) — using persistent
  profiles to carry authenticated state without pasting credentials.
- [Policy reference](policy.md) — named presets, capabilities, confirmation
  tokens, and host allowlisting.
- [Positioning](positioning.md) — where Glass fits: control plane vs planner
  vs cloud browser, and when to use each tool.
- [Category metric](category-metric.md) — wrong-action count, runner RSS,
  and observe-bytes scoreboard.
- [Reliability metrics](reliability-metrics.md) — versioned action, verification,
  recovery, latency, and resource evidence.
- [Reliability laboratory](reliability.md) — versioned scenarios, adversarial
  fixtures, replay validation, and the release gate.
- [Read-only real-site certification](reliability-real-site.md) — safe
  operator boundaries for live evidence without credentials or destructive
  actions.
- [Production canary](production-canary.md) — hardened, non-destructive live-site
  probe procedure and evidence requirements.
- [Bot-protection & consent walls](bot-protection.md) — legitimate access paths
  when sites challenge CDP automation.
- [Detection-surface report](detection-surface.md) — what stock CDP-driven
  Chrome exposes and what Glass does (and does not) attempt to hide.
- [Security policy](../SECURITY.md) — trust boundaries and vulnerability
  reporting.
- [Changelog](../CHANGELOG.md) — user-visible release changes.

## Maintainer guides

- [Contributing](../CONTRIBUTING.md) — development workflow and checks.
- [Release checklist](release-checklist.md) — repeatable `0.x` release process.
- [Benchmarks](https://github.com/wanazhar/glass/tree/main/benchmarks) — performance methodology.

## Design

- [Architecture](architecture/README.md) — shared browser, session,
  observation, MCP, and TUI contracts.
- [Browser data plane](architecture/browser.md) — CDP, observation, action,
  and profile rules.
- [Automation contracts](architecture/automation.md) — correctness,
  waiting, targeting, safety, and resource-budget contracts for the next
  generation.
- [Terminal UI](architecture/tui.md) — responsive TUI layout and worker
  lifecycle.
