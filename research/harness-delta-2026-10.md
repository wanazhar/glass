# Harness delta survey — August–October 2026

Follow-up to `harness-features-2026.md`. Compiled 2026-10-08 from primary sources: vendor changelogs,
GitHub release bodies and API metadata, official docs, and protocol specifications, all fetched
2026-10-07/08. Version→date mapping for Claude Code comes from npm registry publish timestamps
(`https://registry.npmjs.org/@anthropic-ai/claude-code`); other dates come from GitHub release
timestamps. Star counts are directional. Anything I could not confirm is marked **UNVERIFIED**.

## Headline

The field has mostly converged. Release notes for the last ten weeks are dominated by provider
plumbing (GPT-6 / Opus 5.5 / Fable 5.1 model churn), TUI polish, and reconnect/retry hardening —
not new paradigms. Four things are genuinely new, plus two corrections to the last survey:

1. **A cross-vendor plugin/skill packaging standard actually shipped** — Agent Plugins 1.0
   (published Aug 6, GA in Copilot clients Aug 12), with AWS, Anysphere, Microsoft, OpenAI,
   Vercel plus Google as maintainer.
2. **"Workflows as code"** — durable, checkpointable orchestration programs — landed in Copilot CLI
   (Oct 1) and already existed in Claude Code as the Workflow tool (Aug–Oct iteration).
3. **Sandboxing moved from filesystem to network and credentials.** Claude Code now does per-command
   domain allowlists reviewed with the command, TLS termination, credential masking (JWT-aware,
   AWS SigV4 re-signing). Copilot CLI's local sandbox went GA Oct 7 with network, credential and
   MCP/LSP coverage. This is exactly glass-core's stated gap.
4. **Mod/extension APIs got deep enough to be a platform** — Claude Mods (Oct 1) exposes deferred
   tool registration, prompt autocomplete, cached model calls, and `agent.spawn`.

Corrections to the last survey's baseline: **Roo Code is archived** (last release v3.54.0, repo
read-only, shut down 2026-05-15) and **Aider has been dormant since 2026-05-22**. Both were
presented as live state of the art. Everything else the last survey listed is still true; a few
"UNVERIFIED" items there (Amp toolboxes) are now confirmable.

---

## PART 1 — What is new (Aug–Oct 2026)

### 1.1 Agent Plugins 1.0 — the packaging standard arrived (the biggest structural change)

