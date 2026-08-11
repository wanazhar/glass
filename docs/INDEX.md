# Glass documentation

These guides describe the complete Glass product in this `0.3.4` source
checkout. `glass-browser 0.3.4` and `glass-dev 0.3.4` are the current published
crates. `Local only` means a capability exists in the checkout but is not part
of the published contract.
Platform certification remains separate from source availability; see
[cross-platform feature parity](feature-parity.md).

Glass has two installable product boundaries:

```text
glass-dev (complete product)
  ├─ glass            project runtime · TUI · agents · MCP · browser
  └─ glass-browser    browser-only compatible entry point
          │
          └─ exact dependency on glass-browser (focused package/library)
```

Start with the complete product unless you specifically need only browser
automation or the embeddable Rust crate. Package READMEs describe crates.io
installation; this index owns the cross-product documentation map.

## Choose your path

| You are… | Start here | Then read |
|---|---|---|
| Trying Glass for the first time | [Getting started](getting-started.md) | [Installation](installation.md), [CLI](cli.md) |
| Developing a project in the terminal | [Development Runtime](development-runtime.md) | [Development TUI](architecture/development-tui.md), [daemon](daemon.md) |
| Running resident Pi agents | [Native Pi SDK runtime](pi-sdk-runtime.md) | [Development Runtime](development-runtime.md), [daemon](daemon.md) |
| Automating a repair DAG | [Autonomous task DAGs](task-dag.md) | [Native Pi SDK runtime](pi-sdk-runtime.md), [daemon](daemon.md) |
| Comparing implementation approaches | [Measured experiments](experiments.md) | [Autonomous task DAGs](task-dag.md), [Development Runtime](development-runtime.md) |
| Safely opening an unfamiliar repository | [Workspace trust](workspace-trust.md) | [Security](../SECURITY.md), [Development Runtime](development-runtime.md) |
| Using Glass over SSH, Mosh, or an iPhone | [Mobile and remote](mobile-remote.md) | [Development Runtime](development-runtime.md) |
| Connecting an AI/MCP client | [MCP integration](mcp.md) | [Complete MCP tool catalog](mcp-tools.md), [policy](policy.md) |
| Embedding Glass in Rust | [Rust SDK](rust-sdk.md) | [Examples](examples.md), [docs.rs](https://docs.rs/glass-browser) |
| Building a browser workflow | [Semantic observations](semantic-observation.md) | [Actions](actions.md), [workflows](workflows.md), [semantic execution](semantic-execution.md) |
| Operating authenticated sessions | [Profiles](profile-ergonomics.md) | [Security](../SECURITY.md), [policy](policy.md) |
| Maintaining or releasing Glass | [Contributing](../CONTRIBUTING.md) | [Release checklist](release-checklist.md), [release evidence](release-evidence.md) |

The [complete feature reference](features.md) maps every implemented domain to
its CLI, TUI, MCP, and Rust entry points. It is the fastest way to answer “is
this supported and how do I invoke it?”

Core workflow guides state prerequisites, commands, state ownership, limits,
failure behavior, recovery, and verification. Dense catalogs such as the CLI,
MCP tool, feature, and SDK references optimize for exact lookup. Historical
records under `docs/plan/` preserve their original version claims and are not
current user instructions.

## Core user guides

- [Getting started](getting-started.md) — select an interface, run diagnostics,
  observe a page, perform a guarded action, and close cleanly.
- [Installation and operations](installation.md) — packages, ownership-aware
  updates, complete uninstall, browser discovery, profiles, attach mode,
  logging, policy, and deployment.
- [CLI reference](cli.md) — every command family, global option, locator,
  revision guard, input, and output convention.
- [Complete feature reference](features.md) — exhaustive capability-to-interface
  and capability-to-guide map.
- [Mobile and remote development](mobile-remote.md) — phone workspace, Herdr,
  Mosh, terminal-native pixels, semantic tap, and private Safari forwarding.
- [Development Runtime](development-runtime.md) — bounded files, editing, PTYs,
  diagnostics, graph, replay, experiments, collaboration, agents, and Neovim.
- [Workspace trust](workspace-trust.md) — explicit trust states, identity-bound
  persistence, configuration inspection, project-skill authority, and TUI UX.
- [Native Pi SDK runtime](pi-sdk-runtime.md) — direct AgentSession integration,
  bounded IPC, governed tools, lifecycle, persistence, and migration failures.
- [Autonomous task DAGs](task-dag.md) — automatic dispatch, verification,
  dependency wakeup, retries, budgets, controls, and durable scheduling.
- [Measured experiments](experiments.md) — isolated approaches, automatic
  evidence providers, metric provenance, deterministic ranking, and selection.
- [Actions and revisions](actions.md) and [action contract](action-contract.md) —
  unique targeting, execution phases, verification, failure, and recovery.
- [Semantic observations](semantic-observation.md) — levels, regions, records,
  revisions, deltas, explicit deep DOM, screenshots, and privacy.
- [Semantic execution](semantic-execution.md) — extraction evidence, stable Web
  IR, Task Protocol, deterministic compilation, live binding, and receipts.
- [Intent resolution](intent-resolution.md) — bounded candidates, policies,
  fingerprints, selection, and guarded execution.
- [Workflow definitions](workflows.md) and [workflow authoring](workflow-authoring.md)
  — validation, budgets, conditions, outputs, checkpoints, resume, recorder,
  formatting, linting, preview, and diff.
- [Persistent knowledge](knowledge.md) — profile/workspace scoping, freshness,
  lifecycle, advisory retrieval, migration, and management.
- [Local daemon](daemon.md) — Unix-socket lifecycle, isolated clients,
  resident MCP sessions, recovery, and mutation leases.
- [Extensions](extensions.md) and [experimental capabilities](experimental-capabilities.md)
  — manifests, permissions, sandbox gates, opt-in, and fail-closed availability.

## Interface references

- [Rust SDK](rust-sdk.md) — crate setup, ownership, common API flows, modules,
  Cargo features, errors, privacy, and shutdown.
- [Runnable examples](examples.md) — all checked-in Cargo examples, inputs,
  environment requirements, and output/claim boundaries.
- [MCP integration](mcp.md) — framing, initialization, agreement negotiation,
  cancellation, concurrency, errors, security, and response modes.
- [MCP tool catalog](mcp-tools.md) — every tool in the current checkout's
  negotiated conformance inventory grouped by domain.
- [MCP schema budget](mcp-schema-budget.md) and [registry metadata](mcp-registry.json)
  — inventory-size policy and distribution metadata.
- [Protocol](protocol.md) and [schema compatibility](schema-compatibility.md) —
  canonical request/response envelopes and additive version rules.
- [Experience Layer CLI](cli/experience.md) — shared response projections and
  local diagnostic result artifacts.
- [Knowledge schema](schema/knowledge-v1.schema.json) — machine-readable stored
  knowledge contract.

## Browser operations and safety

- [Policy reference](policy.md) — presets, capabilities, confirmation tokens,
  exact host rules, and denial behavior.
- [Persistent profile ergonomics](profile-ergonomics.md) — logged-in state,
  cookie import/export, ownership, and deletion.
- [Targets, actions, and revisions](actions.md) — locator forms and stale-target
  recovery.
- [Live-site smoke testing](site-smoke.md) — bounded manifests and safe probes.
- [Read-only real-site certification](reliability-real-site.md) and
  [production canary](production-canary.md) — operator boundaries for live
  evidence.
- [Bot protection and consent](bot-protection.md) and
  [detection-surface report](detection-surface.md) — legitimate access paths
  and transparent CDP behavior.
- [Security policy](../SECURITY.md) — trust boundaries and vulnerability
  reporting.

## Reliability, support, and performance

- [Reliability laboratory](reliability.md) and
  [reliability metrics](reliability-metrics.md) — scenarios, fixtures,
  forbidden outcomes, replay evidence, gates, and measurements.
- [Browser automation measurements](category-metric.md) — wrong-action count,
  runner RSS, observation bytes, and comparative rules.
- [Cross-platform feature parity](feature-parity.md) and
  [machine-readable parity matrix](feature-parity.json) — source inventory
  versus target certification.
- [Platform certification](ci-platform-certification.md) and
  [recorded platform evidence](local-platform.md) — evidence boundaries and
  reproduction.
- [Positioning](positioning.md) — Glass versus planners, hosted browsers,
  WebDriver frameworks, and raw CDP.
- [Semantic resource budgets](architecture/semantic-resource-budgets.md) —
  compiler, binding, tool gateway, broker, and file-read budgets.
- [Benchmark suite](../benchmarks/README.md) — methodology and reproducible
  commands.

## Architecture and design

- [Architecture overview](architecture/README.md) — ownership, lifecycle,
  state boundaries, cross-module flow, and module index.
- [Browser data plane](architecture/browser.md) — Chrome lifecycle, CDP,
  observation, action, profiles, errors, and tests.
- [Browser Host RFC](browser-host-rfc.md) and
  [backend capability matrix](backend-capability-matrix.json) — backend
  registration, capability evidence, BiDi boundary, and survivability.
- [Automation contracts](architecture/automation.md) — targeting, waiting,
  topology, input, safety, evidence, visual capture, and resource rules.
- [Experience Layer](architecture/experience.md) — shared user-facing result,
  reference, ownership, and recording contracts.
- [Terminal UI](architecture/tui.md), [Development TUI](architecture/development-tui.md),
  and [remote cockpit](architecture/mobile-cockpit.md) — layouts, interaction,
  workers, phone states, live frames, and SDK orchestration.
- [Connection-aware presentation](architecture/connection-presentation.md) and
  [browser connection/Remote View](architecture/browser-connection.md) — the
  0.3.3 environment-policy matrix, recovery controller, target picker, and
  secure same-session visual plane.
- [Semantic core hardening](architecture/semantic-core-hardening.md) — corpus,
  scoped evidence, live binding, state, continuity, privacy, and agent tools.
- [Ownership and compatibility](ownership.md) — module ownership and
  cross-interface change rules.
- [v0.3.2 development surface atlas](design/v0.3.2/development-surface-atlas.svg)
  — editor, live app, search, process, review, diff, replay, graph, workflow,
  experiment, and collaboration wireframes.
- [v0.3.3 remote cockpit design evidence](design/v0.3.3/README.md) — pinned
  phone concepts, renderer validation, and interaction-reference rationale.

## Maintainer and release guides

- [Documentation style](documentation-style.md) — terminology, procedure, and
  status conventions.
- [Contributing](../CONTRIBUTING.md) — source workflow, checks, security, and
  review expectations.
- [Release checklist](release-checklist.md) — repeatable two-package release.
- [Release evidence](release-evidence.md) — package, source release, platform,
  client, and validation evidence.
- [Changelog](../CHANGELOG.md) — user-visible changes.
- [30-second demo](demo.md) — short operator demonstration.
- [Delivery plans](plan/README.md) — implementation history and active work;
  these records are not substitutes for current user references.
