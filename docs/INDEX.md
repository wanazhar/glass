# Glass documentation

These guides describe the current Glass source tree. They do not claim that
every change in this tree is part of a published release. The
[release evidence](release-evidence.md) records which packages and artifacts
were published. The [feature parity guide](feature-parity.md) records platform
checks and their limits.

Glass has two installable products and one browser implementation crate:

```text
glass-dev (complete product)
  ├─ glass            project workspace · TUI · agents · MCP
  └─ glass-browser    browser CLI and library
          │
          └─ glass-browser-core (native browser engine implementation)
```

Install `glass-dev` for the project workspace. Install `glass-browser` when
you need only browser tools or its Rust API. `glass-browser-core` is an
implementation crate used by `glass-browser`; it is not a third user-facing
product.

The resident agent engine has a similar name but a different role:
`glass-core` owns resident agent execution, permissions, provider/session state,
and turn lifecycle. Glass is the workspace that routes input to Core, presents
Core events, and supplies host integrations. `glass-browser-core` contains the
native browser engine. The [resident harness guide](glass-core-harness.md)
explains Core setup and the host boundary. Its
standalone source product lives at [`products/glass-core`](../products/glass-core/README.md);
see the [standalone product guide](glass-core-product.md) for its own build,
install, package metadata, and license boundary.

This index is the repository's topic and section home. The generated
[Markdown catalog](documentation-catalog.json) records every Markdown file's
class, topic, canonical route, and heading anchors. Run the
[documentation change check](../scripts/check-documentation-wiki.py) to see
page, section, anchor, and body-only changes against a base commit.

The [workspace implementation matrix](../BUG_HUNT_REPORT.md) tracks the
current GC/Glass ownership and end-to-end evidence for the workspace changes
under review. The [bug report note](../BUG_REPORT.md) routes readers to that
matrix.

## Choose your path

