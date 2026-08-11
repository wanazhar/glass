# Public parity and daemon event cursors

Status: Complete locally

## Contract

Keep `DevelopmentToolRouter` authoritative for TUI, CLI, MCP, glassd, Pi,
kernels, and external coding agents. Provide stable task and safe trust
inspection APIs, and let fresh daemon clients resume bounded workspace events
without independently polling every service.

## Implementation

- Added `glass.task.inspect` and `glass.task.verify` as stable external-agent
  names over the existing task snapshot and attributed verification-evidence
  semantics. Existing `get` and `evidence` names remain compatible.
- Retained `glass.workspace.trust.status` and
  `glass.workspace.trust.inspect` as read-only APIs. No external trust mutation
  tool exists.
- Added `workspace.events` to the authenticated native daemon protocol with an
  exclusive sequence cursor, bounded batches, a 512-envelope workspace ring,
  explicit loss metadata, and value-free event envelopes.
- Classified router calls into workspace, agent, task, process, browser, LSP,
  debugger, test, Git, and experiment event kinds without copying outputs,
  prompts, secrets, or private reasoning.

## Evidence

Daemon tests prove a fresh client handle resumes after its prior cursor and
that ring overflow reports both dropped count and the oldest recoverable
sequence. Full library and development-runtime integration suites pass. The
measured full-product MCP catalog is 292 tools and 142,478 UTF-8 schema bytes,
below its 160 KiB review ceiling.
