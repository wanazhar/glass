# Complete MCP tool catalog

This catalog names every browser tool in the `0.3.4` client-conformance inventory.
The full `glass` command also merges its live `glass.*` Development Workspace
catalog at runtime; these tools are governed by actor, authority, confirmation,
workspace-generation, and project-revision metadata.
The server's `tools/list` response is authoritative for exact JSON Schema,
required fields, and additive descriptions in the installed version. Use this
catalog for discovery and [MCP integration](mcp.md) for framing, negotiation,
cancellation, concurrency, policy, errors, and session security.

Tool calls use MCP JSON-RPC framing. Camel-case input fields map to canonical
Glass request payloads; MCP-only fields such as `responseMode` and
`includeTrace` do not become part of browser or task contracts.

## Resident development tool inventory

The full `glass` product adds the following namespaced tools to the
`glass-browser` catalog. Tools in one family share the same resident service,
actor attribution, bounded-result rules, and revision/confirmation policy.

| Family | Exact tools | Scope |
|---|---|---|
| `glass.agent` | `glass.agent.abort`, `glass.agent.clone-session`, `glass.agent.compact`, `glass.agent.entries`, `glass.agent.follow-up`, `glass.agent.fork`, `glass.agent.list`, `glass.agent.messages`, `glass.agent.model`, `glass.agent.name`, `glass.agent.new-session`, `glass.agent.prompt`, `glass.agent.spawn`, `glass.agent.stats`, `glass.agent.steer`, `glass.agent.switch-session`, `glass.agent.thinking` | Persistent Pi sessions, steering, lifecycle, model state, and evidence. |
| `glass.browser` | `glass.browser.act`, `glass.browser.attach`, `glass.browser.diff`, `glass.browser.navigate`, `glass.browser.observe`, `glass.browser.reconnect`, `glass.browser.screenshot`, `glass.browser.semantic`, `glass.browser.snapshot`, `glass.browser.start`, `glass.browser.state`, `glass.browser.stop`, `glass.browser.target.select`, `glass.browser.targets` | Resident Chrome lifecycle, revision-safe actions, observations, targets, and evidence. |
| `glass.capabilities` | `glass.capabilities.inspect` | Effective resident-tool availability and unavailable reasons. |
| `glass.command` | `glass.command.run` | Bounded foreground command execution attributed to the Pi actor. |
| `glass.debug` | `glass.debug.attach`, `glass.debug.breakpoint.remove`, `glass.debug.breakpoint.set`, `glass.debug.configuration_done`, `glass.debug.continue`, `glass.debug.disconnect`, `glass.debug.evaluate`, `glass.debug.events`, `glass.debug.exception.set`, `glass.debug.inspect`, `glass.debug.launch`, `glass.debug.pause`, `glass.debug.processes`, `glass.debug.restart`, `glass.debug.scopes`, `glass.debug.stack`, `glass.debug.start`, `glass.debug.step`, `glass.debug.stop`, `glass.debug.terminate`, `glass.debug.threads`, `glass.debug.variables` | Resident DAP lifecycle, configuration, breakpoints, execution, watches, reverse-request processes, and inspection. |
| `glass.diagnostics` | `glass.diagnostics.run` | Bounded shared language-server diagnostics. |
| `glass.editor` | `glass.editor.buffers`, `glass.editor.diff`, `glass.editor.open`, `glass.editor.replace`, `glass.editor.save`, `glass.editor.selection` | Attributed editor buffers, selections, diffs, replacements, and saves. |
| `glass.eval` | `glass.eval.execute`, `glass.eval.list`, `glass.eval.reset`, `glass.eval.start`, `glass.eval.stop` | Persistent language kernels and bounded execution state. |
| `glass.file` | `glass.file.delete`, `glass.file.edit`, `glass.file.find`, `glass.file.grep`, `glass.file.list`, `glass.file.mkdir`, `glass.file.patch`, `glass.file.read`, `glass.file.rename`, `glass.file.search`, `glass.file.write` | Workspace-confined reads, discovery, search, patches, and mutations. |
| `glass.git` | `glass.git.blame`, `glass.git.branch.create`, `glass.git.branch.switch`, `glass.git.branches`, `glass.git.commit`, `glass.git.conflicts`, `glass.git.diff`, `glass.git.stage`, `glass.git.stash.list`, `glass.git.stash.pop`, `glass.git.stash.push`, `glass.git.status`, `glass.git.unstage`, `glass.git.worktree.create`, `glass.git.worktree.list`, `glass.git.worktree.remove` | Native repository inspection, branches, staging, commits, stashes, conflicts, and worktrees. |
| `glass.graph` | `glass.graph.explain`, `glass.graph.path`, `glass.graph.query` | Revisioned causal graph queries, paths, and explanations. |
| `glass.lsp` | `glass.lsp.code_actions`, `glass.lsp.completion`, `glass.lsp.declaration`, `glass.lsp.definition`, `glass.lsp.diagnostics`, `glass.lsp.document_symbols`, `glass.lsp.events`, `glass.lsp.formatting`, `glass.lsp.hover`, `glass.lsp.implementation`, `glass.lsp.list`, `glass.lsp.range_formatting`, `glass.lsp.raw`, `glass.lsp.references`, `glass.lsp.rename`, `glass.lsp.semantic_tokens`, `glass.lsp.signature_help`, `glass.lsp.start`, `glass.lsp.stop`, `glass.lsp.workspace_symbols` | Shared LSP lifecycle and the complete typed language-operation surface. |
| `glass.memory` | `glass.memory.explain`, `glass.memory.forget`, `glass.memory.retrieve` | Scoped retrieval, explanations, and confirmed forgetting. |
| `glass.process` | `glass.process.health`, `glass.process.input`, `glass.process.list`, `glass.process.logs`, `glass.process.ports`, `glass.process.resize`, `glass.process.restart`, `glass.process.start`, `glass.process.stop` | Resident PTY lifecycle, I/O, health, ports, and bounded logs. |
| `glass.replay` | `glass.replay.diff`, `glass.replay.inspect`, `glass.replay.list` | Bounded causal timeline discovery, inspection, and comparison. |
| `glass.runtime` | `glass.runtime.inspect` | Project, process, actor, diagnostic, and resident-state inspection. |
| `glass.semantic` | `glass.semantic.diff`, `glass.semantic.inspect`, `glass.semantic.links` | Semantic state inspection, diffs, and source/runtime links. |
| `glass.task` | `glass.task.plan` | Value-free task compilation and explanation. |
| `glass.test` | `glass.test.cancel`, `glass.test.discover`, `glass.test.results`, `glass.test.run`, `glass.test.run-affected`, `glass.test.watch` | Test discovery, execution, affected selection, watching, cancellation, and results. |
| `glass.web_ir` | `glass.web_ir.continuity`, `glass.web_ir.diff`, `glass.web_ir.inspect` | Validated Web IR inspection, diffs, and entity continuity. |
| `glass.workflow` | `glass.workflow.cancel`, `glass.workflow.list`, `glass.workflow.pause`, `glass.workflow.record`, `glass.workflow.resume`, `glass.workflow.run`, `glass.workflow.verify` | Durable workflow lifecycle, verification, and recording. |