| You are… | Start here | Then read |
|---|---|---|
| Trying Glass for the first time | [Getting started](getting-started.md) | [Installation](installation.md), [CLI](cli.md) |
| Developing a project in the terminal | [Development Runtime](development-runtime.md) | [Development TUI](architecture/development-tui.md), [daemon](daemon.md) |
| Editing, reviewing, or collaborating on source | [Development Runtime](development-runtime.md) | [Development TUI](architecture/development-tui.md), [MCP tool catalog](mcp-tools.md) |
| Launching or delegating to coding harnesses | [Development Runtime](development-runtime.md) | [CLI](cli.md), [Glass Core resident harness](glass-core-harness.md) |
| Running resident agents and inspecting GC durable tasks | [Glass Core resident harness](glass-core-harness.md) | [CLI](cli.md), [Development Runtime](development-runtime.md), [daemon](daemon.md) |
| Searching the web or reading an online source | [Glass Core web tools](../products/glass-core/docs/web-tools.md) | [Glass Core resident harness](glass-core-harness.md) |
| Understanding Core and Glass ownership | [Glass Core resident harness](glass-core-harness.md) | [Development TUI](architecture/development-tui.md), [Development Runtime](development-runtime.md) |
| Building or installing the standalone coding harness | [Standalone Glass Core](glass-core-product.md) | [Glass Core resident harness](glass-core-harness.md) |
| Reviewing the repository migration evidence | [Glass Core monorepo migration](migration/glass-core-monorepo.md) | [Maintainer handbook](maintainers/README.md) |
| Automating a repair DAG | [Autonomous task DAGs](task-dag.md) | [Glass Core resident harness](glass-core-harness.md), [daemon](daemon.md) |
| Running persistent analysis code | [Persistent kernels](kernels.md) | [MCP tool catalog](mcp-tools.md), [Security](../SECURITY.md) |
| Debugging a resident process | [Resident debugger](debugger.md) | [Development Runtime](development-runtime.md), [Persistent kernels](kernels.md) |
| Explaining a regression path | [Development Graph and replay](development-graph.md) | [Measured experiments](experiments.md), [Autonomous task DAGs](task-dag.md) |
| Comparing implementation approaches | [Measured experiments](experiments.md) | [Autonomous task DAGs](task-dag.md), [Development Runtime](development-runtime.md) |
| Understanding workspace integration authority | [Development Runtime](development-runtime.md) | [Customization governance](customization-governance.md), [Security](../SECURITY.md) |
| Auditing skills, hooks, and tools | [Customization governance](customization-governance.md) | [Development Runtime](development-runtime.md), [Security](../SECURITY.md) |
| Using Glass over SSH, Mosh, or an iPhone | [Mobile and remote](mobile-remote.md) | [Development Runtime](development-runtime.md) |
| Connecting an AI/MCP client | [MCP integration](mcp.md) | [Complete MCP tool catalog](mcp-tools.md), [policy](policy.md) |
| Embedding Glass in Rust | [Rust SDK](rust-sdk.md) | [Examples](examples.md), [glass-browser docs.rs](https://docs.rs/glass-browser), [glass-dev docs.rs](https://docs.rs/glass-dev) |
| Building a browser workflow | [Semantic observations](semantic-observation.md) | [Actions](actions.md), [workflows](workflows.md), [semantic execution](semantic-execution.md) |
| Measuring native browser site journeys | [Native browser journey corpus](architecture/native-journey-corpus.md) | [Native browser engine](architecture/native-engine.md), [backend capability matrix](backend-capability-matrix.json) |
| Operating authenticated sessions | [Profiles](profile-ergonomics.md) | [Security](../SECURITY.md), [policy](policy.md) |
| Maintaining or releasing Glass | [Contributing](../CONTRIBUTING.md) | [Release checklist](release-checklist.md), [release evidence](release-evidence.md) |

## Product and package boundaries

- [Glass product overview](../README.md) — early-alpha scope, workspace role,
  and first commands.
- [Package and feature comparison](features.md) — installable products,
  package ownership, and capability-to-interface map.
- [Glass Core product](glass-core-product.md) and
  [Glass Core source product](../products/glass-core/README.md) — standalone
  harness role, source/workspace relationship, build, install, and licensing.
  The Glass CLI forwards unambiguous `glass <core-command>` names and the full
  `glass core ...` namespace to the monorepo Core binary; this does not resolve
  the independent GCN product. In the workspace, the slash palette's Core
  model picker uses the resident runtime, and unsupported interactive commands
  do not open a second TUI. Forwarded human CLI headings and command guidance
  identify whether the caller used `glass`, `glass core`, or standalone `gc`.
- [Glass Core documentation index](../products/glass-core/docs/INDEX.md) —
  product setup, terminal workflow, provider configuration, permissions,
  integrations, runtime, and development records.
- [Glass Core extension catalog](../products/glass-core/extensions/README.md) —
  installed and optional extensions, kept separate from Glass workspace
  extensions.
- [Browser crate](../crates/glass-browser/README.md) and
  [Glass development crate](../crates/glass-dev/README.md) — package-level
  APIs and ownership.
- Workspace implementation packages: [`glass-core`](../crates/glass-core/README.md),
  [`glass-browser-core`](../crates/glass-browser-core/README.md),
  [`glass-runtime-client`](../crates/glass-runtime-client/README.md),
  [`glass-mod-api`](../crates/glass-mod-api/README.md), and
  [`glass-tui`](../crates/glass-tui/README.md). These are Cargo workspace
  members, not additional user-facing Glass products.

## Setup, authentication, providers, and models

- [Getting started](getting-started.md) and
  [installation and operations](installation.md) — choose an install, verify
  it, select a browser, and resolve common first-run problems.
- [Glass Core resident harness](glass-core-harness.md) — engine selection,
  provider readiness, authentication ownership, model configuration, and
  session setup.
- Glass Core [getting started](../products/glass-core/docs/getting-started.md),
  [configuration](../products/glass-core/docs/configuration.md), and
  [documentation index](../products/glass-core/docs/INDEX.md) — provider
  connection, authentication, model selection, and runtime settings.

## Workspace operations

- [Development Runtime](development-runtime.md) — project files, native
  editor, PTYs, Git, LSP diagnostics, graph, replay, collaboration, and agents.
- [Development TUI](architecture/development-tui.md) and
  [terminal UI architecture](architecture/tui.md) — workspace surfaces,
  conversation and source focus, live GC commands, keyboard and mouse
  interaction, responsive layout, and workers.
- [Mobile and remote development](mobile-remote.md) — SSH/Mosh, phone access,
  terminal rendering, and browser controls.
- [Processes and daemon](daemon.md) — persistent clients, project sessions,
  leases, shutdown, and recovery.
- [Browser operations and safety](#browser-operations-and-safety) — profiles,
  browser targets, actions, policies, observations, and live-site checks.

## Orchestration, recovery, and review

- [Autonomous task DAGs](task-dag.md) — task creation, dependencies, dispatch,
  verification, retries, budgets, and controls.
- [Development Runtime: agents and authority](development-runtime.md#agents-and-actor-authority)
  — project threads, attribution, and mutation ownership.
- [Glass Core resident harness](glass-core-harness.md) — provider session
  lifecycle, host runtime protocol, and tool handoff.
- [Workflows](workflows.md) and [workflow authoring](workflow-authoring.md) —
  checkpoints, resume, validation, and bounded execution.
- [Actions and revisions](actions.md) — guarded changes, receipts, review,
  failure, and recovery.
- [Customization governance](customization-governance.md) — skills, hooks,
  tool evidence, and integration policy.

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
- [Customization governance](customization-governance.md) — authority classes,
  skill provenance, hook/tool evidence, effective mutation policy, and labels.
- [Glass Core resident harness](glass-core-harness.md) — the default resident
  engine: host runtime protocol, session lifecycle, tool execution, permissions,
  engine selection, and reported capability gaps.
- [Glass Core product](glass-core-product.md) — build and install the
  standalone coding harness, whose Rust packages share Glass's root workspace
  and version source.
- [Glass Core monorepo migration](migration/glass-core-monorepo.md) — source
  refs, history preservation, security and license audit, and migration checks.
- [Autonomous task DAGs](task-dag.md) — automatic dispatch, verification,
  dependency wakeup, retries, budgets, controls, and durable scheduling.
- [Measured experiments](experiments.md) — isolated approaches, automatic
  evidence providers, metric provenance, deterministic ranking, and selection.
- [Persistent kernels](kernels.md) — Python, JavaScript, shell, and SQL state,
  governed Glass bindings, attribution, bounds, cancellation, and security.
- [Resident debugger](debugger.md) — stdio and loopback TCP DAP, reverse
  requests, owned debuggee PTYs, session inspection, and adapter certification.
- [Development Graph and replay](development-graph.md) — typed observable
  resources, explicit evidence links, causal paths, replay bounds, and privacy.
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
- [Full-suite reliability](full-suite-reliability.md) — concurrent subsystem
  stress, queue bounds, dropped-event evidence, and the live-browser gate.
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
- [Coding harness architecture](harness-architecture.md) — snapshot workers,
  bounded queues, governed jobs, event cursors, responsive rendering, and
  shutdown behavior.
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
- [Browser core crate](architecture/browser-core.md) — engine ownership, the one-way crate boundary, feature gating, and integration surface.
- [Native browser engine](architecture/native-engine.md) — the Glass-owned
  engine kernel, browser-complete expansion, runtime limits, and promotion
  gates for issue #40. Its versioned contract is the [Glass Core Web
  Profile](plan/native-engine-browser-profile.md).
- [Native browser journey corpus](architecture/native-journey-corpus.md) —
  versioned local and public journeys, performance evidence, evidence
  boundaries, and capability coverage.
- [Alternative browser runtimes](browser-host-rfc.md#current-browser-runtime-mapping) —
  experimental Firefox BiDi and Safari WebDriver support, plus the native
  browser feasibility boundary.
- [Automation contracts](architecture/automation.md) — targeting, waiting,
  topology, input, safety, evidence, visual capture, and resource rules.
- [Experience Layer](architecture/experience.md) — shared user-facing result,
  reference, ownership, and recording contracts.
- [Terminal UI](architecture/tui.md), [Development TUI](architecture/development-tui.md),
  and [remote cockpit](architecture/mobile-cockpit.md) — layouts, interaction,
  workers, phone states, live frames, and SDK orchestration.
- [Canonical Browser Workspace](architecture/browser-workspace.md) and
  [Glass Product Workspace](architecture/product-workspace.md) — the shared
  browser/App controller, direct Agent/Code/App UX, focus, desktop, and phone
  interaction contracts for the current source line.
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

## Credits and project references

- [Project inspiration and attribution](credits-and-inspiration.md) — product
  research references for coding assistants and related systems, distinct from runtime
  dependencies and third-party license notices.
- [Glass Core third-party notices](../products/glass-core/THIRD_PARTY.md) —
  applicable adapted-code and bundled web-asset notices.

## Maintainer and release guides

- [Maintainer handbook](maintainers/README.md) — the versioned documentation,
  ownership, validation, review, and release-process wiki.
- [Documentation style](documentation-style.md) — terminology, procedure, and
  status conventions.
- [Contributing](../CONTRIBUTING.md) — source workflow, checks, security, and
  review expectations.
- [Agent working instructions](../AGENTS.md) — repository structure, build and
  test commands, coding style, and documentation expectations for agents.
- [Release checklist](release-checklist.md) — repeatable two-package release.
- [Release evidence](release-evidence.md) — package, source release, platform,
  client, and validation evidence.
- [Tag signing](tag-signing.md) — current `unknown_key` evidence, maintainer
  identity repair, and the GitHub-verification release gate.
- [Glass 0.3.14 release notes](releases/0.3.14.md) — current dated release
  source record; exact publication evidence is maintained separately.
- [Glass 0.3.13 release notes](releases/0.3.13.md) — previous published
  release record.
- [Glass 0.3.12 release notes](releases/0.3.12.md) — historical immutable
  tagged-release record.
- [Glass 0.3.11 release notes](releases/0.3.11.md) — previous published
  release record.
- [Glass 0.3.10 release notes](releases/0.3.10.md) — immutable failed
  exact-tag candidate record.
- [Glass 0.3.9 release notes](releases/0.3.9.md) — previous complete issue #36
  release record.
- [Glass 0.3.8 release notes](releases/0.3.8.md) — failed exact-tag candidate
  retained for audit evidence.
- [Glass 0.3.7 release notes](releases/0.3.7.md) — failed exact-tag candidate
  retained for audit evidence.
- [Glass 0.3.6 release notes](releases/0.3.6.md) — failed exact-tag candidate
  retained for audit history; it was not published to crates.io.
- [Glass 0.3.5 release notes](releases/0.3.5.md) — previous release record.
- [Changelog](../CHANGELOG.md) — current `Unreleased` changes followed by the
  0.3.14 release entries and immutable historical release entries.
- [Migrate from 0.3.13 to 0.3.14](migration/0.3.14.md) — current upgrade
  contract for package ownership, TUI, editor, crew, Git, and
  browser changes.
- [Migrate from 0.3.12 to 0.3.13](migration/0.3.13.md) — previous published
  release migration.
- [Migrate from 0.3.11 to 0.3.12](migration/0.3.12.md) — historical upgrade
  contract for the tagged release.
- [Migrate from 0.3.9 to 0.3.11](migration/0.3.11.md) — previous published
  release migration.
- [Migrate 0.3.9 to 0.3.10](migration/0.3.10.md) — historical failed
  exact-tag candidate guidance.
- [Migrate 0.3.5 to 0.3.9](migration/0.3.9.md) — previous BrowserWorkspace,
  Agent, daemon, TUI, security, and exact-tag portability repairs.
- [Migrate 0.3.5 to 0.3.8](migration/0.3.8.md) — historical failed candidate.
- [Migrate 0.3.5 to 0.3.7](migration/0.3.7.md) — historical failed candidate.
- [Migrate 0.3.5 to 0.3.6](migration/0.3.6.md) — historical candidate guide.
- [Migrate 0.3.4 to 0.3.5](migration/0.3.5.md) — previous package, trust,
  runtime, and tooling transition.
- [30-second demo](demo.md) — short operator demonstration.
- [Delivery plans](plan/README.md) — current-source navigation, proposed work,
  and historical implementation records; these are not current user
  instructions.

## Historical, generated, and evidence records

- [Delivery plan archive](plan/README.md) — distinguishes active plans from
  completed and historical implementation evidence; follow current guides for
  present behavior.
- [Glass Core implementation plan](../products/glass-core/docs/plan/README.md)
  and [research index](../products/glass-core/research/README.md) — Core-owned
  proposal, implementation, and dated research records.
- [Migration records](migration/0.3.14.md) — current upgrade contract and
  links to previous versioned migration records. These records document
  historical changes; they do not describe supported compatibility routes.
- [Legacy resident runtime migration archive](migration/pi-sdk-runtime.md) —
  historical implementation record only; Glass Dev has no Pi runtime or
  compatibility selector.
- [Retired workspace trust controls](migration/workspace-trust.md) — historical
  account of the former project gate; follow current guides for present
  workspace behavior.
- [Release notes](releases/0.3.14.md) and
  [release evidence](release-evidence.md) — source records and publication
  evidence with version boundaries intact.
- [Changelog](../CHANGELOG.md) — unreleased changes and historical release
  entries.
- Generated and fixture Markdown is classified as `generated` in the catalog;
  those files are consumed by tools, tests, or product builds, not user guides.
- [Complete per-file and section inventory](documentation-catalog.json) — all
  repository Markdown, including package docs, research, plans, reviews,
  generated records, and historical evidence.

The generated catalog is intentionally compact structured data rather than a
second prose copy of every page. Each catalog entry links to its canonical
Markdown path and lists the section titles and anchor fragments detected from
that file. The existing CLI, MCP, examples, Rust API, release-truth, and
documentation-depth checks remain the authorities for their code-derived and
substantive inventories.

Plan and research files retain the status and evidence written in each record;
their presence does not mean a proposal is implemented. If a historical page
names a successor, follow that page's explicit link. The index does not infer
one-to-one replacements for records that have none.