Published **Aug 6, 2026** by GitHub with AWS, Anysphere (Cursor), Microsoft, OpenAI and Vercel;
Google joined as core maintainer the same day. **GA in VS Code, Copilot CLI, GitHub Copilot SDK
and the Copilot app on Aug 12, 2026**, on all plans.
([GitHub changelog](https://github.blog/changelog/2026-08-12-agent-plugins-1-0-in-vs-code-copilot-cli-and-the-copilot-app/),
[spec](https://github.com/agentplugins/agent-plugins-spec/blob/main/spec/1.0.0.md),
[authors' guide](https://agent-plugins.org/plugin-authors/build-an-agent-plugin))

Package shape: root `plugin.json` (`$schema` → agent-plugins.org schemas), optional `skills/`
(validated against the Agent Skills spec), optional `mcp.json` at the root, and vendor-namespaced
directories (`com.github.copilot/`) that other clients ignore. Clients can discover skills and MCP
config from the same package; distribution, enablement and install UX are deliberately out of scope.
An "Awesome Copilot" marketplace ships by default ([github/awesome-copilot](https://github.com/github/awesome-copilot)).

Independent confirmations that competitors are converging on the same artifacts:
- Codex CLI "Install portable Agent Plugins and search across local, personal, workspace, and remote
  plugin catalogs" — rust-v0.147.0, **2026-08-07** ([releases](https://github.com/openai/codex/releases)).
- Codex imports Cursor-managed skills and syncs imported Claude and Cursor conversations — same
  release ([releases](https://github.com/openai/codex/releases)).
- Copilot CLI v1.0.89 (**2026-09-28**) "Add support for Claude Code rule files in `.claude/rules` as
  custom instructions" ([releases](https://github.com/github/copilot-cli/releases)).
- Claude Code `/init` and `/import` read Cursor rules, `.github/copilot-instructions.md`,
  `.devin/rules/`, `.windsurf/rules/` and `.clinerules`; and Claude Code now reads `AGENTS.md`
  directly (v2.1.277, ~2026-09-20) ([memory docs](https://code.claude.com/docs/en/memory)).
- Cross-harness skill interop continues under [agentskills.io](https://agentskills.io) and
  [agents.md](https://agents.md/), both live.

### 1.2 Workflows as code (orchestration becomes a program)

- **Copilot CLI / Copilot app / Copilot SDK, Oct 1, 2026**: "Dynamic workflows" — a program that
  defines how a task is carried out, combining deterministic steps with agents, with sequential and
  parallel stages, structured results passed between stages, subagents that verify each other,
  checkpoints you can pause and resume. Shipped inside a Copilot extension so it gets the extension
  API, and the docs explicitly contrast it with `/fleet` (delegation) — workflows are *code*.
  Public preview.
  ([GitHub changelog](https://github.blog/changelog/2026-10-01-dynamic-workflows-in-copilot-cli-and-the-copilot-app/),
  [docs](https://docs.github.com/copilot/concepts/agents/dynamic-workflows))
- **Claude Code** has the equivalent as "Workflow tool"/dynamic workflows: `/workflows` run list,
  `agent()`, `parallel()`, `pipeline()`, a per-run agent cap that is configurable
  (`CLAUDE_CODE_WORKFLOW_MAX_CONCURRENT_AGENTS`, 1–256), workflow agents in the `agent.spawn` mod
  hook with run and index, and default workflow *sizes* per plan. This is where fan-out became a
  first-class, restartable artifact instead of a tool call.
  ([changelog](https://code.claude.com/docs/en/changelog))

### 1.3 Mods go deep (Claude Mods, Oct 1)

Claude Code v2.1.287 "Added Claude Mods: plugins may now modify deeper behavior"
([changelog](https://code.claude.com/docs/en/changelog)), with a first-party example
("You should know", a side agent that flags things you or Claude might miss). Through Oct 6–7 the
mod API gained: `$.tool.register` with `isDeferred` (tool schemas deferred behind tool search by
default), `prompt.autocomplete` rows in the prompt box, `$.model.complete` with prompt caching,
`agent.spawn` for teammates with `$.agent.list()` states, `$.ui.selection()`, typed theme keys,
`plugin eval` (scored, reproducible plugin evals with JSON+HTML reports), `plugin validate`, and a
`gh api`-style built-in. Direction of travel: the harness is a platform, and third parties
extend the *agent loop*, not just the tool list.

### 1.4 Sandboxing → network and credentials

- Claude Code: per-command `allowed_domains` in auto mode with sandboxing — the hosts a command
  needs are reviewed *with* the command and opened for it alone, other hosts refused; sandbox
  credential masking with `extract`/`onExtractNoMatch`, `decode: "jwt"` with `maskClaims`, and
  `awsPairs`/`sigv4` AWS SigV4 re-signing, requiring `network.tlsTerminate`; managed sandbox
  read-deny paths; project settings can no longer widen an admin-required sandbox.
  ([changelog](https://code.claude.com/docs/en/changelog))
- Copilot CLI: **local sandboxing GA Oct 7, 2026**, powered by Microsoft eXecution Container (MXC),
  translating one policy to Windows/macOS/Linux controls: limit readable/writable files, control
  access to the internet, local networks, git and GitHub CLI credentials, apply sandboxing to local
  tools *including MCP and language servers*, and let enterprises require sandboxing that developers
  cannot weaken.
  ([GitHub changelog](https://github.blog/changelog/2026-10-07-local-sandboxing-for-github-copilot-now-generally-available/))
  Its release notes also show the plumbing: proxy CA trust management (`copilot sandbox ca`),
  masked git credentials with SSH remote rewrites, `GITHUB_TOKEN` withheld from sandboxed shells by
  default, network denials that name bypass guidance, and `permissions.limitTo` for managed domain
  boundaries.
- Codex: "Approved filesystem escalation can now grant broader write access while preserving denied
  reads and network restrictions" (rust-v0.161.0, Oct 7) — escalation is scoped, not a hole.

### 1.5 Session mobility hardened (and went multi-client)

- Claude Code: `claude attach|logs|stop|respawn|rm` (name-prefix accepted), Remote Control for
  phone/web, forking a Remote Control session into a local background session, `/teleport`, a
  **self-hosted runner** (`claude self-hosted-runner`) so your own machines host cloud sessions, and
  cross-session messaging that works on Bedrock/Vertex/Foundry and with telemetry off.
- Codex: `codex agents` dashboard (search/start/open/rename/stop, now with worktree ownership and
  archiving), `codex queue` to send messages to existing local *or remote* sessions, `@`-mentions of
  other tasks, shared background server with daemon lifecycle commands, `--no-daemon` opt-out.
- Copilot CLI: `--server` sessions, `--acp` and `--ahp-host`, enterprise managed settings applied in
  all three, a Ctrl+E environment picker switching local vs cloud runs, `--resume/--connect` using
  your GitHub auth.
- Amp: shared runners (`--share`), Mac app starts a runner, one runner serves many directories and
  creates worktrees per thread, iOS/macOS apps; t3code: relay tunnels, LAN/tailnet address learning,
  mobile app, "T3 Code MCP" so agents can create/message/wait on/interrupt threads.
  ([Amp news](https://ampcode.com/news), [t3code releases](https://github.com/pingdotgg/t3code/releases))
- Crush remains the "two TUIs on one session over SSE" reference
  ([README](https://github.com/charmbracelet/crush)).

### 1.6 Agent-to-agent messaging is now table stakes in the big harnesses

Claude Code: cross-session `SendMessage`/`ListAgents` on the same machine, now across machines; new
`crossSessionInbound` (hold messages for approval when the target runs bypassed permissions) and
`dialogExpiry` settings; `notify_when_idle` one-shot pings; a dedicated reviewed "hand-back" call
for subagent results, and subagent output framed so it cannot pass as session instructions.
Codex: task `@`-mentions, agents can read/create/message tasks from the terminal.
Amp: agents spawn and message other agents across threads; Puck is a meta-agent with per-thread
instances ("Many, Many Pucks", Oct 6).

### 1.7 Voice shipped for real (whether or not it is used)

Codex: experimental `/voice` Sep 17, **enabled by default Sep 22** with an F8 toggle, `/voice
settings`, and bundled audio runtimes for Linux and Windows; microphone/channel selection Oct 7.
qwen-code: browser "Live Voice" calls with microphone levels, screen sharing into voice calls, and a
standalone voice daemon. Amp: "Talk to Puck" realtime voice (Aug 18). Droid and OpenHands ship
dictation. Aider's `/voice` is the ancestor.
([Codex releases](https://github.com/openai/codex/releases), [qwen-code releases](https://github.com/QwenLM/qwen-code/releases), [Amp news](https://ampcode.com/news))

### 1.8 Memory got a file format (and an auditor)

Claude Code auto memory: per-repository directory at `~/.claude/projects/<project>/memory/` with a
`MEMORY.md` index loaded every session (first 200 lines or 25KB, whichever comes first) plus topic
files read on demand; typed entries (`user`/`feedback`/`project`/`reference`); `modified`
frontmatter timestamps; subagents can have their own memory. Also `/doctor prompt-audit` (audit
CLAUDE.md/skills/commands/agents for patterns written for older models) and `/skill-doctor` (which
skills go unused and what they cost in context). Codex has memories + Computer History.
([memory docs](https://code.claude.com/docs/en/memory), [Codex memories](https://developers.openai.com/codex/customization/memories))

### 1.9 MCP 2026-07-28 is a breaking revision — plan for it

The largest revision since launch: stateless core (no `initialize` handshake, no protocol-level
sessions, no `Mcp-Session-Id`), `server/discover` must be implemented, `subscriptions/listen`
replaces HTTP GET + `resources/subscribe`, Multi Round-Trip Requests replace server-initiated
`roots/list`/`sampling`/`elicitation`, Tasks becomes an official extension with polling, list
results must carry `ttlMs`/`cacheScope`, deterministic `tools/list` ordering is recommended
(prompt-cache friendly), and OAuth moves toward Client ID Metadata Documents with DCR deprecated.
Roots, Sampling and Logging are deprecated with a 12-month window; HTTP+SSE transport is formally
Deprecated.
([spec changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog),
[release blog](https://blog.modelcontextprotocol.io/posts/2026-07-28/))
Adoption is visible in clients: Claude Code negotiates 2026-07-28 by default for stdio servers;
Codex supports "the opt-in MCP 2026-07-28 protocol, including paginated discovery, multi-round
requests, and non-blocking server startup" (Aug 7); pi has project overrides, OAuth client names and
CIMD; Copilot adds `/mcp login`, `--mcp-github-auth` and per-server connect timeouts.

### 1.10 Smaller deltas worth knowing

- **Terminal-level agent status**: pi added OSC 7501 "program status" so terminals and dashboards can
  show working/blocked/done/failed; Copilot CLI added OSC 777 notifications for Ghostty/WezTerm.
  ([pi v1.1.0, 2026-10-07](https://github.com/earendil-works/pi/releases))
- **Skill/plugin hygiene**: `/skill-doctor`, `claude plugin eval`, `/usage` analytics for plugin and
  skill activity (Codex), skill allow-lists (OpenHands replaced all-on catalog with explicit
  allow-list), organization skill policy (Droid).
- **Review as a first-class agent**: Claude `/code-review` + `/ultrareview` (cloud review of a local
  branch, `--max-findings`), Copilot code review API (Oct 2), Droid's Droid Shield scans exactly
  what a commit publishes before allowing it.
- **Git-flow polish**: Amp "Restack" (turn a thread's changes into a cleaner commit series, Sep 11);
  Codex worktrees default-on (Sep 22) with `/worktree`, browsing and resume; GitHub **stacked pull
  requests GA** (Oct 6).
- **Local models**: Copilot CLI `/model` discovers Ollama models (Oct 7) with explicit offline mode;
  goose has local inference (Vulkan on Linux); qwen ships a Kubernetes CSI runtime for durable
  workers.
- **New entrants**: **DeepSeek Harness**, described as an MIT-licensed agent framework on the Cordis
  meta-framework (Aug 2026) — I could not locate a primary repository in this pass: **UNVERIFIED**.
  **Prime Agent** (Prime Intellect) v0.9.3 (2026-09-06) formalizes a persistent IPython REPL +
  "Continual Harness" that rewrites its own scaffolding
  ([arXiv 2608.23552](https://arxiv.org/abs/2608.23552)).

### 1.11 Landscape corrections (things the last survey got wrong or stale)

- **Roo Code is dead.** Repo `RooCodeInc/Roo-Code` is **archived**, last push 2026-05-15, final
  release v3.54.0; an official sunset notice says all products shut down that day and roocode.com now
  redirects to roomote.dev (a different product from the same company). Boomerang subtasks are a
  historical feature, not a live competitor.
  ([repo](https://github.com/RooCodeInc/Roo-Code), [sunset doc](https://github.com/RooCodeInc/Roo-Code-Docs/blob/main/docs/sunset.md))
- **Aider is dormant.** Last commit 2026-05-22, last release v0.86.1 (2025-08-09 by tag date); the
  `main branch` section of HISTORY.html is model-name churn and small fixes (repo-map tagging for
  Fortran/Haskell/Julia/Zig, `/ok`). Aider's repo map and lint/test loop remain the canonical
  references, but nothing new is coming from there.
  ([repo metadata](https://github.com/Aider-AI/aider), [HISTORY](https://aider.chat/HISTORY.html))
- Project moves: goose is now `aaif-goose/goose` (Agentic AI Foundation); OpenHands is now
  `OpenHands/OpenHands`; pi releases come from `earendil-works/pi`.
- Run rates, for calibration: Claude Code 2.1.221 (Aug 3) → 2.1.293 (Oct 7), 62 versions in 66 days;
  Copilot CLI 1.0.78 (Aug 3) → 1.0.94 (Oct 7); Codex ≈253 releases (mostly alpha) Aug 3–Oct 7;
  opencode 1.18.16 (Aug 10) → 1.18.35 (Oct 6); Crush 0.88.1 (Aug 7) → 0.97.1 (Sep 29); goose 1.46.0
  (Aug 12) → 1.53.0 (Oct 2); Cline CLI 3.0.51 → 3.0.69.

---

## PART 2 — The deferred candidates, reassessed

Verdict key: **SHIP** (worth doing next), **LATER**, **SKIP**.

| # | Candidate | State of the art (Aug–Oct 2026 evidence) | Verdict |
|---|---|---|---|
| 1 | Tool search / deferred tools | Claude Code: on by default, `auto:N` thresholds, 10k catalog, 5 results, `alwaysLoad` exemption, `isDeferred` for mods ([docs](https://code.claude.com/docs/en/agent-sdk/tool-search)). Also Copilot CLI, pi, Amp (Dec 2025). glass-core already defers MCP past 40 tools | LATER (delta only) |
| 2 | LSP integration | opencode (40+ servers, diagnostics into edits) is still best-in-class ([docs](https://opencode.ai/docs/lsp/)); qwen "experimental LSP support for code intelligence"; Copilot CLI ships `lsp.json` plugins + `copilot lsp list`, warms servers, sandbox-aware; Claude Code has an LSP tool with per-server `requestTimeout` (60s) | SHIP |
| 3 | Repo map / code index | Aider's tree-sitter+PageRank repomap is frozen; **Maki's `index` tool** is the new reference and publishes the accounting: +59 tok/turn, saves 224 tok/turn on reads ([README](https://github.com/tontinton/maki)). Cline `search_codebase`, Copilot "indexed search" | SHIP |
| 4 | Auto-verification after edits | Aider's `--auto-lint`/`--auto-test` unchanged ([docs](https://aider.chat/docs/usage/lint-test.html)); Claude Code now tells Claude to run a project `verify` skill right before commits ([changelog](https://code.claude.com/docs/en/changelog)); Codex hooks run async + can call MCP tools; Droid Droid Shield pre-commit scan | SHIP |
| 5 | Cross-session memory | Claude Code auto memory (format above) is now the de-facto reference; Codex memories; Maki `memory` tool + `/memory` | SHIP |
| 6 | Scheduled / autonomous runs | Claude `/loop` (incl. self-paced dynamic mode, no-prompt default, Loops breakdown in `/usage`); Codex automations + daemon schedules + "saved threads and active goals recover after daemon restarts"; Droid Loop + Missions; Cline `.cline/cron/*.md` task specs; OpenHands automations with cron editing; qwen `/goal` | SHIP |
| 7 | Image / screenshot input | Universal: Claude Code attaches images/PDFs (with downscaling for oversized), Codex generates *and* edits images, Droid resizes screenshots to keep fine detail, pi has vision models | SHIP (small) |
| 8 | Architect/editor split | Aider's `/architect`+`/editor-model` is the canonical version; new evidence: Claude `/advisor` with enforced model *pairings* (Sonnet 5.5 advising Opus 4.7/4.8), Droid Spec Mode runs with its own model, Copilot workflows ask two models and report only on agreement, Amp's Dial picks models per mode | LATER |
| 9 | Browser / computer use | Claude in Chrome (permission-gated, org policy, 90s batches), Codex browser + computer use, Copilot CLI **computer use on desktop apps (Oct 1)**, Amp "Desktop" inside an orb, goose ships web-search/browser-use as built-in skills | SKIP (extension-only, if ever) |
| 10 | Voice | Shipped by default in Codex; qwen Live Voice; Amp voice-to-Puck; Aider `/voice` | SKIP |
| 11 | Session share links | opencode `/share` (opt-in) still the reference; **Amp killed public threads** (Jun 2, "The End of Public Threads") and kept only workspace sharing — evidence the privacy cost wins | SKIP |
| 12 | MCP HTTP/SSE + OAuth | MCP 2026-07-28 makes HTTP the *default* shape (stateless, `server/discover`, header routing, cacheable lists); clients added `/mcp login`, OAuth pre-registration, CIMD, Touch ID verification for MCP requests (Codex) | SHIP |
| 13 | Worktree merge flows | Codex worktrees default-on with ownership + confirmed deletion of clean worktrees; Amp Restack; t3code "choose where new worktrees are created" + per-thread worktrees; GitHub stacked PRs GA | SHIP |
| 14 | Multi-client attach / remote control | Claude Remote Control + attach/respawn + self-hosted runner; Crush `serve` (SSE, live-mirrored, first-wins `--yolo`); Codex app-server + `codex agents`/`queue`; t3code relay + mobile; Amp shared runners + iOS | LATER (security caveat) |
| 15 | Sandbox network restrictions | Now the frontier — Claude per-command domain review + TLS termination + credential masking; Copilot MXC GA covering network, credentials, MCP and language servers | SHIP (top) |
| 16 | Agent teams / multi-session orchestration | Claude agent teams + `SendMessage`/`ListAgents` + `crossSessionInbound`; Codex task @-mentions/`queue`; t3code MCP thread control; Amp Puck | LATER |
| 17 | Spec mode | Droid Spec Mode (plan → approve → implement, spec model separate, proposal download, SDK helper to exit spec mode); GitHub [Spec Kit](https://github.github.com/spec-kit/) popularised spec-as-artifact; Claude plan mode is the minimal version glass-core already has | LATER (small) |
| 18 | Record & replay | Codex record & replay + fork-to-another-model ([docs](https://developers.openai.com/codex/extend/record-and-replay)); nothing new elsewhere in Aug–Oct | LATER |
| 19 | Oracle / second opinion | Amp `oracle` (a different model as a tool, [news](https://ampcode.com/news/oracle)); Claude `/advisor` incl. headless text form + pairing checks; Copilot workflow "two models agree" pattern | SHIP (small) |
| 20 | Plugin/skill marketplaces + interop | **Agent Plugins 1.0 GA**; Claude plugin marketplaces + claude.ai skill sync; Codex remote plugin catalogs; Copilot Awesome Copilot default marketplace; Codex/Copilot/Claude cross-import | SHIP (top) |

Notes where the verdict needs nuance:

- **Tool search is a partial duplicate.** glass-core already defers MCP tools past 40 to
  `mcp_search`/`mcp_call`. The genuine deltas are (a) generalising deferral to *all* extension/mod
  tools, not just MCP, (b) an `auto:N` policy (activate only when deferrable schemas exceed N% of
  the window) instead of a hard tool count, and (c) immutable prefixes: Claude keeps the tool list
  byte-stable so late-connecting tools load deferred instead of rewriting the prompt and invalidating
  cache — directly relevant to glass-core's cache-warmer.
- **Voice is now "real" but not valuable for glass-core.** Codex made it default and added bundled
  audio runtimes; that is decisive evidence it is shipping, not that it is used. A no-tokio Rust TUI
  with minimal dependencies would spend a large amount of surface on capture + STT + device
  selection. Keep not-doing it.
- **Multi-client attach is valuable but currently dangerous for glass-core** because it has no
  permission prompts. A 0600 Unix socket is a sane boundary; a relay/phone client needs an auth and
  revocation story first.
- **Image input is the cheapest real capability gap.** Every competing harness accepts images; a
  bash-only agent cannot see a failing UI. It is core work (message assembly + paste detection) but
  small.

---

## PART 3 — Ranked: what glass-core should ship next

Ranked by user value × fit with a minimal, extension-first, no-tokio Rust harness. Difficulty assumes
blocking I/O and the existing NDJSON extension protocol.

### 1. Egress-restricted bash sandbox (network + credential policy) — core, medium→large

Close the last hole in the write-jail: give the opt-in sandbox a default-deny network policy with an
explicit per-command allowlist, and scrub obvious credentials from the environment it hands to bash.
Ship it as policy, not as a prompt: `[sandbox.network] allow_hosts = ["api.github.com", "*.internal"]`,
per-command widening through the existing `authorize` hook (so an extension can decide, and a strict
mode can refuse), and a loopback proxy for hosts you want to intercept later. Best implementations:
Claude Code's per-command `allowed_domains` reviewed *with the command*, TLS termination and
credential masking (`extract`, `decode: "jwt"`, `awsPairs`/`sigv4`) at
[code.claude.com/docs/en/changelog](https://code.claude.com/docs/en/changelog); Copilot CLI's MXC-based
local sandbox (GA 2026-10-07) which covers network, local networks, git/GitHub credentials and
applies to MCP and language servers too
([changelog](https://github.blog/changelog/2026-10-07-local-sandboxing-for-github-copilot-now-generally-available/)).
Why now: every serious harness shipped this in the last ten weeks, and glass-core's "no permission
prompts by design" stance is only defensible for unattended runs if egress is bounded. It composes
with the Landlock/sandbox-exec write-jail, the read allowlist, and the authorize hook that already
gates tools.

### 2. Agent Plugins 1.0 compatibility + an install path — core loader + extension, small→medium

Teach glass-core to load a portable `plugin.json` package: `skills/` children validated against the
Agent Skills spec, `mcp.json` fed to the existing mcp-bridge, and unknown vendor directories ignored.
Then add the missing half of interop — an installer (`glass plugin add <git-or-zip-source>` with
SHA-256 pinning and the existing content-hash trust gate applied to the installed manifest) and one
shared marketplace index so `glass plugin search` works. Reference implementation and the exact
package layout: [agent-plugins.org](https://agent-plugins.org/plugin-authors/build-an-agent-plugin),
spec at [agentplugins/agent-plugins-spec](https://github.com/agentplugins/agent-plugins-spec), and the
GA announcement covering VS Code/Copilot CLI/SDK
([GitHub changelog](https://github.blog/changelog/2026-08-12-agent-plugins-1-0-in-vs-code-copilot-cli-and-the-copilot-app/)).
Why now: a real cross-vendor standard landed six weeks ago with AWS/Anysphere/Google/Microsoft/OpenAI
/Vercel behind it, and glass-core's skills are already markdown and its MCP bridge already exists, so
compliance is mostly a loader plus a manifest. It composes with skills, AGENTS.md trust gating, the
mcp-bridge, and the extension directory.

### 3. Verification loop on changed files (lint / typecheck / test) — mod + skill, small

After a turn that wrote files, run the project's own verifier on exactly the changed paths, and feed
failures back as a tool result with exit-code-first framing. glass-core already detects what a
command wrote and runs a syntax parser; this is the next step: respect a per-repo `verify` command
(auto-detected from `package.json`/`pyproject.toml`/`Makefile`, overridable in AGENTS.md), debounce
so a multi-edit turn verifies once, and cap output to keep the cache warm. Best implementation:
Aider's `--auto-lint` / `--lint-cmd` / `--test-cmd` semantics and its per-language linters
([docs](https://aider.chat/docs/usage/lint-test.html)); Claude Code moved to instructing the model to
run a project `verify` skill immediately before commits
([changelog](https://code.claude.com/docs/en/changelog)). Why now: it is the highest measured-value
harness-side guardrail in the literature the README already cites, it is cheap, and it is the
difference between "wrote plausible code" and "left the repo green". Composes with post-write
verification, the `turn_end` hook, exit-code-first tool results, and the cache-warmer.

### 4. `index` — a tree-sitter/ctags code skeleton tool — out-of-process extension, medium

A tool that returns a file's high-level skeleton with exact start–end lines per symbol (functions,
classes, exports) and is advertised as "use before read". This is the cheapest large token win for a
bash-first harness: reads dominate a coding agent's token spend, and Maki publishes the accounting
(+59 tok/turn, −224 tok/turn on reads) at [github.com/tontinton/maki](https://github.com/tontinton/maki).
Aider's PageRank repomap remains the reference for *cross-file* ranking
([docs](https://aider.chat/docs/repomap.html)) but is frozen; Maki is the live implementation, and
`ast-grep`/`semble` in its config show the extension path. For glass-core, prefer ctags or an
optional tree-sitter build behind a feature flag to preserve the dependency-minimal core, ship as an
extension (like hashline and todo), and let codemode consume structured output. Why now: the
alternative is paying full-file reads forever; the harness already spills output past 60KB, so it has
the plumbing for one more cheap tool.

### 5. Cross-session memory on `state` — extension, small→medium

An extension that reads and writes a per-repository memory directory with a size-bounded index
(`MEMORY.md`: one line per memory, 200 lines / 25KB loaded each session; topic files read on demand)
and injects the index through the `system_prompt` hook. Mirror Claude Code's format
([docs](https://code.claude.com/docs/en/memory)) rather than inventing one, so memories are readable
and portable, and gate it behind the same trust rules as AGENTS.md. Why now: memory only needs a
file convention and an injection point, glass-core has both (`state`, `system_prompt`), and the
"which skills/notes cost how much context" question now has tooling to answer (`/skill-doctor`,
Codex `/usage`). Composes with `state` (which survives compaction), `/compact`, and session export.

### 6. mcp-bridge v2: HTTP/SSE transport + OAuth, per MCP 2026-07-28 — extension, medium

Extend the stdio bridge with a blocking HTTP client and SSE parsing, `server/discover`, per-request
`_meta` version/capabilities (the new stateless core), Multi Round-Trip Request handling for
`input_required` results, and OAuth with Client ID Metadata Documents plus a local redirect flow.
Deprecation-proofing matters here: Roots/Sampling/Logging are deprecated, `initialize` is gone, and
list results now carry `ttlMs`/`cacheScope`
([spec changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog)). Best current
client behaviour to copy: Crush's OAuth including pre-registered clients, pi's CIMD and project-level
server overrides, Codex's `/mcp login` and `--oauth-client-secret`. Why now: remote MCP is where the
useful servers live (the stdio-only bridge can't reach them), and the spec just changed under
everyone's feet, so doing it once, to the new revision, avoids two migrations. Composes with the
existing namespacing and `mcp_search`/`mcp_call` deferral past 40 tools.

### 7. `/loop` — durable scheduled wakeups and headless runs — core timer + extension, small→medium

A scheduler that can wake a session at a time or interval with a prompt, persist across restarts,
survive compaction, and run headless (`glass run --json` already exists). Add a run ledger so runaway
loops are visible (Claude shows loop run counts, tokens per run and last run in `/usage`), and default
to a bounded number of runs. Best implementation: Claude `/loop` (self-paced dynamic mode, no-prompt
autonomous default) at [code.claude.com/docs/en/changelog](https://code.claude.com/docs/en/changelog);
Codex automations plus daemon schedules that recover saved threads and active goals
([docs](https://developers.openai.com/codex/automations)). Why now: it is the single feature that
turns glass-core from an interactive tool into something that makes progress while you are away, and
the building blocks (serve/attach, `/bg`, notifications, budgets) all exist. Composes with
notifications (you learn when it finishes), budgets/cost display, and the trust gate for unattended
scope.

### 8. LSP diagnostics bridge — out-of-process extension, medium→large

An extension that manages language servers for the workspace and surfaces diagnostics after edits,
injected through the existing `tool_result` rewriting hook so a broken write comes back with the
compiler's error rather than the model discovering it three turns later. Best-in-class is opencode's
LSP integration (40+ servers, diagnostics wired into edits,
[docs](https://opencode.ai/docs/lsp/)); Copilot CLI's `lsp.json` plugin component and `copilot lsp
list` show how to package it; Claude Code's 60s per-server `requestTimeout` is the failure mode to
copy. Extension, not core: it is a capability (the same argument the repo already makes for hashline
and todo), and it keeps the LSP client's async-shaped I/O out of the single-threaded core. Composes
with post-write verification and hashline (diagnostics reference line numbers the model can edit
directly).

### 9. Worktree apply-back / restack — core command + extension, small→medium

Detached worktrees currently strand their work. Add `glass worktree apply <name>` (cherry-pick or
`format-patch`/`am` the subagent's commits, surface conflicts, then remove the worktree) and a
`/restack` that turns a messy series of agent commits into logical ones without touching final file
contents. Best implementations: Amp's Restack ([news](https://ampcode.com/news/restack-your-changes)),
Codex worktrees with ownership and confirmed deletion of clean managed worktrees
([releases](https://github.com/openai/codex/releases)), and GitHub's stacked PRs GA
([changelog](https://github.blog/changelog/2026-10-06-stacked-pull-requests-generally-available)).
Why now: fan-out shipped, so the missing half is getting the work back; without it the parallelism is
a dead end for anyone who wants reviewable history. Composes with the parallel subagent fan-out,
`/git` staging+commit, checkpoints/undo.

### 10. `/second-opinion` — oracle on a different model — extension, small

A command that runs a pinned alternate model over the current diff or question and returns a critique
(or a verdict), using the existing subagent machinery plus a named agent definition bound to the
second model — no new core concepts. Best implementations: Amp's `oracle`
([news](https://ampcode.com/news/oracle)) and Claude's `/advisor` including the headless text form and
enforced pairing checks ([changelog](https://code.claude.com/docs/en/changelog)); Copilot's workflow
pattern of reporting only when two models agree is a nice variant. Why now: it is nearly free given
run_subagent + agent definitions + fallback chains, and it converts a naive self-review into an
adversarial one. Composes with agent definitions, model fallback chains, budgets (the cost of a
second pass is visible), and `/git` for diff selection.

**Honourable mentions (rank 11–13):** image/paste input (core, small–medium — the cheapest real
capability gap); multi-client attach with an auth story (core, medium — worth doing only after an
auth/revocation design, since glass-core has no permission prompts); a session inbox so two
glass-core sessions can message each other (extension, small — Claude and Codex both now ship this).

---

## PART 4 — Duplicates and hype

**Would duplicate what glass-core already has — do not re-ship:**
- Tool search / deferred loading as a *new* feature: the MCP deferral past 40 tools covers the common
  case. Only the generalisation (all extension tools), `auto:N` policy and byte-stable tool list are
  new.
- Plan mode + a read-only inspect tool covers most of "spec mode"; a spec workflow is a **skill**
  (write the plan to a file, iterate) not a new mode.
- Subagent fan-out, named agent definitions, worktree isolation — already shipped; #9 is the missing
  return path, not the parallel machinery.
- Turn-completion notifications, budgets/cost, checkpoints/undo, session search/branching, export to
  Markdown, `/btw`, `/init`, prompt caching + cache warmer, output spill-to-disk, ACP server,
  serve/attach, MCP stdio bridge, todo list, hashline, codemode, web fetch/search — all present and
  still at parity with the field.
- Prompt-cache-friendly practices: glass-core is already ahead (three breakpoints, warm policy). The
  only new trick worth stealing is the byte-stable tool list from Claude Code (2.1.268).

**Overhyped — skip:**
- **Computer use as a headline.** Now shipped by Claude, Codex and Copilot CLI, and it is still
  flaky and token-hungry; wrap someone else's driver in an extension if a user asks.
- **Voice.** Codex enables it by default; that is ubiquity, not evidence of value for a minimal TUI.
- **Public session share links.** opencode keeps them opt-in for a reason, and Amp ended public
  threads entirely in June 2026. Local Markdown/HTML export covers the real use.
- **Parallel-session dashboards / Kanban / desktop control planes.** Real products, wrong layer —
  expose the attach protocol and let them build.
- **Cloud orbs / hosted agent VMs / Droid Computers.** Infrastructure businesses, not features.
- **MCP everywhere.** Still true that a bash-first agent reaches most APIs directly; the case for MCP
  is remote servers and auth isolation, which is exactly what #6 addresses. Do not grow the tool
  surface.
- **"Self-improving" / continual-harness claims.** Prime Agent is the most interesting research
  direction and the least verifiable; do memories (#5) and manual skill authoring first.
- **Benchmark leaderboards as procurement signals.** Still not reproducible from primary sources.
- **Agent teams as ceremony.** Value remains fan-out + isolation, not the team metaphor.

**Unverified in this pass (flagged for honesty):** a newsletter headline suggesting a large Cursor
acquisition closed — **UNVERIFIED**, not pursued; a "DeepSeek Harness" MIT agent framework — real
enough to appear in multiple secondary sources but I found no primary repository — **UNVERIFIED**;
star counts and "trending" signals are directional, not a census.

---

## PART 5 — Method and limits

- Primary sources only for factual claims: vendor changelogs (`code.claude.com/docs/en/changelog`,
  `opencode.ai/changelog`, `ampcode.com/news`), GitHub release bodies fetched via the API for Codex,
  Copilot CLI, Cline, qwen-code, t3code, Crush, goose, OpenHands, pi, and the GitHub blog for
  Agent Plugins / dynamic workflows / sandboxing / local models; protocol site for MCP 2026-07-28;
  official docs for tool search, memory, LSP, lint/test, worktrees, automations.
- No access to closed-source internals; behaviour is inferred from published notes.
- Claude Code version→date mapping is from npm publish timestamps, which can lead the changelog date
  by a day.
- The window is ~10 weeks; the previous survey covered through early October, so some entries above
  restate things it already noted (marked as such where it matters).
