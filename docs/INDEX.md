# Glass documentation

## User guides

- [Installation and operations](installation.md) — build, browser discovery,
  profiles, attach mode, logging, and production deployment.
- [CLI reference](cli.md) — global options, commands, targets, and output.
- [MCP integration](mcp.md) — stdio configuration, tools, session behavior,
  and security.
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
- [Benchmarks](../benchmarks/README.md) — performance methodology.

## Design

- [Architecture](architecture/README.md) — shared browser, session,
  observation, MCP, and TUI contracts.
- [Browser data plane](architecture/browser.md) — CDP, observation, action,
  and profile rules.
- [Agent browser automation](architecture/automation.md) — correctness,
  waiting, targeting, safety, and resource-budget contracts for the next
  generation.
- [Terminal UI](architecture/tui.md) — responsive TUI layout and worker
  lifecycle.

## Internal delivery history

The files under [`plan/`](plan/README.md) record implementation work and are
maintainer context, not public API commitments.