The checked-in development conformance fixture pins this exact inventory.
Clients must still use `tools/list` for schemas and the negotiated capability
agreement for availability; an inventory entry is not authority to mutate.
## Project and agent runtime

These tools are browser-free. Paths are confined to a canonical project root,
retained content is bounded, and mutations are actor-attributed.

| Tool | Use |
|---|---|
| `project.inspect` | Detect project type, commands, configuration, and runtime state. |
| `project.files` | List bounded files and directories with explicit truncation, ignored-directory, and skipped-symlink metadata. |
| `project.search` | Search files, entities, processes, events, and commands. |
| `project.read` | Read one bounded UTF-8 project file. |
| `project.edit` | Replace and atomically save one file with external-agent provenance. |
| `project.mkdir` | Create one confined directory. |
| `project.rename` | Rename or move one confined path. |
| `project.delete` | Delete one file or empty directory after explicit confirmation. |
| `project.diagnostics` | Request bounded real rust-analyzer diagnostics. |
| `project.run` | Start a configured or explicit command in a bounded PTY. |
| `project.processes` | List resident managed processes. |
| `project.process.stop` | Stop one process. |
| `project.process.output` | Read one bounded process-output tail. |
| `project.diff` | Return code, runtime, semantic, and workflow impact. |
| `project.timeline` | Return the bounded actor-attributed timeline. |
| `project.events` | Read a cursor-bounded event page with gap reporting. |
| `project.session.status` | Inspect canonical-root resident-session ownership. |
| `project.session.detach` | Confirm and clean up one resident session. |
| `project.capsule.save` | Save a non-sensitive reconnect capsule atomically. |
| `project.capsule.show` | Read a reconnect capsule when present. |
| `project.capsule.clear` | Confirm and remove a reconnect capsule. |
| `project.inbox` | Return the bounded phone attention inbox. |
| `project.verification.card` | Summarize code/process/semantic/visual evidence without implicit capture. |
| `project.replay` | Replay a bounded development revision window. |
| `project.graph` | Discover markers or navigate source/runtime links. |
| `project.breakpoint` | Evaluate a semantic breakpoint against before/after entity state. |
| `project.neovim.probe` | Probe Neovim PTY and headless RPC compatibility. |
| `project.experiment.create` | Create an isolated Git worktree experiment and dedicated port. |
| `project.attach` | Attach an external actor with declared authority. |
| `project.link` | Record explicit source/runtime provenance and confidence. |
| `agent.hello` | Negotiate the Glass local harness protocol. |
| `agent.prompt` | Run one bounded deterministic local-harness prompt. |
| `agent.steer` | Send a steering event to the local harness. |

