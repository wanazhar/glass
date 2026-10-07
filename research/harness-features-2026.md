# Features Across Open-Source Coding Harnesses — State of the Art, October 2026

Research compiled from primary sources: repository source, official documentation, changelogs,
and protocol sites. The field is large — one curated index lists 130+ CLI coding agents
([bradAGI/awesome-cli-coding-agents](https://github.com/bradAGI/awesome-cli-coding-agents),
updated 2026-10-05) — so this survey covers the prominent harnesses and the features that are
converging across them. Star counts and "trending" signals come from that directory and one
daily digest ([digest](https://startupcorners.com/digest/devtools-digest-2026-10-04)); treat
magnitudes as directional. Anything unconfirmed is marked **UNVERIFIED**.

This document supports the harness-feature port backlog; it is a companion to
`harness-context-management-2026.md` (compaction) and `prompt-caching-2026.md`.

---

## PART 1 — PER-HARNESS FINDINGS

### 1. Claude Code

- **Agent teams** — peer sessions with a shared task list, mailboxes, `SendMessage`, and tmux/iTerm split-pane orchestration, on top of subagents. Tokens scale linearly with teammates; wall-clock drops on parallelizable work.
- **Subagents** — isolated-context delegates that return only a summary + metadata trailer.
- **Plugins/mods** with a deep hook API (`agent.spawn`, `prompt.autocomplete`, `$.ui.*`) and marketplaces.
- **Sandboxed bash** — bwrap/Seatbelt-backed, domain prompts, plus a documented comparison of runtime/dev-container/Docker/VM isolation.
- **Tool search** — deferred tool schemas so large tool catalogues don't inflate every prompt; vendor guidance is that under ~10 tools upfront loading is faster.
- Remote Control / `claude attach`, background sessions, `/loop` scheduled tasks surviving restarts, cross-session messaging, Claude in Chrome, `/ultrareview`, screen-reader mode, vim mode, `keybindings.json`.
- **Auto-memory** and claude.ai skill sync.

Sources: [agent teams](https://code.claude.com/docs/en/agent-teams),
[subagents](https://code.claude.com/docs/en/sub-agents),
[changelog](https://code.claude.com/docs/en/changelog),
[sandboxing](https://code.claude.com/docs/en/sandboxing),
[sandbox environments](https://code.claude.com/docs/en/sandbox-environments),
[tool search](https://code.claude.com/docs/en/agent-sdk/tool-search).

### 2. Codex CLI (OpenAI)

- **Sandbox modes** mapped onto Seatbelt / Bubblewrap+Landlock+seccomp / Windows restricted tokens.
- **Subagents** (GA Mar-2026, ~6 concurrent) with `explorer`/`worker` roles and TOML custom agent configuration; a "guardian" auto-approval reviewer.
- Hooks, including a `PostToolUse` pattern used for "auto-run tests after every file write".
- **Web search** built in; **browser** and **computer use**; **scheduled automations**.
- **Memories** / Computer History for cross-session recall.
- **Record & replay** with fork-to-another-model.
- **Git worktrees** as first-class environments; app-server + SDK.

Sources: [sandboxing](https://developers.openai.com/codex/sandboxing),
[subagents](https://developers.openai.com/codex/agent-configuration/subagents),
[web search](https://developers.openai.com/codex/web-search),
[browser](https://developers.openai.com/codex/browser),
[computer use](https://developers.openai.com/codex/computer-use),
[automations](https://developers.openai.com/codex/automations),
[memories](https://developers.openai.com/codex/customization/memories),
[record and replay](https://developers.openai.com/codex/extend/record-and-replay),
[worktrees](https://developers.openai.com/codex/environments/git-worktrees),
[2026 roundup (secondary)](https://codex.danielvaughan.com/2026/03/27/codex-cli-in-2026-whats-new/).

### 3. opencode (sst / anomalyco)

- **Agents/subagents** with per-agent models and glob-scoped permissions, `@`-mentions, child-session navigation, and a `doom_loop` recovery key.
- **LSP diagnostics** wired into edits (40+ servers).
- **Session share links** — opt-in, since conversations are not shared by default.
- Terminal + desktop + IDE clients.

Sources: [agents](https://opencode.ai/docs/agents/), [LSP](https://opencode.ai/docs/lsp/),
[share](https://opencode.ai/docs/share/), [docs](https://opencode.ai/docs/).

### 4. Crush (charmbracelet)

- **Shared workspaces** — `crush serve` lets two TUIs (or more clients) attach to one *running* session over SSE, live-mirrored, with `IsBusy`/`AttachedClients` state and first-wins `--yolo` semantics.
- Bash-interpreted `crushrc`; load-time command execution caveat documented in-tree.
- **MCP with OAuth** including pre-registered clients, sessionless-server handling.
- **LSP**; **skills** discovered from `~/.agents/skills`, `~/.claude/skills`, `.cursor/skills` and the [agentskills.io](https://agentskills.io) standard.
- **Desktop notifications**: `auto|native|osc|bell|disabled` — native locally, OSC over SSH.
- `.crushignore`; live theme editing; model-catalogue auto-update.

Source: [README](https://github.com/charmbracelet/crush).

### 5. Aider (Paul Gauthier)

- **Repo map** — tree-sitter symbols, dependency graph, PageRank-ranked selection under a `--map-tokens` budget. The canonical context-selection implementation.
- **Auto-lint / auto-test after every edit** — per-language linters, `--lint-cmd`, `--test-cmd`, `--auto-test`, `/run`; failures feed back to the model.
- Architect/editor two-model split (`/architect`, `/editor-model`, `/weak-model`).
- `/voice` dictation. Git-as-state: edits are commits.

Sources: [repomap](https://aider.chat/docs/repomap.html),
[lint/test](https://aider.chat/docs/usage/lint-test.html),
[modes](https://aider.chat/docs/usage/modes.html),
[voice](https://aider.chat/docs/usage/voice.html).

### 6. Cline

- Plan/Act modes, checkpoints, MCP Marketplace, and a **Kanban board** that also drives other agents (Claude Code, Codex, opencode).

Sources: [CLI](https://cline.bot/cli), [kanban](https://docs.cline.bot/usage/kanban),
[checkpoints](https://docs.cline.bot/core-workflows/checkpoints).

### 7. Roo Code

- Built-in modes plus **Boomerang/Orchestrator subtasks**: isolated child context, summary returns to the parent. A convention the model follows when given a task tool — the value is fan-out + isolation, not ceremony.

Source: [boomerang tasks](https://roocodeinc.github.io/Roo-Code/features/boomerang-tasks/).

### 8. Gemini CLI → Antigravity CLI

- **Extensions** bundling prompts + MCP servers + commands + themes + hooks + sub-agents + skills; install from GitHub with a gallery.
- Checkpointing. **Consumer tiers sunset 2026-06-18** in favour of the Go-based Antigravity CLI.

Sources: [extensions](https://geminicli.com/docs/extensions/),
[checkpointing](https://geminicli.com/docs/cli/checkpointing/),
[transition blog](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/).

### 9. Goose (Block → Agentic AI Foundation)

- **Recipes** — portable prompt + extensions + settings bundles — with **parallel subrecipes**.

Source: [recipes](https://goose-docs.ai/docs/guides/recipes/).

### 10. OpenHands

- Cloud + local GUI + Docker sandbox; Python/REST **Agent SDK**; a separate automation/scheduling repository.

Sources: [intro](https://docs.openhands.dev/overview/introduction), [SDK](https://docs.openhands.dev/sdk).

### 11. Amp (Sourcegraph)

- **Orbs** — per-thread cloud machines that keep working when the laptop closes.
- Threads across web/macOS/iOS/TUI with notifications; **Puck** (agent coordinator); **`oracle`** second-opinion tool; model routing/subagents. "Toolboxes" reported but **UNVERIFIED**.

Sources: [docs](https://ampcode.com/docs), [tools](https://ampcode.app/docs/tools),
[models/subagents](https://ampcode.com/docs/models-and-subagents).

### 12. pi (@earendil-works/pi)

- Minimal harness: unified LLM API + loop + TUI + SDK; extensions, skills, prompt templates, themes, packages; print/JSON-stream/**RPC** modes.
- **Explicit project trust** that controls which project resources load — and explicitly does *not* sandbox tools.
- Author's stance: "I've been moving all my MCPs to skills."

Sources: [pi docs](https://pi.dev/docs/latest),
[skills-vs-MCP](https://lucumr.pocoo.org/2025/12/13/skills-vs-mcp/),
[Pi essay](https://lucumr.pocoo.org/2026/1/31/pi/).

### 13. qwen-code (QwenLM)

- Five approval modes; **`/goal`** autonomous coding with an independent judge model and impossible-goal detection; LLM-classifier auto-approval.
- Compaction circuit-breaker redesign (three consecutive failures, not one permanent lock) documented in its design docs.

Sources: [approval mode](https://qwenlm.github.io/qwen-code-docs/en/users/features/approval-mode/),
[weekly update](https://qwenlm.github.io/qwen-code-docs/en/blog/updates/weekly-update-2026-05-21/).

### 14. GitHub Copilot CLI

- Instructions/skills/hooks/subagents/custom agents/plugins/marketplaces; `/plugin update`.
- **`/fleet`** — splits a plan into parallel subagents with per-subtask models.

Sources: [comparison](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/comparing-cli-features),
[fleet](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/fleet).

### 15. Droid (Factory AI)

- **Spec Mode** — plan → approve → implement (`Shift+Tab`, `--use-spec`); interactive + `droid exec`.
- "#1 on Terminal-Bench" **UNVERIFIED**.

Sources: [spec mode](https://docs.factory.com/autonomy-and-safety/specification-mode),
[CLI reference](https://docs.factory.com/droid-cli/cli-reference).

### 16. Newcomers worth watching

- **t3code** — "agent harness control surface": remote-ready UI over local CLI agents + git + terminals.
- **Maki** (Rust) — tree-sitter indexing; sandboxed `code_execution` that chains tool calls without polluting context.
- **Prime Agent** — IPython-kernel-as-only-tool, "Continual Harness" with `/refine`.
- Around them: Continue, OpenSquilla, Reasonix, Codewhale, and a whole genre of parallel-session dashboards (vibe-kanban, cmux, herdr, Orca).

Sources: [t3code](https://github.com/pingdotgg/t3code),
[t3code deepwiki](https://deepwiki.com/pingdotgg/t3code),
[awesome list](https://github.com/bradAGI/awesome-cli-coding-agents).

---

## PART 2 — WHAT IS CONVERGING

1. **Subagent fan-out.** Claude Code (agent teams + subagents), Codex (roles, ~6 concurrent),
   Copilot (`/fleet`), opencode (primary/subagents), Roo (Boomerang), Goose (parallel subrecipes),
   Kilo (native subagents, after deprecating its orchestrator mode). The 2026 UX win is
   *fan-out + isolation* — exploration, review, and multi-module work stop eating the main context.
2. **OS-level sandboxing**, shipped by default on several newcomers: Codex (one policy enum over
   Seatbelt/Bubblewrap/Landlock/DACLs), Claude Code (bwrap/Seatbelt), Orca (fail-closed
   Landlock+seccomp). It is what makes "no permission prompts" defensible for unattended runs.
3. **Repo trust gates.** pi separates project trust (which resources load) from permissions, and
   warns trust does not sandbox tools. Better documentation of load-time execution risks is part
   of the trend (Crush's `crushrc` warning).
4. **Agent definition files** — TOML (Codex), markdown with frontmatter (Claude Code, opencode) —
   per-agent model, prompt, and permissions.
5. **A skills standard** ([agentskills.io](https://agentskills.io)) with cross-harness discovery
   directories; the "skills vs MCP" debate has moved toward skills for local capability.
6. **Web search/fetch as first-class tools** (Claude Code, Codex, opencode).
7. **Turn-completion notifications** (`auto|native|osc|bell`) — the cheapest lever that makes
   long autonomous runs usable away from the keyboard.
8. **Auto-verification after edits** (Aider's lint/test loop; Codex `PostToolUse`; Claude's
   verify-before-commit guidance).
9. **Deferred tool loading** (`tool search`) as tool catalogues grew past what a prompt should pay for.
10. **Session mobility** — attach/remote-control (`claude attach`, `crush serve`, Amp threads,
    t3code's whole premise).

---

## PART 3 — RANKED GAPS FOR GLASS-CORE (survey result)

Ordering = user value × fit. Difficulty assumes Rust, no tokio, blocking I/O, and the existing
NDJSON extension protocol.

| # | Candidate | Shipped by | Difficulty | Home |
|---|---|---|---|---|
| 1 | Parallel subagent fan-out + agent definitions + worktree isolation | Claude Code, Codex, Copilot, opencode, Roo, Goose, Kilo | medium | extension + host |
| 2 | OS-level sandbox for bash (opt-in per workspace) | Codex, Claude Code, newcomers | medium (Linux+macOS) / large (parity) | core |
| 3 | Repo trust gate for instructions/skills/extensions | pi, Claude Code (folder trust), Crush (load-time warnings) | small | core |
| 4 | Session server + attach | Crush, Claude Code, Amp, t3code | medium–large | core |
| 5 | ACP server mode for editors | Zed ecosystem (Cline, Codex adapter, Droid, Gemini, Copilot) | medium | core |
| 6 | Skill/extension ecosystem interop + installer | Crush, Claude Code, Copilot, Gemini, pi | small–medium | core |
| 7 | First-class web search + fetch | Claude Code, Codex, opencode | small | extension |
| 8 | Auto-verification loop after edits | Aider, Codex hooks, Claude guidance | small | mod + skill |
| 9 | Repo map / symbol index | Aider, Maki, Zap, Kilo | medium–large (tree-sitter) / medium (ctags+grep) | extension |
| 10 | MCP client as out-of-process bridge | Crush, Copilot, Codex, Gemini | medium | extension |
| 11 | Cross-session memory | Codex, Claude, Letta, OB-1, Prime | medium | extension on `state` |
| 12 | Turn-completion notifications | Crush, Codex, Amp | small | core |
| 13 | Image/screenshot input | opencode, Codex, Claude Code | small–medium | core |
| 14 | Scheduled / autonomous runs | Codex automations, Claude `/loop`, qwen `/goal` | small–medium | extension/scheduler |
| 15 | Browser automation / computer use | Codex, Claude in Chrome, Cline | large | extension |

Adopted for the current port wave (see `docs/plan/`): items 1, 3, 4, 5, 7, 10, 12 and compaction
hardening (manual `/compact`, overflow-refusal recovery from Crush's #3620 cascade, circuit
breaker from qwen-code), plus `/init` and session export.

---

## PART 4 — OVERHYPED / NOT PORTED

- **MCP everywhere.** Shipped by all major harnesses because the ecosystem expects it, but a
  bash-first agent with network access reaches most of those APIs directly, and the conversation
  has turned against universal MCP for local dev: "Did Skills Kill MCP?"
  ([round-up](https://www.speakeasy.com/blog/skills-vs-mcp)), "I've been moving all my MCPs to
  skills" ([Ronacher](https://lucumr.pocoo.org/2025/12/13/skills-vs-mcp/)), MCP's durable value
  being auth isolation ([Willison](https://simonwillison.net/tags/skills/)). If it ships at all,
  it ships as an out-of-process bridge, never core.
- **Dedicated orchestrator modes / agent swarms.** Kilo deprecated its Orchestrator mode and now
  says the current agent already has the capability
  ([docs](https://kilo.ai/docs/code-with-ai/agents/orchestrator-mode)). Value is in fan-out +
  isolation, not ceremony.
- **Parallel-session dashboards / Kanban boards / desktop control planes.** Real products, wrong
  layer: expose a clean attach protocol and let them build on it.
- **Cloud orbs / hosted agent VMs.** Infrastructure businesses, not features a local minimal
  harness should copy.
- **Huge built-in surface area.** 78 slash commands, 42 tools, 40+ LSP servers, ~230 tool
  interfaces in various references — the *tool search* trend exists because that surface is a
  cost. A small verified tool surface is a feature.
- **Session share links.** Great demo, real privacy cost (opencode gates them behind `/share` for
  that reason). A local Markdown/HTML export gets most of the value.
- **Voice input.** Demo-delightful, rarely used for real refactors; drags in audio capture + STT.
- **Computer use as a headline capability.** Flaky and token-hungry; wrap someone else's driver.
- **Self-improving / "continual harness" claims.** The most interesting research direction and the
  least verifiable; do plain memories and manual skill authoring first.
- **Benchmark leaderboards as procurement signals.** Claims were not reproducible from primary
  sources in this survey (**UNVERIFIED**).

---

## PART 5 — LIMITATIONS

- Primary sources dominate (official docs, repo READMEs, changelogs, protocol sites); secondary
  claims are flagged inline. The landscape moves weekly.
- Not surveyed deeply (no unique primitive contributed): Continue, OpenSquilla, Reasonix,
  Codewhale, SWE-agent, Junie/Cursor CLIs.
- The directory-based prominence signal is directional, not a census.
- "forghetti" does not resolve to any coding harness in any index queried — treated as a mistaken
  name and dropped.
