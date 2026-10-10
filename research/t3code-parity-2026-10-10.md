# T3 Code parity research

Date: 2026-10-10. T3 Code source revision: `f7664fdf425555e0f85b74d904cfa4eaf57cf7af`.
GC baseline: `a1e6460`. Findings come from documentation and source inspection, not a runtime comparison.

## Product direction

T3 Code coordinates external coding harnesses across clients and machines. GC runs its own native harness. Preserve that fast default; add optional backend adapters only where they bring useful subscription, session, or tooling compatibility. GUI parity should mean equivalent workflows, with terminal-appropriate controls.

## Delivery candidates

Existing means an inspected primitive exists; it does not establish complete UX parity.

| Priority | Workflow | GC foundation | Proposed delivery |
| --- | --- | --- | --- |
| 1 | Workspace and thread hub | Resume/search, sessions, agent board | One searchable picker with project, branch, model, unread activity, running and blocked states; background submission |
| 1 | Durable follow-up queue | FIFO queue and steer slot | Saved prompts with edit, reorder, delete, and promote-to-steer actions; explicit delivery receipts |
| 1 | Unified transcript | Native markdown and tool views | Identical rendering in live and scrolled history, distinct tool/reasoning/error sections, stable scroll anchors |
| 1 | Attention inbox | Notifications and teammate status | One place for failures, completed background work, questions, and pending reviews; jump to exact event |
| 1 | Context composer | Attachments and context inspector | File/range/quote references with previews, source attribution, estimated size, and removable context items |
| 1 | Turn-specific review | Git/hunk review and checkpoints | Separate changes attributable to this turn from whole-branch changes and imported Git changes |
| 2 | Safe rewind | Workspace checkpoints | Coordinated conversation and file rewind, clear keep-files option, shared-directory protection, preview before restore |
| 2 | Worktree lifecycle | Worktree tools and optional agent isolation | New task in new or existing worktree, branch naming, safe reuse, completion cleanup preview |
| 2 | Source-control cockpit | Stage, review, commit | Cohesive branch/commit/PR workflow, CI status and review feedback; publishing remains explicit |
| 2 | Background terminal inspector | Jobs and process controls | Focusable live terminal/output panel, bounded retained history, reconnect, input ownership and exit status |
| 2 | Usage center | Turn/session tokens and budgets | Model/cache breakdown, provider quota when available, known versus estimated costs, per-task and teammate attribution |
| 2 | Settings provenance | Auth/config/policies | Consistent project/environment/default inheritance viewer, reset-to-inherited actions and diagnostics |
| 3 | Optional external harness backends | GC exposes an ACP server | Capability-aware ACP client and dedicated adapters as needed; ACP serving does not provide this capability |
| 3 | Portable handoffs | Compaction and handoff artifacts | Preserve original request, recent work and failure state; retrieve omitted history by reference; disclose missing capabilities |
| 3 | Remote multi-client control | Local Unix-socket serve/attach | Versioned authenticated protocol, replay cursors, bounded catch-up, per-thread subscriptions and explicit control ownership |
| 3 | Managed automations | Idle interval loops | Schedule/run history, missed-run policy, host/project/model selection; signed webhooks as optional module |
| 4 | Optional companion UI | Headless runtime | Browser/mobile client consuming the same event contract, without adding GUI dependencies to the terminal path |

## Performance architecture

Borrow T3's principles rather than its implementation stack:

- Load navigation metadata independently of transcript bodies. Subscribe only to visible or actively controlled threads.
- Page older history; bound initial loading and catch-up by rows and bytes. Large tool bodies and diffs load on demand.
- Seek event cursors through indexes. Never scan all completed conversations just to find active work.
- Persist commands before acknowledging them; distinguish receipt, execution, provider completion, and post-turn finalization.
- Coalesce streaming updates. Cache parsed finalized blocks; streaming should invalidate only the active block.
- Release background subscriptions when panels close. Apply backpressure to slow clients without blocking providers or input.
- Start optional adapters, language services, browser integrations, and remote services lazily.
- Measure the actual release binary and active protocol. Synthetic fixtures must exercise the same rendering and transport paths users run.

