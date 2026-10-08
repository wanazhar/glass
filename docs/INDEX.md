# Glass Core design and delivery

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
