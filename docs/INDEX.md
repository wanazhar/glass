# Glass Core design and delivery

- [Six-codebase deep-read](../research/six-codebases-2026-10-10.md): Codex, Pi, OMP, T3 Code, Crush and OpenCode inspected in-tree; 38 feature candidates with verified gaps, per-codebase source findings and a suggested next wave.
- [Expanded roadmap](../research/t3code-parity-2026-10-10.md): T3 Code, Codex, Pi and OMP research, 43 candidates and tooling-first delivery order.
- [Turn recovery](plan/tasks/turn-recovery-001.md): stable prompt recovery, local compaction fallback and consistent tool cards.
- [Tooling foundation](plan/tasks/tooling-foundation-001.md): revision-checked read continuation, mutation previews, explicit shell inputs and durable state recovery.

- [Auto-verify](plan/tasks/auto-verify-001.md): the project verifier runs after writes, through the host bridge.
- [Cross-session memory](plan/tasks/memory-001.md): MEMORY.md index injected per project, user-level only.
- [Image input](plan/tasks/image-input-001.md): PNG/JPEG/GIF/WebP attachments ride as image blocks, no codec.
- [Agent Plugins](plan/tasks/plugins-001.md): Agent Plugins 1.0 loader, gc plugin add/list/remove, plugin MCP servers.
- [Worktree apply-back](plan/tasks/worktree-apply-001.md): gc worktree list/apply reclaims isolated fan-out work.
- [Oracle](plan/tasks/oracle-001.md): /oracle sends the diff to an alternate model, read-only and unrecorded.
- [Sandbox egress](plan/tasks/sandbox-egress-001.md): port-limited outbound TCP and credential env scrubbing when the sandbox is on.
- [Loops](plan/tasks/loops-001.md): /loop fires prompts on an interval while the session is idle.
- [Session comms](plan/tasks/session-comms-001.md): multi-client attach broadcast plus gc send / inbox between sessions.
- [Index](plan/tasks/index-001.md): outline source files before reading them; the repo-map token win, extension-first.
- [LSP](plan/tasks/lsp-001.md): language-server diagnostics appended to tool results after edits.
- [MCP v2](plan/tasks/mcp-v2-001.md): MCP 2026-07-28 negotiation plus Streamable HTTP transport.
- [Durability](plan/tasks/durability-001.md): log repair, effect sandwich, partial sidecar, cursor attach, idempotent submissions.
- [Context edits](plan/tasks/context-edit-001.md): /forget, /unforget, /rewrite change only future model context.
- [Program status](plan/tasks/program-status-001.md): OSC 7501 working/blocked/done/error/idle reports.

- [Subagent fan-out](plan/tasks/subagent-fanout-001.md): batched delegation, named agent definitions, worktree isolation, host-side parallelism.
- [Turn notifications](plan/tasks/notifications-001.md): OSC 9 / bell attention for long or failed turns.
- [Model fallback](plan/tasks/model-fallback-001.md): ordered model failover on pre-event provider failures.
- [Compaction hardening](plan/tasks/compaction-hardening-001.md): manual /compact, overflow-refusal recovery, summary circuit breaker, bounded serialization.
- [/init](plan/tasks/init-001.md): read-only AGENTS.md drafting with review, save, and headless --write.
- [Session export](plan/tasks/export-001.md): raw-log Markdown export, /export and gc export.
- [Web extension](plan/tasks/web-extension-001.md): web_fetch and web_search, out of process, no keys required by default.
- [MCP bridge](plan/tasks/mcp-bridge-001.md): namespaced MCP tools via an extension, deferred past 40 tools.
- [Workspace trust gate](plan/tasks/trust-gate-001.md): consent for AGENTS.md content, keyed by hash, headless --trust.
- [Optional sandbox](plan/tasks/sandbox-001.md): Landlock/sandbox-exec confinement for shell commands, off by default.
- [ACP server](plan/tasks/acp-001.md): glass-core acp, Agent Client Protocol v1 over stdio for editors.
- [Serve and attach](plan/tasks/serve-attach-001.md): a session on a Unix socket, with a terminal client that detaches.

- [Chat readability](plan/chat-readability.md): main chat Markdown, streaming, responsive reading column, and tool feedback.
- [Implementation task](plan/tasks/chat-readability-001.md): module and integration scope, review gates, PTY verification.

- [Color and motion](plan/visual-polish.md): coordinated terminal palettes, active-work animation and composer presentation.
- [Visual polish task](plan/tasks/visual-polish-001.md): implementation and installed PTY verification.

- [Live turn UI](plan/live-turn-ui.md): stable terminal ownership, active transcript keys, expanded commands and provider-exposed reasoning views.
- [Live turn delivery](plan/tasks/live-turn-ui-001.md): screenshot regressions and installed verification.
- [Tool navigation and shortcuts](plan/tasks/tool-navigation-001.md): searchable tool records, exact command/output copying and contextual key discovery.
- [Submitted composer and greeting behavior](plan/tasks/composer-greeting-001.md): immediate input repaint and tool use matched to conversational intent.
- [Inline scrolling](plan/inline-scroll.md): Codex-style reading with pinned input and back-to-bottom navigation.
- [Inline scrolling task](plan/tasks/inline-scroll-001.md): keyboard/mouse integration and small-screen verification.
- [Inline scrolling review](plan/reviews/inline-scroll-001-01.md): independent PTY evidence and remaining provider gates.
- [Scrolling and message spacing fixes](plan/tasks/chat-scroll-spacing-001.md): initial wheel routing and compact replies without role headings.
- [Reader redraw and history fixes](plan/tasks/reader-redraw-001.md): complete body repaint and readable tool/reasoning projection.
- [Reader redraw review](plan/reviews/reader-redraw-001-01.md): filled-screen PTY reproduction and installed-release evidence.

