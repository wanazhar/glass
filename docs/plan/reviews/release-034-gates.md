# Glass v0.3.4 issue #34 gate review

Status: released on 2026-08-10

This review maps the twelve blocking categories in issue #34 to executable
source evidence. A checked item means the implementation and local proof exist;
it does not claim publication or untested native-platform certification.

## 1. Pi integration

- [x] `AgentRegistry` owns independent persistent Pi processes; no generic
      harness adapter or embedded OMP runtime exists.
- [x] Governed tools expose prompt, steer, follow-up, abort, compact,
      new/clone/fork/switch sessions, model, thinking, history, and statistics.
- [x] Structured lifecycle and evidence render in the TUI; real RPC tests cover
      multiple Pi sessions.

## 2. Real resident tools

- [x] Files, editor, PTYs, browser actions, semantics, workflows, and memory use
      one `DevelopmentWorkspace` registry.
- [x] The router checks actor, call ID, mutation authority, confirmation,
      workspace/project revision, and browser revision.
- [x] Pi, TUI, CLI, MCP, and daemon clients consume the same router.

## 3. LSP

- [x] One persistent manager owns lifecycle, diagnostics, completion,
      navigation, symbols, code actions, formatting, semantic tokens, rename,
      and bounded raw calls.
- [x] Real rust-analyzer evidence serves two attributed actors through one
      server.

## 4. DAP

- [x] Framed adapters support launch/attach/configuration, restart,
      breakpoints/exceptions, steps, threads, stacks, scopes, variables,
      evaluation, events, disconnect, and termination.
- [x] Live debugpy proves breakpoint-to-continue state. No second adapter is
      installed in the validation environment; fixtures cover protocol errors.
- [x] Pi and TUI operations use governed debugger tools.

## 5. Multi-agent

- [x] Independent Pi sessions run concurrently with dependencies, background
      state, cancellation, budgets, worktrees, and bounded evidence.
- [x] Agent TUI surfaces expose each worker and its evidence.

## 6. Durable workspace

- [x] Authenticated mode-0600 Unix IPC owns complete workspaces independently
      from clients, with quotas and stale-context rejection.
- [x] Fresh clients recover the same Pi agent, PTY process, SQL kernel, and real
      Chromium identity after disconnect.

## 7. Git, tests, and experiments

- [x] Native Git status/diff/stage/branch/commit/blame/stash/conflict/worktree
      operations execute with bounded output.
- [x] Tests provide discovery, configured suites, structured results,
      run-affected, watch, timeout, and cancellation.
- [x] Experiments own isolated worktrees/workspaces, agents, processes, ports,
      tests, and evidence-derived ranking and selection.

## 8. Graph and replay

- [x] Typed revisioned nodes/edges provide bounded updates, invalidation,
      queries, causal paths, and explanations.
- [x] Observable actions feed one replay/diff timeline without hidden model
      reasoning.

## 9. Product boundary

- [x] `glass-dev` owns the entry point, resident registry, development
      services, external MCP backend, durable daemon, and decomposed full TUI.
- [x] `glass-browser --no-default-features` remains independently buildable;
      dependency metadata is strictly `glass-dev -> glass-browser`.
- [x] The `glass-dev` package declares both user-facing binaries.

## 10. TUI architecture

- [x] Development state, commands, rendering, and shell control are outside the
      legacy browser TUI monolith.
- [x] Desktop, compact, and purpose-built phone layouts have snapshot coverage;
      printable keys and palette routes reach every major surface.

## 11. Performance and reliability

- [x] Pi, browser, DAP, process, and daemon work execute outside the TUI event
      loop. Queues, logs, responses, events, schemas, and outputs are bounded.
- [x] Browser presentation policy is unchanged. Real tests prove PTY and
      Chromium cleanup, closed CDP ports, and no disposable-profile orphans.

## 12. Release evidence

- [x] Version metadata and substantive release notes are synchronized at 0.3.4.
- [x] Host-native Pi, rust-analyzer, debugpy, Chromium, daemon, experiment, MCP,
      and PTY TUI evidence is recorded with target limits.
- [x] Final workspace CI-equivalent, rustdoc, packages, and isolated installs
      must pass before the candidate is locally ready.
- [x] The signed tag, exact-tag CI and fuzz, ordered crates.io publication,
      clean registry installs, and source-only GitHub Release are verified in
      `docs/release-evidence.md`.

## Forbidden-outcome audit

No generic embedded adapter or OMP runtime was added. Pi tools do not bypass
Glass state. Browser navigation/actions are live. DAP and multi-agent execution
have real processes and independent state. New subsystem code lives in focused
`glass-dev` modules rather than the browser TUI monolith. Experiments own real
worktrees and runtime evidence. `glassd` preserves useful resources. Mobile is
purpose-built. External agents call CLI/MCP/daemon directly. Failures remain
typed and cannot be serialized as healthy state.
