# Glass v0.3.4 issue #34 delivery analysis

Status: Active direct local development
Status: Historical record; superseded by the current 0.3.14 source/release evidence.

Issue [#34](https://github.com/wanazhar/glass/issues/34) is authoritative for
this release. A capability counts only when a real resident subsystem owns its
state, at least one user or agent surface consumes it, mutations are governed
and attributed, and executable evidence covers success and failure. Types,
descriptors, screenshots, or documentation without an integrated consumer do
not satisfy a gate.

## Locked architecture

```text
                              glass-dev
                                  │
              ┌───────────────────┴───────────────────┐
              │                                       │
     DevelopmentWorkspace                         Glass Agent
              │                                  Pi runtime only
              └───────────────────┬───────────────────┘
                                  │
       files · editor · PTY · Git · LSP · DAP · tests · kernels
                                  │
                browser · workflows · Web IR · memory
                                  │
                 graph · timeline · experiments · replay
                                  │
                    glassd local durable ownership
                                  │
                         TUI · CLI · MCP clients

glass-dev ──depends on──> glass-browser
glass-browser never depends on glass-dev
```

Pi is the sole embedded agent runtime. External agents remain direct clients
of Glass CLI, MCP, and daemon contracts. There is no generic harness adapter
and no embedded OMP runtime.

## 0.3.3 baseline inventory

| Area | Existing evidence | 0.3.4 gap |
|---|---|---|
| Product ownership | `glass-dev` installs both commands | development implementation and full TUI still live in `glass-browser`; the package is a thin launcher |
| TUI | responsive desktop/compact/mobile shell and browser recovery | `tui/app.rs` is 9,764 lines and owns most development state and rendering |
| Agent | bounded request/event model, Pi RPC process, approvals, file/process tools | direct SDK-grade session registry, resume/fork/compact state, scheduler, and real browser/workflow tools |
| Files/editor | confined atomic writes, buffers, conflicts, undo/redo, syntax/folds | complete editor tool surface, LSP UI integration, Git gutters, navigation and conflict surfaces |
| Processes | resident PTYs, process-tree ownership, bounded output | complete agent operations, ports/health events, durable ownership |
| LSP | persistent client with diagnostics, hover, definition, references, symbols, formatting, rename | declaration/implementation, workspace symbols, completion, signature help, code actions, range formatting, semantic tokens, shared manager |
| Debugging | semantic breakpoint types only | real DAP transport, lifecycle, adapters, state, governed tools, TUI |
| Git | diff/status proxy and experiment worktree helpers | native status/index/branch/commit/blame/stash/conflict/worktree service |
| Tests | configured verification command | discovery, structured runs/results, affected/watch/cancel, workflow-as-test |
| Kernels | bounded one-shot commands | named persistent Python/JavaScript/shell/SQL sessions with cancel/reset/state |
| Durability | browser-focused daemon and reconnect capsule | daemon-owned complete development workspace surviving TUI disconnect |
| Experiments | worktree creation and scalar evidence comparison | agents, processes, browsers, tests, workflows and empirical selection per experiment |
| Graph/replay | source/entity links and bounded timeline windows | shared typed nodes/edges across every subsystem, invalidation and path explanations |
| Customization | project commands and experimental extension contracts | skills, governed hooks/custom tools, command-palette registration |

## Delivery checkpoints

### 1. Ownership and shell foundation

- make `glass-dev` a library and the owner of development contracts;
- leave browser-neutral contracts and standalone browser surfaces in
  `glass-browser`;
- remove the `development-runtime` feature from the browser package boundary;
- split full development dispatch/TUI controllers from browser-only dispatch;
- decompose new subsystem state and rendering outside the monolithic app.

Evidence: dependency metadata, minimal-feature browser build, both installed
commands, ownership tests, and source-size/module checks.

### 2. Glass Agent and resident tool router

- implement structured Pi session create/resume/list/select/fork/compact,
  model/thinking, steer/follow-up/abort, streaming and lifecycle state;
- bind tools to one live `DevelopmentWorkspace` service registry;
- make browser and workflow tools execute authoritative operations;
- add independent scheduled agent state, dependency queues, cancellation,
  worktree assignment, collaboration events, budgets and evidence.

Evidence: multiple concurrent independent sessions, live tool state, governed
mutations, stale revision rejection, cancellation, and TUI inspection.

### 3. IDE runtime services

- complete one shared persistent LSP manager;
- implement real framed DAP clients and adapter lifecycle;
- add native Git and structured test services;
- add bounded persistent execution kernels;
- expose every service through governed agent tools and typed TUI commands.

Evidence: real installed adapters/servers where available plus deterministic
fixture peers for protocol success, malformed input, timeout and cancellation.

### 4. Durable runtime and intelligence

- make the local daemon own workspace services independently from TUI clients;
- add local scoped authentication, stale-client rejection, quotas, recovery and
  clean shutdown;
- expand experiments to isolated agent/worktree/process/browser/test evidence;
- unify graph and replay events across source, runtime, browser and agents.

Evidence: disconnect/reconnect retains identity and live resources; experiment
comparison and causal path queries are derived from recorded evidence.

### 5. Product surfaces

- complete the native editor and agent-native desktop/compact/mobile views;
- expose debugger, Git, tests, agents, experiments, graph and replay through
  command palette and printable-key routes;
- add governed skills, hooks, project commands and custom tools;
- preserve connection-aware presentation and browser recovery behavior.

Evidence: reducer tests, terminal snapshots, mobile reachability, bounded event
queues, responsive streaming, and full interaction scenarios.

### 6. Release convergence

- synchronize every package/client/doc/check to 0.3.4;
- run full workspace format, lint, tests, rustdoc, package and install gates;
- record browser, Pi package, LSP, DAP, durability and integrated scenario
  evidence;
- audit every issue checkbox and forbidden outcome before release readiness.

## Local commit contract

Each meaningful checkpoint ends with a clean worktree and a focused
conventional commit. Expected families are:

1. `refactor(dev): move development ownership into glass-dev`
2. `refactor(tui): decompose development workspace controllers`
3. `feat(agent): integrate native Pi sessions`
4. `feat(agent): add governed multi-agent scheduler`
5. `feat(lsp): complete shared language services`
6. `feat(debug): add resident DAP runtime`
7. `feat(dev): add Git test and kernel services`
8. `feat(daemon): preserve durable development workspaces`
9. `feat(experiments): compare isolated runtime evidence`
10. `feat(tui): expose the agent-native development suite`
11. `chore(release): prepare Glass 0.3.4`

Fixes discovered within a checkpoint are included before that checkpoint is
committed. Later review findings receive their own focused fix commit.

## Gate evidence map

| Issue gate | Primary implementation proof |
|---:|---|
| 1. Pi integration | session registry, structured events and native controls |
| 2. Real agent tools | resident service router and authority/revision tests |
| 3. LSP | shared service integration and real/fixture server tests |
| 4. DAP | adapter transport/lifecycle and debugger scenario tests |
| 5. Multi-agent | concurrent sessions, dependency/cancel and TUI evidence |
| 6. Durable workspace | client disconnect/reconnect survival tests |
| 7. Git/tests/experiments | native service and isolated comparison scenarios |
| 8. Graph/replay | cross-subsystem timeline and causal path tests |
| 9. Product boundary | one-way metadata, package and install inspection |
| 10. TUI architecture | decomposed controllers/renderers and layout snapshots |
| 11. Performance/reliability | bounded queues/logs, responsiveness and cleanup |
| 12. Release evidence | workspace CI, browser/tool smoke and packaged installs |

## Forbidden-outcome audit

The final review must prove that development ownership no longer accumulates
under `glass-browser`, new subsystems do not enlarge one monolithic TUI file,
DAP and multi-agent execution are real rather than metadata, browser/workflow
tools execute against live authority, mutations are attributable, durability
preserves useful resources, experiments have isolated evidence, mobile remains
purpose-designed, external agents do not nest through Pi, and failures cannot
be reported as healthy state.

## Release boundary

Local implementation and commits are authorized. Pushes, tags, issue changes,
crates.io publication, and GitHub Releases require explicit remote-mutation
authorization after all local gates pass.