Project mutation through a daemon-bound client requires the current mutation
lease. Read-only inspection does not start Chrome. Pi-specific control remains
behind the Glass harness; these MCP tools do not expose raw Pi RPC.

## Task Protocol and Web IR

| Tool | Use |
|---|---|
| `validateTask` | Strictly validate an authored task without compiling or starting Chrome. |
| `compileTask` | Compile a task against validated Web IR without starting Chrome. |
| `executeTask` | Freshly compile, bind, confirm, execute, and verify a browser-backed task. |
| `inspectWebIr` | Return bounded metadata for validated Web IR. |
| `validateWebIr` | Validate Web IR graph, details, coverage, and bounds. |
| `diffWebIr` | Return bounded changes across compatible revisions. |
| `continuityWebIr` | Classify one entity as unchanged, changed, rebound, removed, or ambiguous. |

Task input values are not returned in plans or receipts. Offline Web IR never
contains live authority. `executeTask` requires current revision and applies
risk, ambiguity, confirmation, capability, lease, binding, and postcondition
rules from [semantic execution](semantic-execution.md).

## Navigation and direct interaction

| Tool | Use |
|---|---|
| `preflightNavigation` | Check URL policy without Chrome, DNS, or confirmation-token consumption. |
| `navigate` | Navigate and return revision-aware page and identity metadata. |
| `preflight` | Resolve a target and clickability without input, focus, scroll, or revision change. |
| `click` | Click one unique locator or revisioned reference. |
| `clickAt` | Policy-gated exact viewport-coordinate click; never retargeted. |
| `clickExpectPopup` | Click and return exactly one causally verified popup. |
| `doubleClick` | Double-click one unique actionable target. |
| `hover` | Move the pointer over one actionable target. |
| `drag` | Drag one unique source to one unique destination. |
| `type` | Insert text, optionally after uniquely resolving/clicking a target. |
| `clear` | Clear one editable target. |
| `check` | Ensure a checkbox/radio is checked. |
| `uncheck` | Ensure a checkbox is unchecked. |
| `select` | Select one exact option value. |
| `fillForm` | Pre-resolve and fill at most 16 fields. |
| `key` | Dispatch one complete key press. |
| `keyDown` | Dispatch key-down only. |
| `keyUp` | Dispatch key-up only. |
| `shortcut` | Dispatch one explicit modifier shortcut. |
| `scroll` | Scroll by bounded CSS-pixel deltas. |

All element locators must resolve uniquely. Mutation tools accept revision
guards and return typed verification/recovery data. Coordinate clicks require
the dedicated policy capability and current geometry.

## Observation, extraction, and intent

| Tool | Use |
|---|---|
| `observe` | Compact or semantic structured observation; deep DOM, pixels, and values are opt-in. |
| `observeBootstrap` | Advisory URL/title/readiness/text/revision evidence without action targets. |
| `observeDelta` | Bounded same-route added/removed/changed control delta. |
| `getText` | Explicit visible text. |
| `getDOM` | Explicit full DOM inspection. |
| `screenshot` | Explicit viewport, clip, element, or full-page PNG/JPEG/WebP evidence. |
| `inspectPage` | Task-oriented page/region/target projection with route and revision. |
| `findTarget` | Resolve one declared semantic intent into candidates without action. |
| `actAndVerify` | Execute one explicit semantic intent with optional postcondition. |
| `extractStructured` | Extract typed bounded fields/records with provenance and continuation. |
| `recoverRun` | Return conservative browser-free recovery for an indeterminate execution. |
| `resolveIntent` | Normalize and resolve bounded current candidates without dispatch. |
| `executeIntent` | Re-observe, re-resolve, and execute only the selected candidate. |
| `reconcileReferences` | Classify prior revisioned references as preserved, relocated, or lost. |

Sensitive structured fields require `read_sensitive_extraction`. Continuation
is bound to revision, route, region, and extraction-contract digest.

## Knowledge and advisory memory

