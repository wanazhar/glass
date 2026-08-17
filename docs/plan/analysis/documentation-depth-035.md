# Glass 0.3.3 documentation depth audit

Status: Complete and verified locally

## Problem

The prior documentation gate proves inventory mentions and local-link
integrity. It does not prove that a guide teaches a complete workflow. A
command name can appear once without documenting prerequisites, inputs,
outputs, limits, failure states, recovery, persistence, or security. The root
README also presents the browser runtime before the complete `glass-dev`
product even though the repository now ships a development workspace, local
agent harness, remote phone cockpit, browser control plane, MCP server, and
Rust SDK.

The current CLI family table also contains stale nested entries. A validator
must compare the documented nested command inventory with live Clap output,
not only require that live top-level names occur somewhere in prose.

## Documentation contract

Every current user guide must answer the applicable questions below.

| Question | Required content |
|---|---|
| What is this for? | audience, outcome, ownership boundary, and non-goals |
| How do I start? | prerequisites, exact command, expected result, and next action |
| What state changes? | process, browser, project, profile, file, and persistence ownership |
| What are the limits? | bounds, defaults, unsupported cases, and privacy behavior |
| How does it fail? | typed/classified failure, whether an effect may have happened, and safe recovery |
| How do interfaces relate? | CLI, TUI, MCP, SDK, and client mapping without claiming false parity |
| How do I verify it? | status/doctor/inspection command or deterministic validation step |

Reference catalogs may use dense tables instead of tutorials. Architecture
documents must specify types, state ownership, lifecycle, errors, extension
points, and tests. Historical plan and release evidence must retain the truth
of the version they recorded and are not rewritten as current user guides.

## Complete public-surface disposition