T3's numeric budgets are implementation-specific; GC should derive its own regression limits from its current release baseline. Track input latency, redraw latency, startup, history paging, RSS, output volume, reconnect time and provider token overhead separately.

## Suggested first release

Deliver the first six rows as one coherent workbench improvement. Follow with safe review/rewind/worktrees, then remote and external backends. Avoid launching all adapters and automation features before the central thread/event model is stable.

Acceptance should include actual release PTYs at desktop and narrow phone sizes, streaming while scrolling, queue editing during tools, reconnect without duplicate sends, background completion while another thread is focused, and large-history lazy loading. Keep model input unchanged by decorative UI state and fetch historic context only when requested.

## Primary sources

All source links below pin the inspected revision.

- [Repository and license](https://github.com/pingdotgg/t3code/tree/f7664fdf425555e0f85b74d904cfa4eaf57cf7af)
- [Architecture](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/internals/overview.md)
- [Performance regression contracts](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/internals/performance-regressions.md)
- [Composer and queued prompts](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/composer.md)
- [Threads and workspaces](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/thread-sidebar.md)
- [Source control and turn attribution](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/source-control.md)
- [Portable context handoffs](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/portable-handoffs.md)
- [Remote access](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/remote-access.md)
- [Settings and automation](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/project-settings.md)
- [Usage](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/usage.md)
- [Terminal history](https://github.com/pingdotgg/t3code/blob/f7664fdf425555e0f85b74d904cfa4eaf57cf7af/docs/user/terminal.md)

## Additional research: Codex, Pi, and Oh My Pi

Researched 2026-10-10. OMP means `can1357/oh-my-pi`. Pi's current repository is `earendil-works/pi` (the older `badlogic/pi-mono` location redirects). Inspected Pi revision `c5f5b3282d5e4203c085e59837ba17aeaf2829b5` and OMP revision `01e0b5c26e267dff08c305f51d1ab56ec5a2f05d`; official Codex documentation was fetched live. These are research candidates, not implemented features or measured upstream performance guarantees. Some upstream features are optional or experimental.

The original table has 17 candidates. Continue numbering below; improvements to those candidates are called out separately to avoid counting them twice.

| # | Priority | Additional candidate | Inspiration / evidence | GC foundation and proposed increment |
| --- | --- | --- | --- | --- |
| 18 | 1 | Structured review findings | [Codex review][codex-review] | Hunk comments exist. Add findings with severity, location, evidence and disposition, plus navigable review cards and machine-readable export. Separate review from automatic fixes. |
| 19 | 1 | Schema-validated final answers | [Codex automation][codex-exec] | JSON presentation exists. Add optional final-response JSON Schema contracts for headless runs, with validation errors and bounded correction; rendering JSON is not validation. |
| 20 | 1 | Typed teammate deliverables | [OMP task tools][omp-home] | Persistent workers return text. Allow a parent-selected result schema, artifact references and explicit incomplete status; validate results without discarding useful partial work. |
| 21 | 1 | Deferred capability discovery | [Pi extensions][pi-ext] | GC keeps a small tool surface and optional extensions. Add searchable capability metadata and request-scoped activation; avoid sending every extension schema on every turn. |
| 22 | 1 | Cache-aware prompt changes | [Pi extensions][pi-ext] | Skills and request audit exist. Record tool/skill changes and resulting prompt-prefix changes; expose likely cache invalidation without presenting an estimate as a measured cache hit. |
| 23 | 1 | Large output artifact references | [OMP blob architecture][omp-blobs] | Bounded previews exist. Preserve full results in local artifacts, give stable range-readable references, and distinguish UI truncation from model-visible truncation. |
| 24 | 1 | Failure recovery inspector | [OMP retry policy][omp-retry] | GC already has retry, fallback and tool repair. Show failure class, attempt, provider/request identifiers when available, cooldown and safe next action; add a transport reset that preserves local history. Never silently replay completed mutations. |
| 25 | 1 | Reviewable diagnostic bundles | [Pi session reporting][pi-sessions] | Doctor/context diagnostics exist. Export a local bundle with terminal dimensions, rendering state, recent events and redacted provider diagnostics; preview contents and require explicit action to upload. |
| 26 | 2 | Conversation branch explorer | [Pi sessions][pi-sessions] | Fork and resume exist. Add a navigable message tree with branch labels and retained alternatives; distinguish branching conversation from restoring workspace files. |
| 27 | 2 | Inspectable compaction receipts | [Pi compaction][pi-compact] | Compaction/audit exist. Show retained boundaries, original-history references, cumulative read/changed file lists and summarization usage; support user preservation instructions. |
| 28 | 2 | Transactional structural edits | [OMP staged resolution][omp-resolve] | Revision-checked native edits and LSP previews exist. Add optional AST transformations with preview, stale checks, explicit apply/reject and transaction-wide validation. Do not assume multiple file renames are atomic. |
| 29 | 2 | Edit protocol comparison and recovery | [OMP hashline][omp-home] | GC already includes a hashline extension and revision-checked native files. Benchmark both on realistic stale edits, Unicode and repeated lines; improve error-local reread hints before choosing a default. Upstream advertised gains are not GC measurements. |
| 30 | 2 | LSP diagnostics after changes | [OMP LSP][omp-lsp] | Optional code intelligence exists. Debounce diagnostics after accepted edits, show only relevant deltas, and apply multi-file rename/code actions through the same stale-checked preview flow. |
| 31 | 2 | Policy explanation and simulation | [Codex rules][codex-rules] | Permissions, trust and sandbox modules exist. Add an explanation of the matched policy and a dry-run command evaluator with positive/negative examples and compound-shell handling. Policy hints are not OS confinement. |
| 32 | 2 | Extension lifecycle and renderer contracts | [Pi extensions][pi-ext] | Process extensions and native tool cards exist. Specify cancellation, shutdown, nested-call lineage and a shared structured renderer contract for live, history and exports; release resources when disabled. |
| 33 | 2 | Reproducible extension bundles | [Pi packages][pi-packages] | Plugin management exists. Bundle tools, skills, templates and themes with pinned revisions, inspectable permissions, update previews and rollback. No install-time execution by default. |
| 34 | 3 | On-demand reviewer/advisor | [OMP advisor][omp-advisor] | Persistent reviewers can already be orchestrated. Add explicit review cadence, separate usage, concise evidence-linked notes and parent disposition. Default to milestones or user request; continuous review incurs extra tokens. |
| 35 | 3 | Debugger adapter integration | [OMP debugger][omp-home] | Shell debugging remains possible. Add an optional DAP module for breakpoints, stack frames and variables, started only on demand; process attach and evaluation must respect permissions. |
| 36 | 3 | Project memory with provenance | [OMP memory][omp-memory] | Capture-to-skill and handoff artifacts exist. Add opt-in facts with source, project scope, review/update/forget actions and bounded retrieval. Keep stale claims visible and avoid automatic large memory injection. |
| 37 | 3 | Runtime constraint reminders | [OMP stream rules][omp-rules] | Hooks/policies exist. Evaluate opt-in reminders at safe boundaries with deduplication and retry receipts. Mid-stream regex intervention is experimental inspiration, not a security boundary; prefer deterministic checks for forbidden actions. |

### Enrich the existing 17 candidates

- **Agent hub / attention inbox:** OMP exposes parent/child lineage, unread messages, resolved model, retry state, actual usage and parked-agent revival. Extend GC's existing agent board with these fields and narrow-screen roster/inspector navigation; do not create another competing agents screen. [Agent Hub][omp-hub]
- **Code mode:** Pi's extension API separates model-facing and callable tools, supports nested-call tracking, and accounts for nested usage. Extend GC's existing code mode with these contracts and cancellation/backpressure. Do not register a second code execution surface. [Pi extensions][pi-ext]
- **Session tree and handoff:** Pi summarizes the branch being left and retains original session entries. Keep branch summaries inspectable and distinguish selected context from the durable transcript. [Pi sessions][pi-sessions]
- **Terminal customization:** Pi documents external-editor composition, keyboard help and user shell commands whose output can be kept outside model context. Integrate these consistently with GC's composer and command menu where absent. [Pi terminal usage][pi-usage]
- **Remote sessions:** OMP's collaborative view/control distinction is useful for the remote protocol: explicit read-only observers, one input owner, revocable access and an audit trail. A public relay would be a separate security and operations project. [OMP collaboration][omp-home]

### Performance and cost gates for these additions

1. Keep capabilities lazy. Metadata discovery must not start every server, parse every grammar or load every transcript.
2. UI navigation and expanding a card must not make a model request. Show estimated and provider-reported usage separately.
3. Retrieve output artifacts and old messages by range, with byte limits and cancellation. Large results should not be duplicated in every projection.
4. Preserve prompt prefixes where provider semantics allow it. Measure cache behavior rather than promising savings from a particular representation.
5. Make advisor, debugger, AST, memory and remote collaboration optional. Include their startup, memory and token overhead in their own feature budgets.
6. Compare actual GC release performance before/after; verify terminal interactions with PTYs when implementing. This research-only update does not run tests or imply implementation verification.

### Revised shipping recommendation

Start with the tooling foundation phase below, then deliver the first six T3 workbench items. Bring **structured review, typed results, deferred discovery, output artifacts and diagnostic bundles** into that delivery where their dependencies fit. Then build branch/compaction transparency and transactional edit workflows. Leave continuous advisors, debugger adapters and memory behind explicit opt-in until their benefit and overhead are measured.

## Tooling foundation phase

Added at the user's request. These are proposed improvements to existing operations, not claims that the current implementations lack every listed safeguard. Audit current contracts and failure paths first, preserve working protections, then close verified gaps. Read, edit and write can remain native operations behind the small model-facing surface; adding separate tool schemas requires evidence of a net benefit.

| # | Priority | Operation | Scope and acceptance |
| --- | --- | --- | --- |
| 38 | Foundation | Read | Predictable line ranges and revision IDs, explicit truncation with continuation references, targeted rereads and bounded large-file handling. Preserve Unicode and distinguish missing, unreadable, binary and changed files. |
| 39 | Foundation | Edit | Precise stale/ambiguous-edit errors, useful reread hints, readable previews and validation of an entire edit batch before applying it. Compare existing revision-checked and hashline approaches on representative tasks before choosing defaults. Never repair a failed edit by silently overwriting newer content. |
| 40 | Foundation | Write | Explicit create-only versus overwrite semantics, expected revisions, atomic file replacement where supported, optional validation and diagnostics. Document metadata handling, symlink policy and durability guarantees; failed validation must leave original content intact. |
| 41 | Foundation | Bash | Responsive streaming and cancellation, accurate exit/signal/timeout reporting, owned background processes and reconnectable output. Bound previews while retaining accessible full output; distinguish a detached job from a completed command. Honor permissions across shell wrappers and subprocesses. |
| 42 | Foundation | State | Consistent validated operations for tasks and agents, durable update receipts, explicit ownership and revisions, compact results and recoverable errors. Support duplicate-request detection where retries could repeat a mutation. |
| 43 | Foundation | Shared tool contracts and presentation | Stable call IDs and nested lineage, structured status/error/artifact fields, cancellation semantics and bounded repair. Use consistent cards in streaming, history and headless output. Separate display truncation from context truncation; expose recovery actions without duplicating errors. |

### Implementation order

1. Inventory operation schemas, dispatch paths, existing protections and observed failures. Record a release baseline for latency, memory, output size, retries and task token use.
2. Establish shared receipts and error classes while preserving compatibility. Improve read/edit/write correctness and recovery first.
3. Improve bash process lifecycle and state durability; connect the same structured results to the TUI and headless output.
4. Compare task outcomes and performance against the baseline. Enable optional expensive diagnostics only when requested or relevant.

### Verification required during implementation

- Use the actual release build in PTYs at desktop and narrow phone dimensions; cover streaming while scrolling, cancellation and follow-up submission.
- Exercise stale reads, repeated text, Unicode, CRLF, empty/large files, permission failures, symlinks and concurrent edits. Verify failure paths preserve original files.
- Exercise output floods, stderr, nonzero exits, signals, timeouts, detached jobs, reconnect and process cleanup without interrupting unrelated processes.
- Exercise concurrent state updates, malformed requests, duplicate delivery and restart recovery; a receipt must describe what was durably accepted or applied.
- Measure representative coding tasks for retries and token use as well as input/redraw latency, peak memory and startup. Record model/provider variability; do not infer savings from a single successful run.

The roadmap now contains **43 candidates**, including six tooling foundation items. This section changes delivery priority; it does not authorize or claim completed implementation.

### Additional primary sources

[codex-review]: https://learn.chatgpt.com/docs/code-review
[codex-exec]: https://learn.chatgpt.com/docs/non-interactive-mode
[codex-rules]: https://learn.chatgpt.com/docs/agent-configuration/rules
[pi-ext]: https://github.com/earendil-works/pi/blob/c5f5b3282d5e4203c085e59837ba17aeaf2829b5/packages/coding-agent/docs/extensions.md
[pi-sessions]: https://github.com/earendil-works/pi/blob/c5f5b3282d5e4203c085e59837ba17aeaf2829b5/packages/coding-agent/docs/sessions.md
[pi-compact]: https://github.com/earendil-works/pi/blob/c5f5b3282d5e4203c085e59837ba17aeaf2829b5/packages/coding-agent/docs/compaction.md
[pi-packages]: https://github.com/earendil-works/pi/blob/c5f5b3282d5e4203c085e59837ba17aeaf2829b5/packages/coding-agent/docs/packages.md
[pi-usage]: https://github.com/earendil-works/pi/blob/c5f5b3282d5e4203c085e59837ba17aeaf2829b5/packages/coding-agent/docs/usage.md
[omp-home]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/README.md
[omp-hub]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/agent-hub.md
[omp-blobs]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/blob-artifact-architecture.md
[omp-retry]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/non-compaction-retry-policy.md
[omp-resolve]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/resolve-tool-runtime.md
[omp-lsp]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/lsp-config.md
[omp-advisor]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/advisor-watchdog.md
[omp-memory]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/memory.md
[omp-rules]: https://github.com/can1357/oh-my-pi/blob/01e0b5c26e267dff08c305f51d1ab56ec5a2f05d/docs/ttsr-injection-lifecycle.md

## Delivery started

The tooling-first implementation is tracked in [Tooling foundation](../docs/plan/tasks/tooling-foundation-001.md). The first slice adds bounded revision-checked read continuation, mutation previews, strict edit/shell validation, durable state recovery and process-group timeout cleanup. The screenshot-driven reliability work is tracked in [Turn recovery](../docs/plan/tasks/turn-recovery-001.md). These are partial foundations; the remaining candidates are not marked complete.

### Current delivery status

| Item | Completed in first slice | Remaining |
| --- | --- | --- |
| 38 Read | Revision-checked byte continuation, Unicode pagination, strict CLI validation | Broader file error diagnostics and representative task benchmarks |
| 39 Edit | Batch validation, ambiguity/stale hints, dry-run previews | Editing strategy comparison and richer previews/diagnostics |
| 40 Write | Explicit preview receipts, empty creates, documented metadata/atomicity limits | Expanded diagnostics and durability coverage |
| 41 Bash | Invalid-input rejection and process-group timeout cleanup | Background lifecycle, streaming/reconnect and permissions work |
| 42 State | Durable clears/nested updates and rollback | Ownership, revisions, receipts and duplicate-request handling |
| 43 Shared contracts | Typed colored cards across scroll/live, bounded explicit expansion | Unified structured errors/artifacts, lineage and headless parity |

Items 1–37 require an implementation coverage audit; this delivery does not mark them complete.