| Tool | Use |
|---|---|
| `observeKnowledge` | Fresh semantic observation plus optional scoped knowledge assessment. |
| `resolveIntentWithKnowledge` | Current candidates with eligible historical fingerprints as secondary evidence. |
| `knowledgeList` | List profile-scoped records. |
| `knowledgeShow` | Inspect one record and bounded provenance. |
| `knowledgeStats` | Return counts and serialized size. |
| `knowledgeInvalidate` | Mark one record stale, contradicted, or quarantined. |
| `knowledgePurge` | Confirm and purge one exact origin scope. |
| `memoryStatus` | Report advisory-memory lifecycle counts. |
| `memoryInspect` | Inspect one advisory record. |
| `memoryExplain` | Explain why memory cannot authorize mutation. |
| `memoryForget` | Remove one advisory record. |
| `memoryExport` | Export the validated advisory snapshot. |
| `memoryPrune` | Remove stale, contradicted, or quarantined records. |
| `memoryReindex` | Reload and validate advisory memory from disk. |

Knowledge and memory never return executable browser references or grant
mutation authority.

## Workflow, verification, checkpoints, and snapshots

| Tool | Use |
|---|---|
| `batch` | Execute at most 32 typed operations after whole-batch policy preflight. |
| `workflow` | Validate and execute a bounded declarative workflow. |
| `verify` | Evaluate URL/title/visibility/text/topology/dialog/download/revision or boolean predicates. |
| `wait` | Wait for one typed condition until an explicit deadline. |
| `exportCheckpoint` | Export a redacted checkpoint of at most 4 KiB. |
| `importCheckpoint` | Validate and restore checkpoint target/frame context. |
| `sessionSnapshot` | Create, list, inspect, diff, or purge redacted local snapshots. |

Snapshot management is browser-free except creation. Checkpoint import does
not assert that a prior mutation succeeded; resume logic must reconcile.

## Targets, frames, dialogs, diagnostics, and downloads

| Tool | Use |
|---|---|
| `listTargets` | List page targets without selecting one. |
| `createTarget` | Create a page target without selecting it. |
| `selectTarget` | Explicitly select the active page target. |
| `closeTarget` | Close one target; closing the active target leaves no implicit selection. |
| `listFrames` | List bounded frames in the active target. |
| `selectFrame` | Explicitly select the active frame. |
| `diagnostics` | Collect bounded redacted console/network and lifecycle metadata. |
| `acceptDialog` | Accept the currently open JavaScript dialog. |
| `dismissDialog` | Dismiss the current JavaScript dialog. |
| `dismissConsent` | Dismiss recognized OneTrust/Cookiebot UX; never anti-bot bypass. |
| `download` | Wait for one download into an authorized existing directory. |
| `upload` | Set 1–16 bounded regular local files; contents are never returned. |
| `clipboardRead` | Read at most 8 KiB of system clipboard text. |
| `clipboardWrite` | Write clipboard text capped at 8 KiB. |

## Storage, PDF, evaluation, and emulation

| Tool | Use |
|---|---|
| `cookies` | Read cookies for the current page under persistent-profile policy. |
| `setCookies` | Set bounded cookies under persistent-profile policy. |
| `clearCookies` | Clear cookies under persistent-profile policy. |
| `localStorage` | Read at most 64 entries with bounded values. |
| `sessionStorage` | Read at most 64 entries with bounded values. |
| `printToPdf` | Return explicit base64 PDF bytes. |
| `evaluate` | Run policy-gated JavaScript and return its JSON value. |
| `setNetworkConditions` | Apply preset or explicit bounded network conditions. |
| `clearNetworkConditions` | Restore normal session network conditions. |
| `setCpuThrottling` | Apply a bounded CPU multiplier. |
| `clearCpuThrottling` | Restore CPU multiplier to 1x. |
| `setUserAgent` | Override declared UA and optional language/platform. |
| `clearUserAgent` | Restore the UA captured before override. |
| `setGeolocation` | Set a bounded geolocation override. |
| `clearGeolocation` | Clear geolocation override. |
| `setTimezone` | Set an IANA timezone ID. |

Storage and evaluated values may be sensitive. MCP error/log paths do not echo
raw selectors, source, typed values, page payloads, or CDP errors.

## Workspaces, surfaces, backends, and replay

| Tool | Use |
|---|---|
| `workspaceStatus` | List persisted workspace identities and lifecycle state. |
| `workspaceInspect` | Inspect one normalized workspace identity. |
| `surfaceInspect` | Validate surfaces and report coverage/provenance. |
| `backendStatus` | Validate a transport-neutral backend profile/capability declaration. |
| `backendTest` | Exercise fail-closed capability dependency and response rules. |
| `replayInspect` | Validate a redacted replay against its exact scenario. |
| `replayDiff` | Compare two bounded replays for one scenario. |
| `replayAttach` | Attach a validated replay reference without browser mutation. |

Backend and surface inspection does not create transport authority. Replay is
evidence, not a command stream.

## Discover exact schemas

After initialization:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

Cache schemas only for the negotiated server version. Unknown fields are
normally rejected. Check the initialize `glassAgreement` before assuming an
experimental or policy-sensitive capability is available.
