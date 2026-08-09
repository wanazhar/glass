# Positioning: where Glass fits

Glass is a local-first terminal development environment with an integrated
browser intelligence runtime. `glass-dev` combines project inspection,
editing, PTYs, diagnostics, agents, replay, graphs, remote cockpit views, and
browser verification. `glass-browser` packages the focused browser CLI and
Rust library for consumers that do not need the development workspace.

Glass is not a hosted IDE, general operating-system sandbox, autonomous
workflow generator, cross-browser test framework, or remote browser fleet.

## Product boundary

| Need | Glass surface | Deliberate boundary |
|---|---|---|
| Work on a project over a local or SSH terminal | `glass` TUI and Development Runtime | bounded project root; commands retain the user's OS authority |
| Inspect and control Chrome/Chromium | browser workspace, CLI, MCP, Rust SDK | local browser; structured-first state; explicit privileged capture |
| Connect an agent to project/browser tools | local harness and optional Pi RPC | Glass validates tool schemas and mutation authority; it does not trust model output |
| Automate from another local process | MCP plus repository TypeScript/Python clients | local stdio/socket transport; clients do not contain a browser runtime |
| Resume or observe from an iPhone | mobile TUI, terminal graphics, SSH-forwarded Remote View | loopback-only services; no public hosted relay |
| Build repeatable browser tasks | Task Protocol, workflows, checkpoints, replay | callers author intent and policy; Glass does not invent an unbounded workflow |

## Use the full `glass-dev` product when

- the source tree, editor, PTYs, diagnostics, diff, graph, and verification
  evidence should share one bounded project session;
- an SSH or narrow-terminal workflow needs a responsive six-view cockpit;
- local agents should use schema-validated project and browser tools;
- browser state should be connected to source/runtime evidence; or
- one MCP lifecycle should own persistent project processes and browser state.

Use the focused `glass-browser` package when an application needs only the
browser command, MCP surface, or reusable Rust crate.

## Choose another tool when

- you need a hosted collaborative IDE, browser fleet, or public remote desktop;
- you need Firefox, WebKit, Safari, or a fully certified Windows browser release;
- you need a complete QA framework with assertions, fixtures, code generation,
  cross-browser matrices, and trace viewers;
- you require an OS security sandbox for untrusted repository commands;
- you want an agent to invent and execute unrestricted workflows without
  explicit policy and authority; or
- you need stealth, fingerprint modification, CAPTCHA solving, or bot evasion.

## What distinguishes Glass

- Project state, browser state, events, actors, processes, and verification
  evidence are coordinated locally rather than sent to a hosted control plane.
- The Development Runtime bounds project roots, resident sessions, event
  history, file trees, output, and graph evidence.
- Browser observation is structured-first. Screenshots, DOM, form values, PDFs,
  evaluation, uploads, downloads, and raw CDP are explicit policy-sensitive
  operations.
- Locators resolve uniquely, and revision-guarded actions reject stale page
  state before input.
- Chrome ownership is explicit: Glass does not silently adopt an occupied CDP
  endpoint, and attached browsers are never closed as owned sessions.
- CLI, TUI, MCP, Rust, TypeScript, and Python surfaces preserve the same typed
  failure, capability, revision, and lifecycle contracts.

## Decision path

Start with the [getting-started guide](getting-started.md). Continue to the
[Development Runtime](development-runtime.md) for project work, the
[browser action contract](actions.md) for automation, or
[mobile and remote development](mobile-remote.md) for SSH/iPhone use. Exact
package and platform status is recorded in [release evidence](release-evidence.md)
and [target certification](ci-platform-certification.md); it should not be
inferred from an implementation matrix alone.
