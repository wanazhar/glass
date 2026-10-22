# Glass

**EARLY ALPHA** — Glass is a local coding workspace where a person and an AI
agent can work on the same project. The terminal, editor, project files, Git,
tests, agent conversation, and browser tools share one workspace.

Glass is for developers who want to work beside an agent, inspect its changes,
and take over at any time. The workspace keeps the conversation and project
work close together, so you can edit, run checks, review changes, and test a
web interface without switching between separate tools.

> Glass is early alpha. Interfaces and behavior can change. Review important
> changes and keep your normal project backups.

## What Glass does

- **Work on a project together.** Glass provides a terminal UI, shared editor,
  Git views, project commands, diagnostics, and a resident agent.
- **Keep work visible.** Human and agent changes appear in the same project
  workspace, with revisions, attribution, and review tools.
- **Test web interfaces.** The browser workspace can inspect a page, perform
  actions, and verify the result. Browser automation is part of the workspace
  or can be installed on its own.

Resident agents use the standalone `glass-core` engine by default. Glass owns
project access and tool execution; Glass Core runs the model session. See the
[resident harness guide](docs/glass-core-harness.md) for setup and details.

## Choose a package

| Package | Commands | Use it for |
|---|---|---|
| `glass-dev` | `glass`, `glass-browser` | The complete coding workspace, resident agent integration, MCP server, and browser tools. |
| `glass-browser` | `glass-browser` | Browser automation and the reusable `glass_browser` Rust library. |
| `glass-core` | `glass-core`, `gc` | The standalone coding harness used by Glass resident agents. |

The two packages provide the same `glass-browser` command, so install only one
of them in a Cargo install root. Read the [installation guide](docs/installation.md)
before changing package ownership.

## Get started

Install the complete workspace, check the installation, and open a project:

```console
cargo install glass-dev --locked
glass doctor
cd /path/to/project
glass
```

Glass can inspect and work with a project without starting a browser. To use
resident agents, install or configure the Glass Core engine as described in
[Getting started](docs/getting-started.md). Glass Core's source, independent
Cargo workspace, lockfile, and standalone installer live under
[`products/glass-core`](products/glass-core/README.md). To automate a browser
without the full workspace, install `glass-browser` instead.

## Documentation

- [Getting started](docs/getting-started.md) — choose a workflow and run it.
- [Installation and operations](docs/installation.md) — install, update,
  diagnose, and remove Glass.
- [Development Runtime](docs/development-runtime.md) — files, editor, tests,
  processes, diagnostics, collaboration, and agents.
- [Glass Core resident harness](docs/glass-core-harness.md) — configure and use
  the default agent engine.
- [Glass Core product](docs/glass-core-product.md) — build and install the
  standalone coding harness, whose Rust packages share Glass's root workspace
  and version source.
- [Browser automation](docs/INDEX.md#browser-operations-and-safety) — inspect
  pages, perform actions, and verify results.
- [CLI reference](docs/cli.md) and [MCP integration](docs/mcp.md) — command-line
  and client interfaces.
- [Documentation index](docs/INDEX.md) — topic and section routes for current
  guides, package references, and historical records.
- [Package and feature comparison](docs/features.md) — supported interfaces
  and package boundaries.

## Current source and release evidence

| Source package versions | Status |
|---|---|
| `glass-browser 0.3.14`, `glass-dev 0.3.14` | Current source checkout; see [release evidence](docs/release-evidence.md) for publication status. |

The package version does not change the early-alpha status of the product.
See [platform feature parity](docs/feature-parity.md) for tested platform
coverage and [release evidence](docs/release-evidence.md) for publication
status. Current guides describe this checkout; release records describe the
source and artifacts they name.

## License

See [LICENSE](LICENSE).
