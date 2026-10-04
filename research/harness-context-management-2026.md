# Context Window Management in AI Coding Harnesses — State of the Art, October 2026

Research compiled from primary sources: vendor documentation, live source code, and GitHub issues.
Every factual claim below is cited. Anything I could not verify is explicitly marked **UNVERIFIED**.

**Important framing note on model versions:** As of October 2026 the frontier model line has moved
past the Claude Opus 4.x / GPT-5.x / Gemini 3.x naming in the original request. Current Anthropic
aliases resolve to Opus 5.5 and Sonnet 5.5 (native 1M window), plus a "Fable" family
(Fable 5.1 / Fable 5) for long-horizon work; Google model constants include `gemini-3-pro-preview`
and `gemini-3.1-pro-preview`. I report what the docs actually say as of today rather than the
4.x/5.x-era numbers in the request.

---

## PART 1 — HARNESS-BY-HARNESS COMPARISON

### 1. Claude Code (Anthropic CLI)

| Field | Finding |
|---|---|
| **Compaction strategy (one line)** | Replaces conversation with an LLM-generated structured summary, then re-injects a curated subset of durable state from disk and up to 5 recently-touched files. |
| **Threshold** | Configurable window, not a percentage. Default: compacts "when the conversation reaches the model's context limit," except models with a **native 1M window, which auto-compact at ~967K tokens**. `/autocompact <100k–1M>`, `--autocompact`, `autoCompactWindow` setting, or `CLAUDE_CODE_AUTO_COMPACT_WINDOW` env var (takes precedence over all). |
| **Model-driven?** | **Yes.** LLM writes the summary. As of v2.1.198 the summarization request inherits the session's extended-thinking config. |
| **Notable mechanism** | **Most sophisticated survival contract of any harness.** After compaction: project-root CLAUDE.md and unscoped rules re-injected from disk; auto memory re-injected; fresh git status snapshot; plan-mode plan re-injected from disk; up to **5** recently-modified files re-read (files >5,000 tokens return as a *path reference*, not content); invoked skill bodies re-injected capped at 5,000 tokens/skill and 25,000 total, oldest dropped first; running background commands/subagents kept alive and re-announced; `SessionStart` hooks matching the `compact` source fire and re-add output. Path-scoped rules and nested CLAUDE.md are *summarized away*. |
| **Sliding window / eviction** | No classic sliding window. Two-stage fallback: if a model must be swapped during compaction, Claude Code refuses to fall back to a **smaller** context window. Separately, Anthropic exposes server-side **`clear_tool_uses_20250919`** (clears oldest tool results in place, placeholder text left behind; default trigger 100,000 input tokens, `keep` 3 tool uses, optional `clear_at_least`, `exclude_tools`, `clear_tool_inputs`) and **`clear_thinking_20251015`**. |
| **Token counting** | Mix of API-reported usage and local estimation; `CLAUDE_CODE_MAX_CONTEXT_TOKENS` lets you declare the window for unrecognized/gateway model IDs. `count_tokens` endpoint available client-side. |
| **Persistent on-disk state** | **Yes — the richest.** CLAUDE.md hierarchy, auto memory, plan-mode plan file, plan/todo tracking, AGENTS.md support, hooks (`PreCompact`/`PostCompact`, `SessionStart` w/ `compact` source). |
| **System prompt approach for long-horizon tracking** | Anthropic's engineering guidance: compaction + structured note-taking + sub-agents are the three levers. Subagent returns "only the summary and a small metadata trailer"; README quotes "often 1,000–2,000 tokens." |
| **Published cost/quality measurements** | Anthropic says compaction should enable "minimal performance degradation" and recommends tuning prompts "start by maximizing recall… then iterate to improve precision." **I found no published token-cost or quality benchmark for Claude Code's own compaction.** UNVERIFIED. |

**Context editing (API level, relevant because Claude Code is the reference implementation):**
Anthropic's docs now state plainly that **server-side compaction is the primary strategy** and that
context editing is for specific fine-grained cases. Three distinct server-side mechanisms exist:
`compact-2026-09-04` (on-demand), threshold compaction, and `context-management-2025-06-27`
(context editing). Client-side SDK compaction is **deprecated** in TS/Ruby SDKs
(`compaction_control` deprecated, removed in Python v1.0); Anthropic explicitly recommends against it.