- [October 8 regression fixes](plan/tasks/oct8-regressions-001.md): LSP deadlines, buffered diagnostics and child cleanup.
- [MCP HTTP regression fixes](plan/tasks/mcp-http-regressions-001.md): session negotiation, bounded SSE replies and loopback validation.
- [October 8 verification review](plan/reviews/oct8-regressions-001-01.md): release smoke results and constrained PTY evidence.

- [Pi shipping regression review](plan/tasks/pi-shipping-001.md): durability recovery and compaction-aware context edit fixes.
- [Pi shipping verification](plan/reviews/pi-shipping-001-01.md): final regression tests and small-screen PTY evidence.

- [Round-3 ports](plan/tasks/round3-ports-001.md): subagent report framing, frozen tool list, compaction prompt checkpoint, MCP TTL, OSC 777, /name, serve stop/respawn/logs, extension durable entries, /skill-doctor, /code-review.
- [Round-3 verification](plan/reviews/round3-ports-001-01.md): unit, Clippy, release and full smoke evidence, with the envelope and freeze limitations stated.

- [Round 3 regression fixes](plan/tasks/round3-regressions-001.md): read-only logs, resumed checkpoints, extension state and graceful stop verification.
- [Round 3 verification](plan/reviews/round3-regressions-001-01.md): full release suite and runtime integration evidence.

- [Twenty-item shipping round](plan/ship-next-20.md): completed reliability, terminal and extension delivery.

- [Twenty-item final verification](plan/reviews/ship-final-001-01.md): 571 tests, 40 release smoke suites, independent reviews, small-screen PTYs and performance evidence.

- [Scroll redraw and return-to-tail fix](plan/reviews/scroll-redraw-001-01.md): shared viewport geometry, synchronized frames, wheel/arrow roundtrips and five-size release PTYs.

- [Persistent teammates](plan/persistent-agents.md): reusable workers, mailboxes, interaction previews, transcript paging, human steering and token budgets.
- [Persistent teammate verification](plan/reviews/persistent-agents-001-01.md): release PTYs, live Kilo review, runtime/UI reviews and performance evidence.
- [Empty provider response and error replay fixes](plan/reviews/empty-provider-response-001-01.md): bounded reasoning recovery, stream diagnostics, context inspection and red wrapped transcript errors.
- [Streaming and scrolling presentation](plan/reviews/stream-scroll-presentation-001-01.md): atomic styled rows, color separation, retained statistics, clean scrolling footer and Unicode preview safety.

- [CLI help](plan/reviews/cli-help-001-01.md): terminal-aware command guides, examples, color and pipe behavior.

- [CLI design and resume](plan/reviews/cli-design-resume-001-01.md): shared command presentation and a searchable startup session picker.

- [Coding workflows](workflows.md): native task/debug/review/output views, request and cache diagnostics, bounded teammate roles, optional browser/LSP integration, policies and workflow regression gates.
- [Workflow delivery matrix](plan/workflow-next-20.md): twenty-feature scope and verification evidence.

- [Workflow verification](plan/reviews/workflow-next-20-001.md): release PTYs, unit tests, review fixes, performance measurements and v1 limitations.

- [Model-managed teammate limits](plan/reviews/agent-core-limits-001.md): visible allowances, configurable worker limits, compact receipts and preserved tool/context recovery.

- [History and handoff contracts](plan/tasks/history-contracts-001.md): bounded compaction receipts, branch lineage and portable context previews.

- [Six-codebase feature wave](plan/tasks/six-codebases-wave.md): planned five-wave split with disjoint path ownership.
- [Six-codebase research](../research/six-codebases-2026-10-10.md): deep-read of codex, pi, t3, OMP, opencode and crush — feature candidates that drove the wave.
- [Wave A: agent-loop correctness](plan/tasks/six-wave-a-001.md): loop detection, read recency, sticky routing, cache affinity (`e06417d`).
- [Wave B: security hardening](plan/tasks/six-wave-b-001.md): process hardening, admin requirements layer, egress policy proxy (`3e8fc78`).
- [Wave C: context management](plan/tasks/six-wave-c-001.md): checkpoint/rewind tools, wait, prewalk handoff, branch summarization (`06ba3b2`).
- [Wave D: representation + perf](plan/tasks/six-wave-d-001.md): mermaid renderer, fs-scan cache, blob addressing, OSC 8 hyperlinks (`1b8ea54`).
- [Wave E: extensibility + review](plan/tasks/six-wave-e-001.md): pre-tool-use hook, typed notify events, codemode parity audit (`74bb707`).
- [Delivery ledger](plan/tasks/roadmap-delivery.md): per-commit evidence and integrated verification for every shipped wave.
- [Six-codebase follow-up: citations + blob GC](plan/tasks/six-wave-e-001.md): in-message `@path#L..#hash8` markers now ship, and `gc blobs sweep` removes unreferenced blobs; corrects the wave E note that claimed a session.rs dedupe helper pre-existed (follow-up commit).
- [Six-codebase follow-up 2](plan/tasks/six-wave-e-001.md): codemode ships the pi output-item protocol (@options, `==> text N/M <==`, console block, overflow spill, enforced timeout) and mermaid renders sequence diagrams natively within the existing caps (follow-up commit).

- [Neuralnets and model guidance protocol](plan/neuralnets.md): local attributed learning, human review, versioned capability discovery, cache and security boundaries.