| Surface | Files | Disposition in this pass |
|---|---|---|
| Repository entry | `README.md`, `docs/INDEX.md`, `docs/getting-started.md`, `docs/features.md` | Rewrite the product hierarchy, audience routes, quick starts, operational model, and limitations. |
| Installation and governance | `docs/installation.md`, `docs/customization-governance.md`, `SECURITY.md`, `CONTRIBUTING.md`, `docs/documentation-style.md`, `docs/ownership.md` | Preserve verified install/uninstall detail; deepen customization authority/evidence, lifecycle, contribution, trust, and documentation acceptance where missing. |
| Product packages | `crates/glass-dev/README.md`, `crates/glass-browser/README.md` | Keep self-contained crates.io pages; make the complete-versus-core boundary unmistakable. |
| Development workspace | `docs/development-runtime.md`, `docs/workspace-trust.md`, `docs/pi-sdk-runtime.md`, `docs/task-dag.md`, `docs/experiments.md`, `docs/debugger.md`, `docs/kernels.md`, `docs/development-graph.md`, `docs/full-suite-reliability.md`, `docs/architecture/development-tui.md`, `docs/architecture/mobile-cockpit.md`, `docs/architecture/tui.md`, `docs/harness-architecture.md` | Expand trust, Pi, task, experiment, debugger, kernel, typed graph/replay, bounded concurrent load, files/editor/process/LSP/timeline/agent lifecycles, views, errors, and recovery. |
| Remote use | `docs/mobile-remote.md`, `docs/design/v0.3.3/README.md` | Preserve the detailed 0.3.3 workflow; add troubleshooting and capability-decision guidance. |
| Browser use | `docs/actions.md`, `docs/action-contract.md`, `docs/semantic-observation.md`, `docs/profile-ergonomics.md`, `docs/policy.md`, `docs/bot-protection.md`, `docs/detection-surface.md` | Add complete observe-act-verify procedures, state/persistence boundaries, failure decisions, and safe examples. |
| Semantic automation | `docs/semantic-execution.md`, `docs/intent-resolution.md`, `docs/workflows.md`, `docs/workflow-authoring.md`, `docs/knowledge.md` | Preserve contract detail; improve operator routing and end-to-end examples where shallow. |
| Command and protocol references | `docs/cli.md`, `docs/cli/experience.md`, `docs/mcp.md`, `docs/mcp-tools.md`, `docs/protocol.md`, `docs/schema-compatibility.md`, `docs/mcp-schema-budget.md` | Correct the live command tree, document family inputs/outputs/failures, and retain exact-schema discovery as authority. |
| SDK and clients | `docs/rust-sdk.md`, `docs/examples.md`, `clients/python/README.md`, `clients/typescript/README.md` | Add lifecycle/error/concurrency guidance and realistic complete-client flows without presenting repository clients as published packages. |
| Daemon, extensions, and backends | `docs/daemon.md`, `docs/extensions.md`, `docs/experimental-capabilities.md`, `docs/browser-host-rfc.md`, `docs/positioning.md` | Deepen ownership, shutdown, recovery, sandbox, registration, capability, and selection behavior. |
| Reliability and operations | `docs/reliability.md`, `docs/reliability-metrics.md`, `docs/reliability-real-site.md`, `docs/site-smoke.md`, `docs/production-canary.md`, `docs/category-metric.md`, `benchmarks/README.md` | Keep evidence boundaries; expand the short metric/canary runbooks and result interpretation. |
| Platform and release truth | `docs/feature-parity.md`, `docs/ci-platform-certification.md`, `docs/local-platform.md`, `docs/release-checklist.md`, `docs/release-evidence.md`, `docs/releases/0.3.5.md`, `docs/releases/0.3.6.md`, `docs/releases/0.3.7.md`, `docs/releases/0.3.8.md`, `docs/releases/0.3.9.md`, `docs/releases/0.3.10.md`, `docs/tag-signing.md` | Correct current-versus-historical claims, require substantive exact-tag notes and truthful signature verification; do not turn source checks into native certification. |
| Architecture | `docs/architecture/README.md`, `automation.md`, `browser.md`, `browser-connection.md`, `connection-presentation.md`, `experience.md`, `semantic-core-hardening.md`, `semantic-resource-budgets.md` | Retain technical contracts and fill only missing error/lifecycle/navigation context. |
| Evidence and migration | `docs/demo.md`, `docs/migration/issue-31.md`, `docs/migration/0.3.5.md`, `docs/migration/0.3.6.md`, `docs/migration/0.3.7.md`, `docs/migration/0.3.8.md`, `docs/migration/0.3.9.md`, `docs/migration/0.3.10.md`, `docs/design/v0.3.3/README.md` | Keep intentionally bounded evidence, document package/trust/runtime transitions, route readers to current guides, and label historical scope. |
| Canonical product workspace | `docs/architecture/browser-workspace.md`, `docs/architecture/product-workspace.md` | Keep browser state/action parity, semantic-first presentation, Agent/Code/App information architecture, phone reduction, and input ownership in one current product contract. |

## Acceptance gates

1. The root README introduces `glass-dev` as the complete product and
   `glass-browser` as the focused control-plane/library package.
2. A new user can complete browser-free development, interactive TUI, browser
   verification, remote phone, MCP, and Rust entry flows from role-based links.
3. The CLI reference matches the exact top-level and nested Clap inventories;
   hidden/internal commands are not presented as public.
4. Core guides meet checked content contracts for purpose, prerequisites,
   workflow, state, limits, failures, recovery, security, and related guides.
5. Every public Markdown path is accounted for above; historical delivery
   records remain historical.
6. Package README links survive Cargo packaging. Rustdoc and doctests remain
   warning-free and compilable.
7. Release, version, target, package, and publication claims remain internally
   consistent.

## Result

The root entry point now presents the complete `glass-dev` product before the
focused browser package. Core user guides carry purpose, lifecycle, limits,
failures, recovery, security, and interface relationships. The audit corrected
stale nested CLI entries, browser-only product framing, incorrect phone-view
names, cross-process stateful examples, headed-default text, Windows evidence
wording, and obsolete MCP schema measurements.

The coverage gate now exact-compares top-level and nested Clap inventories,
measures the negotiated MCP tool count and serialized schema against the live
binary, checks the pinned client fixture, and validates repository-local links.
The depth gate accounts for every current guide and enforces 16 substantive
contracts plus known-stale-text rejection.