Source: [Explore the context window](https://code.claude.com/docs/en/context-window),
[Model configuration § Default auto-compact thresholds](https://code.claude.com/docs/en/model-config),
[Context editing](https://platform.claude.com/docs/en/build-with-claude/context-editing),
[Compaction overview](https://platform.claude.com/docs/en/build-with-claude/compaction),
[Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)

---

### 2. Codex CLI (OpenAI)

| Field | Finding |
|---|---|
| **Compaction strategy** | Full history **replacement**, not append. Keeps recent user messages (up to 20,000 tokens) + a summary stored as a **user-role message**. Two paths: remote (`POST /v1/responses/compact`, returns opaque encrypted items) for OpenAI providers; local (inline summarization turn) for everyone else. |
| **Threshold** | `auto_compact_token_limit()` = **`(context_window * 9) / 10`** — i.e. 90% — clamped so a user-set `model_auto_compact_token_limit` can only **lower** it, never raise it. If neither `context_window` nor `model_auto_compact_token_limit` is set, the limit is `None` and the caller falls back to `i64::MAX`, meaning **compaction never fires automatically**. |
| **Model-driven?** | **Yes** locally (same model, same endpoint, `SUMMARIZATION_PROMPT` appended as a user turn). Remotely, the server decides. |
| **Trigger points** | **Pre-turn** (before a new user turn; `InitialContextInjection::DoNotInject`, so the next turn re-injects all context fresh) and **mid-turn** (after a sampling response, when `token_limit_reached && needs_follow_up`; uses `BeforeLastUserMessage` so the model still sees the pending user request last). |
| **Notable mechanism** | `SUMMARY_PREFIX` = *"Another language model started to solve this problem and produced a summary of its thinking process…"*, stored as a user message and used to *exclude* prior summaries from the next compaction's user-message collection. Preserves `GhostSnapshot` items out-of-band so `/undo` still works post-compaction. If the compaction request itself overflows, it iteratively does `history.remove_first_item()` and retries. `PreCompact`/`PostCompact` hooks can cancel compaction entirely. |
| **Persistent state** | `AGENTS.md` (32 KiB limit, per the community setup guide), rollouts on disk under `~/.codex/sessions/`, hooks. |
| **Published cost/quality** | **None found.** UNVERIFIED. |
| **User-facing control complaints** | `model_auto_compact_token_limit` can be silently clamped; **profile-scoped values are reportedly ignored**; the only way to disable auto-compaction is the `--disable auto_compaction` flag — no config key exists (third-party sites; issue #4106 open). |

Sources: [compact.rs (source)](https://github.com/openai/codex/blob/main/codex-rs/core/src/compact.rs),
[Deep analysis of compaction prompt + thresholds](https://gist.github.com/sam-saffron-jarvis/30403c1bc5682bf9f69fa00933aad815),
[Context compaction deep dive](https://codex.danielvaughan.com/2026/04/14/context-compaction-deep-dive-codex-cli-claude-code-opencode/)

---

### 3. Aider (Paul Gauthier)

| Field | Finding |
|---|---|
| **Compaction strategy** | `ChatSummary` class (`aider/history.py`) auto-summarizes older history. Recursive **head/tail split**: keep the tail that fits `max_tokens/2`, summarize the head, and if summary+tail still exceeds, recurse (depth cap 3). Falls back to `summarize_all()` when history is short or split is impossible. |
| **Threshold** | `max_chat_history_tokens`, documented as `min(max_input_tokens / 16, 8192)`, floored at 1024. So **8192 tokens for a 128K model**, 1024 for an 8K model. |
| **Model-driven?** | **Yes** — `model.simple_send_with_retries()` with `prompts.summarize`. |
| **Notables** | `/drop` (remove files from the chat to free context), `/clear`, `/reset` (drop files + clear history), `/tokens` (report context tokens), `/copy-context`, `/context`, `/map`. **Architect/editor two-model split** (`/architect`, `/editor-model`, `/weak-model`) reduces context by having a cheaper/weak model plan and a strong model execute. **Repo map** (`repomap`) uses tree-sitter + PageRank over a dependency graph to pick the most relevant slices of a repo map that would otherwise not fit. |
| **Token counting** | Real tokenizer via LiteLLM, with `model_prices_and_context_window.json` (cached, 24h TTL) for window sizes and costs. This is the most exact-tokenizer-happy design of the bunch. |
| **Persistent state** | Git-based: edits are committed, so "state" is the repo, not a side file. |
| **Published cost/quality** | Aider's own release notes mention "Optimized chat history summarization performance." No quality measurement. UNVERIFIED. |
| **Notable complaint** | **The threshold heuristic is decoupled from the real context window.** On a 1M-window model, `max_chat_history_tokens` stays at 8192 — so the "compact early" heuristic is actually conservative there, but the reported failure is the opposite: users hit *"messages exceed model context window"* on long refactors because the heuristic "kicks in late" on models where `/16` gives a large number. User issue #3493: *"Max chat history tokens not respected."* |

Sources: [In-chat commands](https://aider.chat/docs/usage/commands.html),
[history.py (source)](https://github.com/Aider-AI/aider/blob/main/aider/history.py),
[Repository map](https://aider.chat/docs/repomap.html),
[Issue #3493](https://github.com/Aider-AI/aider/issues/3493)

---

### 4. Cline (VS Code extension)

| Field | Finding |
|---|---|
| **Compaction strategy** | "Auto Compact" summarizes the conversation and replaces history. Ships as a visible summarization **tool call**. |
| **Threshold** | **~80% of the context window with a 40K buffer**, derived from the model's context window — **not** an absolute budget. |
| **Model-driven?** | **Yes**, but with a critical caveat: *"With other models, Cline falls back to standard rule-based context truncation, even if Auto Compact is enabled."* So for non-primary providers it is **deterministic truncation**. |
| **Cost note** | *"Summarization leverages your existing prompt cache from the conversation, so it costs about the same as any other tool call."* |
| **Restoration** | Checkpoints + editing a pre-summarization message both restore pre-summarization state. |
| **Persistent state** | Focus Chain (todo list persists through summarization). |
| **Published cost/quality** | None found. UNVERIFIED. |
| **Known complaints** | **Threshold is percentage-of-window with no user override.** A user moving from a 200K to a 1M window saw the trigger move silently from ~150K to ~800K with no config change, a 12% cache hit rate, 24.1M prompt tokens, and **$49.63 on a single task** — discovered only via the Anthropic bill. Issue #14329 (closed) names Claude Code's `CLAUDE_CODE_AUTO_COMPACT_WINDOW` as the missing feature; the original request (discussion #3248) is **still open**. |

Sources: [Auto Compact docs](https://docs.cline.bot/features/auto-compact),
[Issue #14329](https://github.com/cline/cline/issues/14329),
[Discussion #3248](https://github.com/cline/cline/discussions/3248)

---

### 5. Roo Code (VS Code extension)

| Field | Finding |
|---|---|
| **Compaction strategy** | "Intelligent Context Condensing" (on by default): LLM summarizes earlier conversation; condensed version is what continues to be sent. Original messages remain recoverable via Checkpoints. |
| **Threshold** | **Percentage slider, default 100%.** Reserve policy: **30% of the window reserved by default (20% model output + 10% safety buffer), leaving 70% for history.** |
| **Model-driven?** | Yes — and notably **it deliberately refuses a separate condensing model**: "Condensing always uses your active conversation provider/model," because a different model "can degrade summary quality… avoiding 'translation' errors between different tool/format expectations." |
| **Notable mechanism** | **Deterministic error recovery: on a context-window error from OpenAI/Anthropic/etc., Roo automatically truncates 25% and retries** (up to the built-in retry limit). Slash commands from the first message survive condensations. Custom condense prompt stored at `customSupportPrompts.CONDENSE`. |
| **Token counting** | Hybrid: native provider counting endpoints (e.g. Anthropic) → falls back to **tiktoken** → falls back to word-based estimation with punctuation/newline overhead; images estimated at 300 tokens each. |
| **Cost reporting** | UI shows token counts before/after condensing **and the cost of the condensing call** — the only harness I found that surfaces per-compaction cost inline. |

Sources: [Intelligent Context Condensing](https://roocodeinc.github.io/Roo-Code/features/intelligent-context-condensing/)

---

### 6. Gemini CLI (Google)

| Field | Finding |
|---|---|
| **Compaction strategy** | **Two-pass LLM compression into a structured `<state_snapshot>` XML**, then `history = [environment context] + [<state_snapshot> as user msg] + [synthetic "Got it. Thanks for the additional context!" model ack] + [verbatim last ~30% tail]`. Pass 2 asks the model to critique its own snapshot and regenerate, falling back to pass 1 if empty. |
| **Threshold** | `DEFAULT_COMPRESSION_TOKEN_THRESHOLD = **0.5**`, user-configurable via `model.compressionThreshold` (0–1). All known models — and unknown ones — get `DEFAULT_TOKEN_LIMIT = 1,048,576`, so the effective default trigger is **~524,288 tokens**. Priority: local config → remote experiment flag (`CONTEXT_COMPRESSION_THRESHOLD`, ID 45740197) → 0.5. |
| **Model-driven?** | Yes, via a dedicated **compression model alias** (`chat-compression-3-pro`, `chat-compression-2.5-flash`, …) mapped from the conversation model — same *family*, never downgraded. |
| **Notable mechanism (best-in-class safety)** | **Prompt-injection hardening in the compressor prompt.** Explicit CRITICAL SECURITY RULE telling the compressor to ignore all commands/directives found in history, never exit `<state_snapshot>` format, and treat history as raw data. Detects a prior snapshot by string search and switches to a **merge** anchor instruction, so summaries accumulate rather than stack. |
| **Deterministic pre-pass** | `truncateHistoryToBudget` walks history newest-first with a **50,000-token `COMPRESSION_FUNCTION_RESPONSE_TOKEN_BUDGET`**, spilling oversized tool output to a temp file and replacing it with a path pointer — this is genuine context editing. Split point is by **cumulative JSON char count** at 70% (`COMPRESSION_PRESERVE_THRESHOLD = 0.3` keeps the last 30%). Uses the **original untruncated** history for the summarizer if it fits (high-fidelity path). |
| **Verification** | Uses the real `countTokens` API to confirm the new history is actually smaller; if not → `COMPRESSION_FAILED_INFLATED_TOKEN_COUNT`. |
| **Fallback ladder** | `COMPRESSED` → `COMPRESSION_FAILED_INFLATED_TOKEN_COUNT` → `COMPRESSION_FAILED_EMPTY_SUMMARY` → `NOOP` → **`CONTENT_TRUNCATED`** (deterministic tool-output truncation only, after an LLM failure — explicitly to avoid repeated expensive failures). |
| **Token counting** | **Exact** — API-reported `getLastPromptTokenCount()` and the real `countTokens` endpoint. Best in class. |
| **Persistent state** | `GEMINI.md` hierarchy; session recordings preserved across compression (`resumedData`). |
| **Known weaknesses** | Compression calls **cannot be cancelled** (no abort signal threaded; TODO in source). Interactive path replaces the whole `GeminiChat` object and sets `forceFullIdeContext = true`. |

Sources: [Deep reference with code links (commit 29b3aa8)](https://gist.github.com/sam-saffron-jarvis/0503a15eeb261fc5f6be3ba253e1e645),
[Configuration docs](https://google-gemini.github.io/gemini-cli/docs/get-started/configuration.html),
[DeepWiki: Chat Compression and Context Management](https://deepwiki.com/google-gemini/gemini-cli/4.12-chat-compression-and-context-management)

---

### 7. GitHub Copilot CLI

| Field | Finding |
|---|---|
| **Compaction strategy** | Sends the full conversation to the model with a special structured-summary prompt (goals, what was done, key technical details, important files, planned next steps), then replaces history with the summary **plus original user instructions and current plan/to-do state**. Messages added *during* background compaction are kept. |
| **Threshold** | **~80% of capacity**, leaving a ~20% buffer so tool calls continue while compaction runs in the background. At **~95%** it *pauses and waits* for compaction to finish. Manual `/compact` (Esc cancels); `/clear` wipes. |
| **Model-driven?** | Yes. |
| **Deterministic tool-output gating** | **Tool output >20 KiB is spilled to a temp file by default**; the model gets the path + preview. Tunable via `COPILOT_LARGE_OUTPUT_THRESHOLD_BYTES`. Applies to MCP tools too. |
| **Persistent state (notable)** | **Every compaction writes a numbered, titled checkpoint file into the session workspace.** Retrievable with `/session checkpoints` / `/session checkpoints N`. This is the best *auditability* story in the comparison — you can read back what was preserved. |
| **Token counting** | Exact; `/context` breaks usage down by System Prompt / Custom Instructions / System Tools / MCP Tools / Messages / Free Space / Buffer. |
| **Documented self-assessment** | GitHub is explicit that compaction is lossy: *"the exact wording of every message, the full output of every command, or minor decisions made early… may not be included,"* and that compaction **cannot be reversed** (though `/resume` restores prior sessions). |
| **Published cost/quality** | None found. UNVERIFIED. |

Source: [Managing context in GitHub Copilot CLI](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/context-management)

---

### 8. Crush (Charm / charmbracelet)

| Field | Finding |
|---|---|
| **Compaction strategy** | `StopWhen` predicate halts the agent loop when remaining context ≤ threshold, then calls `Summarize()` — a full LLM summarization turn over session history, stored as an `IsSummaryMessage` message; `getSessionMessages` then reads only from `SummaryMessageID` forward. If tool calls were pending, the turn is **re-queued** with the prompt rewritten: *"The previous session was interrupted because it got too long, the initial user request was: `…`"*. |
| **Threshold** | **Hardcoded constants in `internal/agent/agent.go`**: `largeContextWindowThreshold = 200_000`; `largeContextWindowBuffer = 20_000`; `smallContextWindowRatio = 0.2`. So **>200K window → summarize when 20,000 tokens remain (≈98%); ≤200K → when 20% remains (≈80%)**. |
| **Model-driven?** | Yes — the **large model** produces the summary (`summaryPrompt` template). |
| **Persistent state** | **Yes, and it's the best integrated:** Crush has a real **`todos` tool**, persists `session.Todos`, and `buildSummaryPrompt()` injects the todo list into the compaction prompt with: *"Include these tasks and their statuses in your summary. Instruct the resuming assistant to use the `todos` tool to continue tracking progress."* It also injects a `<system_reminder>` when the todo list is empty. |
| **Token counting** | Provider-reported usage accumulated across `InputTokens + CacheReadTokens + CacheCreationTokens`; falls back to `approxTokenCount` only when usage is zero. |
| **Notables** | **Loop detection** as a second stop condition; sessions/messages in SQLite. `disable_auto_summarize` is all-or-nothing. Two open issues (#3564, #3620) request a configurable ratio. |
| **Known complaints** | **#3620 is the best-documented hard-stop failure in this whole report** (details in Part 3). Also: the summary is display-only, with no way to copy it into a new session (#3599). |

Sources: [agent.go (source)](https://github.com/charmbracelet/crush/blob/main/internal/agent/agent.go),
[Issue #3564](https://github.com/charmbracelet/crush/issues/3564),
[Issue #3620](https://github.com/charmbracelet/crush/issues/3620)

---

### 9. Goose (Block)

| Field | Finding |
|---|---|
| **Compaction strategy** | Documented as a **two-tiered** design: (1) **Auto-Compaction** (LLM summary via the editable `compaction.md` prompt template), (2) **Context Strategies** as the fallback when the limit is still exceeded. |
| **Threshold** | **80% of the token limit** by default in both Desktop and CLI; `GOOSE_AUTO_COMPACT_THRESHOLD` overrides (e.g. `0.6`); **`0.0` disables it**. |
| **Model-driven?** | Yes. Customizable prompt template (`compaction.md`) — user-editable, which almost no other harness offers. |
| **Distinctive: tool-output summarization** | **Goose summarizes older tool call outputs in the background while keeping recent calls in full.** The cutoff is computed from the model context limit *and* the auto-compaction threshold; tunable via `GOOSE_TOOL_CALL_CUTOFF`. This is background/off-path context editing, not just compaction. |
| **Sliding window / eviction** | **Yes — four selectable strategies.** `GOOSE_CONTEXT_STRATEGY` = `summarize` \| `truncate` \| `clear` \| `prompt`. Default in interactive CLI = `prompt` (interactive chooser); headless `goose run` = automatic `summarize`. Desktop = summarization only. Truncation and clear are **CLI-only**. |
| **Persistent state** | `config.yaml`, recipes, session state. |
| **Token counting** | Auto-detected from model name with 5-level precedence (env `GOOSE_CONTEXT_LIMIT` → declarative provider config → provider runtime discovery → canonical metadata → **global default 128,000**). Important caveat the docs state themselves: `GOOSE_CONTEXT_LIMIT` *"only affects the displayed token usage and progress indicators. Actual context management is handled by your LLM."* |
| **Notables** | **Max Turns** (default 1000) — a separate hard-stop guardrail: stops after N consecutive turns without user input and asks whether to continue. UI: green <50%, yellow 50–85%, red >85%. |
| **Project status** | Goose has **moved to the Agentic AI Foundation (AAIF)**, April 2026. |

Source: [Smart Context Management](https://goose-docs.ai/docs/guides/sessions/smart-context-management/)

---

### 10. OpenHands (formerly OpenDevin) — software-agent-sdk

| Field | Finding |
|---|---|
| **Compaction strategy** | A pluggable **Condenser** subsystem (`CondenserBase` → `RollingCondenser` → `LLMSummarizingCondenser`, plus `NoOpCondenser` and `PipelineCondenser` for multi-stage). Replaces the middle of history with a `Condensation` event carrying `forgotten_event_ids`, `summary`, `summary_offset`. |
| **Threshold** | **Event-count based, not token-based**: `max_size` (default **120**), with `keep_first` (default 4) always preserved. Post-condensation target is `max_size // 2` (default 60). |
| **Model-driven?** | Yes, and **the summarizing LLM is explicitly a separate instance** — `llm=llm.model_copy(update={"usage_id": "condenser"})`, "often cheaper model than reasoning LLM." This is the opposite of Roo Code's deliberate decision. |
| **Sliding window** | **Yes, and it's the purest rolling window of the group**: keep head (`keep_first`), keep tail (`target_size - keep_first - 1`), summarize the middle. |
| **Manual trigger** | Agent can emit a `CondensationRequest` event on hitting an LLM context-window error — i.e. **reactive recovery on overflow**, in addition to proactive thresholding. |
| **Token counting** | Event-count proxy rather than true tokens. |
| **Persistent state** | Append-only event log (`persistence_dir`), plus the `TaskTrackerTool` for structured todo tracking. |
| **Published cost/quality** | **Yes — the only harness with vendor-published measurements.** Claims **up to 2x reduction in per-turn API cost**, consistent response times in long sessions, and **equivalent or better performance** on software-engineering tasks vs. un-condensed. Links to an OpenHands blog post on context condensation. I did not fetch the benchmark methodology, so **the rigor of the "equivalent or better" claim is UNVERIFIED.** |
| **Notables** | Condenser returns a `Condensation` rather than a `View`, so the agent **returns early from the step** and re-derives the view on the next step — a clean two-phase design. |

Sources: [Condenser architecture](https://docs.openhands.dev/sdk/arch/condenser),
[Context Condenser guide](https://docs.openhands.dev/sdk/guides/context-condenser)

---

### 11. opencode (sst)

| Field | Finding |
|---|---|
| **Compaction strategy** | `[system prompt][older conversation] → [system prompt][summary][recent 15k][pending work]`. **"Later compactions update the same summary rather than starting over."** |
| **Threshold** | `compaction.buffer`, default **10% of the limit**; `compaction.keep.tokens` default **15000**; `compaction.auto` default true. Also **recovers once** when a provider rejects a request as too long (retried exactly once; a second rejection is returned as an error). |
| **Model-driven?** | Yes, using the **session's model** — "There is no separate compaction model." |
| **Distinctive: native/encrypted compaction** | Per-provider `settings.compaction.type` = `summary` (OpenCode's own) or **`native`** (OpenAI Responses models): sends history to the provider's compaction endpoint, which returns an **opaque encrypted checkpoint item**. Recent messages up to `keep.tokens` are kept alongside. **Portability caveat: an encrypted checkpoint only works with the same provider, model, and endpoint** — switching models reverts to the original conversation. |
| **Tool-output pruning** | In the retained recent window, long tool output is shortened and attachment data is replaced by a short description. (An older revision had a separate backward-scan prune protecting the last 40k tokens of tool output with a 20k minimum prunable — `PRUNE_PROTECT` / `PRUNE_MINIMUM`. Treat the exact constants as **UNVERIFIED for current versions**.) |
| **Structured summary format** | `## Objective` / `## Next Move` etc., "structured so another agent could pick the work up from it." |
| **Persistent state** | Instruction files (AGENTS.md-style) become the **session baseline after compaction** — so fresh versions are picked up, not stale ones. |
| **Known complaints** | Issue #11314: hardcoded 75% threshold (older v1 behavior) causes "severe performance degradation for long-context models… before compaction occurs." v2 exposes config; docs explicitly warn "**Compaction is lossy.**" |

Sources: [Compaction | OpenCode](https://opencode.ai/v2/docs/compaction),
[Issue #11314](https://github.com/anomalyco/opencode/issues/11314),
[Cross-harness research gist (v1 internals)](https://gist.github.com/badlogic/cd2ef65b0697c4dbe2d13fbecb0a0a5f)

---

### 12. qwen-code (Alibaba / QwenLM)

| Field | Finding |
|---|---|
| **Pre-redesign behavior (verified)** | Single ratio `COMPRESSION_TOKEN_THRESHOLD = 0.7` for all window sizes; single-failure permanent lock (`hasFailedCompressionAttempt = true` disables auto-compact for the whole session); tip system hardcoded at 50/80/95% and decoupled from the 70% threshold. |
| **Redesign (design doc, status Draft, 2026-05-14)** | Three-tier ladder replicating Claude Code's absolute-token approach: `auto_threshold = max(0.7 × window, effectiveWindow − 13_000)`, `warn = auto − 20_000`, `hard = effectiveWindow − 3_000`, with `SUMMARY_RESERVE = 20_000`. Measured results: 1M → warn 947K / auto 967K / hard 977K; 200K → 147K/167K/177K; 32K → ratio fallback only. |
| **Notable mechanism** | Three fixes worth stealing: (a) **circuit breaker** — 3 consecutive failures instead of 1 permanent lock; (b) **disable thinking + hard `maxOutputTokens = 20_000`** on the compression call so the buffer math is exact; (c) **token estimation compensation** (`bytesPerToken = 4` text / `2` JSON) to fix two systematic biases in `lastPromptTokenCount`: it lags one turn (excludes the incoming user message) and is **0 on the first turn**, so `--continue` resume and subagent-inherited history always bypass every threshold on first send. |
| **Instructive design note** | Explicitly justifies its constants as *"全部沿用 claude-code 的实测值"* — all taken from Claude Code's measured values. Strong evidence of cross-harness convergence. |

Source: [auto-compaction-threshold-redesign.md](https://github.com/QwenLM/qwen-code/blob/main/docs/design/auto-compaction-threshold-redesign.md)

---

### 13. Amp (Sourcegraph)

| Field | Finding |
|---|---|
| **Compaction strategy** | **Deliberately no automatic compaction.** Manual, model-driven **Handoff**: you state a goal, *"another model then analyzes the thread you're handing off from, to extract relevant information,"* drafting a message for a fresh thread. |
| **Threshold** | **None.** No auto-compact. |
| **Distinctive primitives** | **Fork** (duplicate context at a point), **Edit/Restore** previous messages (resetting the thread so the edited message becomes last), **Thread References** (`@@`, `read_thread` tool — a second model extracts *only what's relevant to your prompt* from another thread rather than importing it wholesale). |
| **Token counting** | Truncates @-mentioned files at **500 lines / 2 KB per line**. |
| **Stated philosophy (very quotable)** | *"Everything in the context window has an influence on the output… the conversation **is** the outcome."* And: models go *"off the rails: hallucinating things that don't exist, failing to do the same things over and over again, declaring victory while standing on a mountain of glass shards."* |
| **Caveat** | **The guide is explicitly marked Archived** — "written in November 2025 for an earlier model and context-management era. Since then, models have become a lot more forgiving." I could not verify whether Amp has since added auto-compaction. **UNVERIFIED.** |

Source: [Context Management in Amp](https://ampcode.com/guides/context-management)

---

### 14. pi (minimal TypeScript harness, `@earendil-works/pi-coding-agent`)

This is the most rigorously specified minimal harness I found, and the best documented on edge cases.

| Field | Finding |
|---|---|
| **Compaction strategy** | Walks **backwards** accumulating token estimates to `keepRecentTokens` (default **20,000**), summarizes everything before the cut point, appends a `CompactionEntry` with `firstKeptEntryId`, and rebuilds context as `[summary][messages from firstKeptEntryId]`. |
| **Threshold** | `contextTokens > contextWindow − reserveTokens`, `reserveTokens` default **16,384**. Configurable globally **and per-model** via `compaction.modelOverrides` — e.g. `reserveTokens: 400000` triggers above 600K on a 1M model. |
| **Model-driven?** | Yes. **Summarization requests disable prompt-cache writes**, since these are one-off prompts unlikely to be reused — a nice cost detail most harnesses ignore. |
| **Best-in-class: repeated-compaction correctness** | "On repeated compactions, the summarized span starts at the previous compaction's **kept boundary** (`firstKeptEntryId`), **not at the compaction entry itself**." This deliberately re-summarizes messages that survived the previous compaction, so information isn't stranded in a kept-but-never-re-summarized zone. |
| **Best-in-class: cut-point validity** | Cut points may only be user messages, assistant messages, bash-execution messages, or custom messages. **"Never cut at tool results (they must stay with their tool call)."** Handles "split user-message spans" (a single span exceeding the budget) by generating **two summaries and merging them**. |
| **Overflow recovery** | A provider context-overflow error or early `stopReason: "length"` can select **one** compact-and-retry. Ordering is precisely specified: persist final response → `turn_end` → `agent_end` → append context-edit omissions → compaction → fresh retry. If recovery compaction fails, omission edits are kept, no compaction appended, no retry scheduled. |
| **Deterministic pre-processing** | `serializeConversation()` flattens messages to text (with `[User]:`, `[Assistant tool calls]: read(path=…)` labels) to stop the model treating it as a conversation to continue. **Tool results truncated to 2,000 characters** during serialization. |
| **Persistent state** | Append-only session log with typed entries; compaction is *recorded*, not destructive. Summary format: Goal / Constraints & Preferences / Progress (Done, In Progress, Blocked) / Key Decisions with rationale / Next Steps / Critical Context, plus **`<read-files>` and `<modified-files>` tags accumulated across compactions**. |
| **Second mechanism** | **Branch summarization** on `/tree` navigation — summarizes the abandoned branch at the common ancestor and injects it into the new branch. |
| **Extensibility** | `session_before_compact` hook can **cancel** compaction or supply a custom summary (e.g. generated by a different model); `session_compact_failed` for telemetry. |

Source: [Compaction Reference · Pi](https://pi.dev/docs/latest/compaction)

---

### Quick-reference table

| Harness | Auto? | Threshold | Model-driven | On-disk state re-injected | Hardest failure mode |
|---|---|---|---|---|---|
| Claude Code | Yes | ~967K (1M models) / model's limit; configurable 100K–1M | Yes | CLAUDE.md, memory, plan, 5 files, skills, hooks | Cumulative loss over many compactions |
| Codex CLI | Yes | 90% of window, clamped down only | Yes (+ server-side remote) | AGENTS.md, hooks, rollouts | Remote endpoint failure → unrecoverable session |
| Aider | Yes | `min(max_in/16, 8192)` | Yes | Git history | Heuristic decoupled from real window |
| Cline | Yes | ~80% of window (40K buffer) | Yes, but non-primary models → truncation | Focus Chain todo list | Percentage threshold silently scales with window |
| Roo Code | Yes | 100% default slider; 30% reserved | Yes — **same model by design** | Task state | Default 100% = compacts at the ceiling |
| Gemini CLI | Yes | **50%** of 1,048,576 ≈ 524K | Yes — dedicated `chat-compression-*` alias | GEMINI.md, session recordings | Uncancellable compression calls |
| Copilot CLI | Yes | ~80%, pause at ~95% | Yes | **Checkpoint files per compaction** | Lossy & irreversible, but auditable |
| Crush | Yes | 20K remaining (>200K) / 20% (≤200K) | Yes — large model | **Todos tool injected into compaction prompt** | One turn can overshoot → permanent death |
| Goose | Yes | 80%; `GOOSE_AUTO_COMPACT_THRESHOLD` | Yes | config.yaml, recipes | Context-limit fallback varies Desktop vs CLI |
| OpenHands | Yes | **120 events** (not tokens) | Yes — **separate cheap model** | Append-only event log, TaskTracker | Event-count ≠ token budget |
| opencode | Yes | limit − 10% buffer | Yes — session model | Instruction files = new baseline | Encrypted native checkpoints are non-portable |
| qwen-code | Yes | 0.7 ratio → 3-tier redesign | Yes | Session files | First-turn / `--continue` threshold blind spot |
| Amp | **No** (by design) | n/a | Handoff only | Threads, forks | Archived guidance; current state unknown |
| pi | Yes | `window − 16,384`, per-model override | Yes | Append-only typed session log | Most carefully specified failure handling |

---

## PART 2 — CROSS-CUTTING ANSWERS

### Is compaction model-driven or deterministic?

**Nearly universal: model-driven summarization.** Every harness except Amp uses an LLM to write the
summary. Deterministic mechanisms exist but almost always as a *pre- or post-pass* around the LLM call:
Gemini's 50K tool-output truncation, pi's 2,000-char tool-result truncation, Copilot CLI's 20 KiB
spill-to-file, Goose's background tool-output summarization, Anthropic's `clear_tool_uses_*`.

The notable philosophical split is **which model writes the summary**:
- **Same model**: Roo Code (explicitly, to avoid tool-format "translation" errors), opencode
  ("no separate compaction model"), Crush, Codex CLI (local path).
- **Same family, dedicated alias**: Gemini CLI (`chat-compression-3-pro` etc.).
- **Cheaper model**: OpenHands (`usage_id: "condenser"`), Claude Code SDK compaction
  (`model` param), Amp's handoff ("another model").

### Token counting: exact vs approximation

Roughly a 50/50 split, and this is more consequential than it looks.

**Exact / API-reported**: Gemini CLI (real `countTokens`, both for the trigger *and* to verify the
result is smaller), Copilot CLI, Crush (provider usage, with `approxTokenCount` fallback only when
usage is zero), Claude Code (`/context`, `count_tokens` endpoint).

**Approximation**: Aider (real tokenizer but a crude `max_tokens` heuristic for *when*),
Roo Code (tiered: native endpoint → tiktoken → word-count estimate), opencode (approximate), pi
(token estimates for cut-point selection), qwen-code (`chars/4` — and the design doc candidly says
the ±30% error means estimation is safe *only* for triggering earlier, never for skipping).

The qwen-code writeup documents the two failure modes that approximation creates, and they're
worth remembering: **lagging one turn** (using the previous response's count, which excludes the
incoming user message) and **first-turn zero** (a freshly-resumed or subagent-inherited session
bypasses every threshold). Both are real bugs that shipped in production harnesses.

### Sliding window / token-budget eviction

| Harness | Mechanism |
|---|---|
| Aider | Recursive head/tail split, depth ≤ 3 |
| OpenHands | Pure rolling window: keep_first head + `target_size − keep_first − 1` tail |
| Goose | Background tool-output summarization with a computed cutoff (`GOOSE_TOOL_CALL_CUTOFF`) |
| Anthropic API | `clear_tool_uses_20250919` with `keep: N tool uses` + `exclude_tools` |
| Claude Code | Re-reads ≤5 most-recent files; skill bodies capped 5K/25K tokens, oldest dropped |
| opencode | Prunes long tool output inside the retained recent window |

### Persistent on-disk state re-injected

This is the most under-appreciated mechanism, and it's what separates harnesses that survive
compaction from those that merely don't crash.

- **Richest**: Claude Code (CLAUDE.md, auto memory, plan file, git status, 5 files, skills, hooks).
- **Best integrated into the summary itself**: Crush — it literally injects the todo list into the
  compaction prompt and instructs the resuming assistant to keep using the `todos` tool.
- **Best auditability**: Copilot CLI's per-compaction checkpoint files.
- **Best accumulation discipline**: pi's `<read-files>` / `<modified-files>` tags carried across
  compactions, and the `firstKeptEntryId` re-summarization rule.
- **Strongest stated rationale**: Anthropic's structured note-taking section — Claude Code's todo
  list and the memory tool are explicitly framed as "persistent memory with minimal overhead."

### Published token costs / quality measurements of compaction

This is where the field is genuinely thin. **Only OpenHands publishes numbers**: up to 2x
per-turn cost reduction and "equivalent or better performance" on SWE tasks (methodology
unverified). Anthropic publishes qualitative guidance only ("maximize recall, then improve
precision"). Everyone else: nothing published that I could find. **UNVERIFIED across the board.**

---

## PART 3 — FAILURE MODES

### The best-documented hard-stop: Crush #3620

This is the single most instructive failure I found, because it was diagnosed from a frozen SQLite
store with hard numbers:

Session counters at death: `prompt_tokens = 151,610`, `completion_tokens = 65,536`. Context window
1,000,000. The stop condition was **correctly not due** (remaining ≈848K vs. a 20K buffer).

The killing turn: **one assistant response containing 1,844 tool-call parts** (`finish_reason: length`,
truncated at the output ceiling), followed by 1,820 tool-result messages totalling ~7.4 MB
(~1.35M tokens). Crush executes all tool calls in a step before the next model call, and the stop
condition only evaluates **between steps**, on usage that only exists after a *successful* response.

The cascade:
1. Next request ≈1.49M tokens against a 1M cap → provider refuses with "prompt exceeds max length".
2. Every request is refused **before any step completes**, so `OnStepFinish` / `updateSessionTokenCounters`
   never run again — **the session's token counters freeze at their last successful values**.
3. The `StopWhen` condition reads only those counters, so **it can never fire again**.
4. `Summarize()` can't rescue it either — it streams the same over-window conversation to the same refusal.

**The session is permanently dead while the TUI keeps repainting and every queued prompt mints another
refused request.**

Three transferable lessons: (a) a flat absolute buffer is wrong on a large window — scale it with the
window; (b) **arm recovery on the refusal path**, because a harness that only checks thresholds
between steps cannot see an overflow it caused itself; (c) recovery from overflow must *truncate*,
not re-stream the full history to a summarizer, because the full history is what didn't fit.

### Codex: compaction failure is unrecoverable (GPT-5.5 era)

Reported failure rate of roughly **80% of compaction operations** under GPT-5.5, with sessions
becoming unrecoverable once the context fills. Three interacting causes:
1. **Endpoint incompatibility** — `/responses/compact` didn't fully support the GPT-5.5 model slug.
2. **Context window mismatch** — the model advertised 400K (or 1M) but sessions reported an effective
   window of ~258,400 tokens. **Auto-compaction triggers on the *advertised* window, so it fires too
   late** — the context is already larger than the compact endpoint can process.
3. **Compaction state lock** — once one attempt fails, subsequent turns push context further over and
   the session dies. Restarting the app does not recover it.

Mitigations: set `model_auto_compact_token_limit` to ~60% of the *effective* window; override
`experimental_compact_prompt_file` to enforce cumulative summary preservation; route the compaction
call through a working model (`/model gpt-5.4` → `/compact` → `/model gpt-5.5`).

This is a genuinely instructive failure because the root cause is **a stale context-window constant
in the client** — the harness's whole trigger logic was anchored to a number the model didn't honour.

### Other recurring complaints

- **"Compaction fires too late because my provider changed the window."** Cline #14329 is the clean
  statement of this: 200K → 1M moved the trigger ~150K → ~800K with no config change, producing a
  12% cache hit rate and $49.63 on one task. Same class of bug as Codex's 258K-vs-advertised mismatch.
- **Roo Code's default threshold slider is 100%** — i.e. by default it condenses at the ceiling.
- **Cumulative quality loss across multiple compactions.** Codex ships an explicit warning after every
  compaction: *"Long threads and multiple compactions can cause the model to be less accurate. Start a
  new thread when possible."* GitHub advises starting fresh when *"the conversation has gone through
  many compactions, and you feel that important context is being lost."* A community measurement
  effort (`compression-monitor`) measures post-compaction behavioral drift via "ghost lexicon"
  (terms that disappear after compaction) and tool-call-distribution fingerprints.
- **Auto-compact "going off the rails" mid-task** — widely reported for Claude Code; the mitigation is
  manual `/compact` with focus instructions *before* starting a long task.
- **Lossy and irreversible**: Copilot CLI documents both explicitly.
- **Customizing the compaction prompt is harder than it should be.** A measurement on Claude Code
  (CLI 2.1.186, 354 runs) found all bare `/compact` calls used the stock template, a CLAUDE.md
  "compact instructions" section was applied **0 times** (the compactor is a separate call; CLAUDE.md
  never reaches it), and `PreCompact` cannot inject on auto-trigger (receives empty
  `custom_instructions`). The only working path is pasting a structured block inline after a manual
  `/compact`, and that block **supplements** rather than replaces the stock summary. *(Third-party
  measurement — treat the specific numbers as indicative, not peer-reviewed.)*
- **Aider: `/16` heuristic** decouples the compaction threshold from the actual window.

### What happens with no compaction — the hard stop experience

Every harness converges on the same three degraded modes:

1. **API refusal.** The provider rejects the request (`prompt is too long` / `context_length_exceeded`).
   Goose turns this into an interactive menu (Clear / Truncate / Summarize) — arguably the best UX of
   the group, and the only harness that makes the hard stop a *choice* rather than a wall.
2. **Silent brutal truncation.** The tail of the transcript is dropped with no record. Roo Code's
   25%-truncate-and-retry is the deliberate, bounded version; Cline's "standard rule-based truncation"
   fallback for non-primary models is the undiscussed version.
3. **Unrecoverable wedging.** The worst outcome, and the one that distinguishes mature from immature
   implementations: a failed compaction that leaves the session unable to make progress on any future
   turn. Codex (GPT-5.5) and Crush (#3620) both have documented cases where **restarting the
   application does not recover the session**.

The design lesson, stated plainly: **the recovery path matters more than the compaction path.** A
harness that compacts at 75% but cannot recover from a failed compaction is worse than one that
compacts at 95% and can. Goose, pi, and (after the fixes) qwen-code are the strongest on this axis —
pi's ordering is specified precisely enough to be testable, and Goose refuses to auto-pick a
lossy strategy for you.

---

## PART 4 — DOES THE FRONTIER NEED A COMPACTOR?

**The evidence says yes, decisively, and the vendor consensus has converged on ~60–80%.**

### Direct evidence that models degrade

**Chroma's "Context Rot"** (Hong, Troynikov, Huber; July 14, 2025) is the canonical reference:
18 frontier models (GPT-4.1, Claude 4, Gemini 2.5, Qwen3), 194,480 LLM calls, task difficulty held
constant while only input length varies. Findings:
- Performance degrades with increasing input length **across all 18 models**, on tasks as simple as
  non-lexical retrieval and verbatim text replication.
- **Lower needle-question similarity degrades faster** — semantic ambiguity compounds with length.
- **Distractors have non-uniform impact that amplifies as input grows.** Even one distractor hurts;
  four compound it. Distractors 2 and 3 appear most frequently in hallucinations across models.
- **LongMemEval (~113K tokens, realistic chat-assistant setting): performance is significantly higher
  on focused inputs (~300 tokens) than on full inputs.** This is the most directly relevant finding
  for coding agents — the gap persists *with full reasoning enabled*.
- Counter-intuitively, **shuffling the haystack (destroying local coherence) consistently IMPROVED
  performance** across all 18 models.
- Model-specific: **Claude models had the lowest hallucination rates but the largest focused-vs-full
  gap — because they abstain under ambiguity.** GPT models hallucinated most confidently.
- Needle *position* showed no notable variation in NIAH specifically (though position effects did
  appear in the repeated-words task).

Note the scope limit: Chroma tested Claude 4 / GPT-4.1 / Gemini 2.5 / Qwen3, **not** Opus 5.5 /
GPT-5.x / Gemini 3.x. The mechanism (attention budget dilution, training-distribution skew toward
shorter sequences, n² pairwise spreading) is architectural and has not been solved by longer windows —
but **whether Opus 5.5 / GPT-5.6 / Gemini 3.x degrade less than their predecessors on these exact
tasks is UNVERIFIED.** No 2026 re-run of context rot against the current frontier was found.

### What the vendors say

- **Anthropic**: *"context windows of all sizes will be subject to context pollution and information
  relevance concerns — at least for situations where the strongest agent performance is desired."*
  Waiting for larger windows "might seem an obvious tactic," but they call context a "finite resource
  with diminishing marginal returns" and describe an "attention budget" that every new token depletes.
  They frame it as **"a performance gradient rather than a hard cliff"** — models stay capable at long
  context but with reduced precision. **Explicitly, they do not claim the frontier models render
  compaction unnecessary.**
- **Amp (Sourcegraph)**: *"Most models provide better results with less context… keep conversations
  short & focused."* (Archived, Nov 2025.)
- **Codex CLI** ships a post-compaction warning that long threads degrade accuracy — i.e. OpenAI ships
  a compactor *and* tells you compaction isn't free.

### The convergence signal

The strongest evidence that a compactor is considered necessary rather than optional is that
**independent implementations are converging on Claude Code's specific numbers.** The qwen-code
redesign doc states its constants are *"全部沿用 claude-code 的实测值"* — all taken from Claude Code's
measured values. opencode independently adopted a "buffer below the limit" model. Pi, Crush, Claude
Code, and qwen-code all landed on **absolute token reserves near the window ceiling** rather than a
pure percentage, and Gemini CLI chose the most aggressive default (50%).

---

## PART 5 — STATE OF THE ART CONSENSUS

### What everyone agrees on

1. **Summarization is model-driven, full-history replacement — not a sliding window.** Only OpenHands
   and Aider implement a genuine rolling head/tail window; everyone else replaces history wholesale.
2. **Summaries must be re-injectable as a user-role message with an explicit "another model handed
   this to you" preamble.** Codex's `SUMMARY_PREFIX`, Anthropic's `<summary>` tags, Goose's and
   Gemini's confirmations. This is a near-universal convention.
3. **Recent history is preserved verbatim alongside the summary** — 20K tokens (Codex, pi),
   15K (opencode), 30% tail (Gemini), `max_size/2` events (OpenHands), `keep` tool uses (Anthropic).
4. **Triggers should be absolute token reserves, not percentages.** Percentage thresholds break
   silently when a provider changes the model's window — documented in Cline #14329 and the Codex
   258K mismatch. Claude Code, pi, Crush, and qwen-code all use absolute reserves.
5. **Compact before ~70–80%, not at the ceiling.** The dominant defaults are 50% (Gemini), 70%
   (qwen-code pre-redesign), 75–80% (Cline, Copilot CLI, Goose, opencode v1). The stated reason —
   a summary generated *after* the model is already in the degraded regime is itself degraded.
6. **Persistent on-disk state is what makes compaction survivable.** Anthropic names this explicitly
   ("structured note-taking"); Crush and Claude Code implement it most thoroughly.
7. **Tool output is the main context hog** and deserves deterministic gating before any LLM call:
   20 KiB (Copilot CLI), 50K tokens (Gemini), 2,000 chars (pi), plus Anthropic's server-side clearing.

### Where implementations genuinely diverge

- **Tool-result eviction vs. summarization.** Anthropic is the outlier here: its docs now say
  **server-side compaction is primary** and position tool-result clearing as a complementary
  "lightest touch." Roo Code goes further and refuses a separate condensing model at all.
- **Separable compaction model.** OpenHands and Amp say yes (cheap model); Roo Code says no with a
  specific technical reason (tool-format translation errors).
- **Event-count vs. token budgets.** OpenHands triggers on 120 *events*. Simple, but decouples the
  trigger from the actual constraint.
- **User-configurability.** Goose, pi, Claude Code, and opencode expose thresholds. Cline and Crush
  do not (both have open issues asking for it).

### Emerging ideas worth watching

- **Native/encrypted server-side checkpoints** (Codex remote compaction, opencode `type: "native"`,
  Anthropic's server-side compaction). Reduces client work but introduces **portability constraints** —
  opencode's encrypted checkpoint only works with the same provider, model, and endpoint.
- **Compaction as a safety surface.** The most interesting new paper: *"Governance Decay: How Context
  Compaction Silently Erases Safety Constraints in Long-Horizon LLM Agents"* (arXiv 2606.22528) shows
  in-context governance constraints an agent reliably obeys **while visible can be silently removed by
  compaction**, causing the same agent to perform prohibited tool actions later. This is a real
  argument for keeping policy in files (AGENTS.md/CLAUDE.md), not just in the transcript — and it aligns
  with Cline's measured finding that a CLAUDE.md compaction-instructions section never reached the
  compactor at all.
- **Prompt-injection hardening of the compressor itself.** Gemini CLI's CRITICAL SECURITY RULE in the
  compression prompt is the only implementation I've seen of this, and given that compaction reads the
  entire untrusted history (including tool output) and then writes instructions back into the agent's
  context, it should probably be standard.
- **Circuit breakers over permanent locks.** qwen-code's move from "1 failure ⇒ auto-compact disabled for
  the session" to 3-consecutive-failures-then-force is strictly better and is worth copying.

### Recommendations

- **Never derive a trigger from a percentage of an advertised window.** Use an absolute reserve
  scaled to the window, and let the user lower it.
- **Arm recovery on the refusal path**, not just between steps. The Crush #3620 cascade shows a
  harness can wedge itself permanently if overflow is only detected at safe points.
- **Recovery from overflow must truncate, not re-summarize** — the full history is what didn't fit.
- **Bound the compaction call's output tokens.** Without a hard `maxOutputTokens`, thinking tokens make
  the buffer math unpredictable across providers (qwen-code documents this precisely for Anthropic vs.
  OpenAI vs. Gemini).
- **Guard against the first-turn / resumed-session threshold blind spot**, where the last-reported token
  count is 0 or stale by one turn.
- **Put durable state in files, not in the transcript**, and verify it actually reaches the compactor.
- **Budget a compaction's own cost into the summary.** Claude Code's measured 54,358 → 2,522 tokens
  (95% reduction) is the kind of number that should be the default expectation.

### Honest gaps

- **No harness except OpenHands publishes compaction cost/quality measurements.** The field optimizes
  on vibes and anecdote. There is no equivalent of Chroma's 194,480-call study for compaction itself.
- **Context rot has not been re-run against the 2026 frontier.** All the canonical data is Claude 4 /
  GPT-4.1 / Gemini 2.5. Claims that Opus 5.5 or GPT-5.6 degrade less are UNVERIFIED.
- **Amp's current state is unknown** — the only guide found is explicitly archived.
- **I could not verify Cline's exact current threshold constant from source** (the file I tried to fetch
  404s; it has moved). The ~80% + 40K buffer figure comes from a maintainer response in issue #9181 and
  is corroborated by issue #14329, but the literal constant is unconfirmed.
- **qwen-code's 3-tier redesign is a Draft design doc (2026-05-14)**, not confirmed shipped.
- **opencode's `PRUNE_PROTECT` / `PRUNE_MINIMUM` constants** are from a Dec 2025 analysis of v1; v2's
  docs describe pruning qualitatively without constants.
- **Cumulative multi-compaction degradation is widely asserted but essentially unmeasured** outside of
  one community project and Codex's warning string.
