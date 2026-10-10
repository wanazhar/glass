# Codebase deep-read — six harnesses, feature candidates

Date: 2026-10-10. Method: shallow clones inspected locally at HEAD
(`openai/codex` main, `earendil-works/pi` main, `can1357/oh-my-pi` main,
`pingdotgg/t3code` main, `charmbracelet/crush` main, `anomalyco/opencode`
main, commit `7b3d4ce`). Findings come from repository docs and source, not
runtime comparison. This is round 4 of the harness study: rounds 1–3 are
[harness-features-2026.md](./harness-features-2026.md),
[harness-delta-2026-10.md](./harness-delta-2026-10.md) and
[round3-candidates-2026-10.md](./round3-candidates-2026-10.md); the T3/Pi/OMP
parity round is [t3code-parity-2026-10-10.md](./t3code-parity-2026-10-10.md)
and remains the authoritative gap ledger for the 43-item roadmap. This doc
mines the six codebases for anything those rounds did not already surface.

Every candidate lists the verified glass-core gap it closes. Existing
primitives are never taken as evidence of complete UX parity.

---

## PART 1 — What each codebase actually contains (source-level)

### 1. Codex (openai/codex) — Rust, Apache-2.0, 128k★

`codex-rs` is a Bazel-built workspace of ~100 small crates. Highlights found
in-tree (beyond what the delta doc already recorded):

| Crate / area | What it does |
| --- | --- |
| `apply-patch` | Standalone patch format: `seek_sequence.rs` handles context-mismatch retries before failing; `standalone_executable.rs` lets the patch tool run as its own binary |
| `network-proxy` | Local HTTP (127.0.0.1:3128) + SOCKS5 (8081) egress proxy with per-permission-profile allow/deny and a "limited" read-only network mode |
| `secrets` | age-based secret storage crate (alongside `keyring-store`) |
| `process-hardening` | `pre_main_hardening()` via `#[ctor]`: disable core dumps, block ptrace, strip `LD_PRELOAD`/`DYLD_*` |
| `tcp-tunnel` | Hidden `codex tcp-tunnel`: loopback → HTTP/3 CONNECT proxy, token on stdin, NDJSON diagnostics with fixed event codes, no credentials in output |
| `mermaid` | Bounded terminal-native Mermaid renderer (flowcharts, sequences, states, classes, ER), lane-owned edges, crossings never join routes, width-capped |
| `agent-graph-store` | Storage-neutral parent/child topology for spawned agents (`local.rs` impl, types + errors separated) |
| `file-watcher` | Workspace file-watch crate consumed by core (auto-refresh, conflict detection) |
| `voice-host` / `realtime-webrtc` | Bundled voice helper: WebRTC negotiation in-process, capture worker, Linux ALSA backend, helper exit protocol |
| `mxc-sandbox` / `windows-sandbox-rs` | Windows: MXC `BaseContainerRunner` routing with cached availability probe, `PSE_SUPPORT_FS_DENY` gating, kill-on-close job |
| `state` | SQLite state DB with `audit.rs` (read-only diagnostics queries), `log_db` filters, `delegated_preview.rs`, migrations + telemetry |
| `rollout` | Session log persistence with a `compression` module (zstd-class), list/list_files tooling |
| `hooks` | Hook engine: `config_rules`, `declarations`, `engine`, `events`, `legacy_notify`, managed-hooks-only admin gate |
| `code-mode` / `code-mode-host` | gRPC + process-owned code-mode session provider; tool descriptions augmented with exec samples when code-mode is on |
| `guardian-context`, `user-verification` | Auto-approval reviewer context; verification workflows |

Docs add: `execpolicy.md` (policy engine for exec approval), lifecycle hooks
with `allow_managed_hooks_only` in a `requirements.toml` admin layer.

### 2. Pi (earendil-works/pi) — TypeScript, minimal core

Packages: `agent`, `ai`, `coding-agent`, `client`, `codemode`, `durable`,
`env`, `evals`, `mcp`, `protocol`, `server`, `tui`, plus the odd one out —
`chord` and `telemetry`.

Key surfaces found in-tree:

- **Session tree as the native model** (`sessions.md`): entries stored as a
  tree, every branch kept; `/tree` navigation plus **branch summarization** —
  a second summarization mechanism triggered when switching branches, using a
  near-identical format to compaction (both track cumulative file ops,
  both disable prompt-cache writes).
- **Virtual models** (`virtual-models.md'): selectable model that routes each
  request to a physical model + thinking level; sticky routing for follow-ups
  (`request.reason !== "user"` keeps the previous model); per-request physical
  identity recorded on the assistant message so replays work; footer shows
  `virtual • level → physical • level`.
- **Codemode** (`codemode.md`): raw-JS tool body in a QuickJS sandbox —
  `tools.<name>(args)` calls, `text()`/`image()` output items, `console`
  block, `store()`/`load()` across calls, `// @options` line for
  `max_output_tokens`/`timeout_ms`; overflow keeps start+end and spills full
  text to a temp file.
- **RPC mode**: long-lived child process, four record families (command /
  response / session events), `--mode rpc --no-session`; a positioned
  alternative to ACP for IDEs.
- **Project trust** formalized: package declarations from `.pi/settings.json`
  only load after trust; `--extension` for one-shot trials.
- **Containerization** doc (`containerization.md`) — running the agent
  against container targets as a first-class mode.

### 3. Oh My Pi (can1357/oh-my-pi) — TypeScript + ~80k lines Rust natives

Most feature-dense of the six. New surfaces not in the earlier OMP research:

| Doc | Mechanism |
| --- | --- |
| `vibe-mode.md` | Director/worker split: parent reduced to `read` + 5 worker-control tools, verifies worker claims by reading touched files; interesting because the same toolset reduction is the mechanism, not the metaphor |
| `prewalk.md` | One-shot handoff from planning model to a cheaper execution model mid-task (default target: the `@smol` role) |
| `magic-keywords.md` | Prompt prose keywords (`ultrathink`, `orchestrate`, `workflowz`, `jevify`) that inject typed contracts + change reasoning effort for one turn; one source table, everything derives from it |
| `rulebook-matching-pipeline.md` | Cross-format rule discovery (GC rule files, pi rules) → normalized `Rule` → precedence resolution → rulebook rules vs TTSR stream rules |
| `fs-scan-cache-architecture.md` | Shared Rust walk cache (`pi-walker`): owned `CollectedEntry` lists, invalidation binding `invalidateFsScanCache(path?)` consumed by glob/fuzzy/ast tools; grep deliberately uncached |
| `blob-artifact-architecture.md` | Two persistences: content-addressed `blob:sha256:` for images, session-scoped artifact files for full tool output; `artifact://`/`agent://` internal URLs |
| `session-tree-plan.md` / `collab.md` | Tree-native sessions; `/collab` live guest rendering of the host session in the guest's own TUI (not terminal mirroring), guests can prompt + interrupt, host runs tools |
| `auth-broker-gateway.md` | `omp auth-broker serve` (SQLite credential vault + OAuth refresh) and `omp auth-gateway serve|stdio` (forward proxy so clients never see tokens) |
| `secrets.md` | Reversible redaction of credential-shaped values in provider-bound message text; placeholders restored in tool args before execution |
| `approval-mode.md` | Three tiers (`read`/`write`/`exec`) declared per tool + policy + user override; unknown tools default to `exec`; argument-dependent `policy` shapes with `policyKey` |
| `handoff-generation-pipeline.md`, `session-operations-export-share-fork-resume.md` | Generation pipeline archives, retry/compat behavior |
| `tools/` | 34 tool docs incl. `wait` (block until next background result/peer message/steering interrupt), `checkpoint`/`rewind` pair (mark state, then collapse exploratory context into a retained report), `context_notes` (per-branch experimental notebook), `debug` (single DAP session driver), `manage_skill`, `security_scan`, `ida`, `computer`, `generate_image`, `tts` |
| `non-compaction-retry-policy.md`, `provider-quirks.md`, `ai-schema-normalize.md` | Provider compat layer depth |

Crates: `pi-ast`, `pi-diff`, `pi-edit`, `pi-iso`, `pi-natives`, `pi-predict`,
`pi-shell` (PTY), `pi-vcs`, `pi-vfs`, `pi-voice`, `pi-walker`.

### 4. T3 Code (pingdotgg/t3code) — TypeScript, agent control surface

Beyond the t3code-parity round's coverage, the tree contains:

- `docs/orchestration-v2/` — a full orchestrator redesign spec: durable
  entity graph (Project → AppThread → Run → ExecutionNode tree), the
  principle **"provider ids are evidence, app ids are identity"**, provider
  *capability* objects (sessions/threads/turns/streaming/approvals/planning/
  subagents/context/checkpointing) instead of name conditionals, and a
  shared primitive `thread relationship + source point + optional context
  transfer resolution` behind fork / provider-handoff / merge-back /
  subagent.
- `performance-regressions.md` — the wire-level regression contract: 75
  recent timeline rows + ~1 MiB cold-open budget, bounded HTTP pages merged
  without disturbing scroll, per-fork-ancestor snapshot bounds, 1 MiB + 128
  events resume catch-up budget, raw bodies omitted at the projection layer.
- `internals/assistant-citations.md` — quotes that carry excerpt + stable
  environment id *in ordinary message text* (survive source deletion);
  selector = render-normalized UTF-16 offsets.
- `internals/composer-context-references.md` — typed inline references in
  composer prose (image, file, terminal excerpt, page element, annotation,
  review comment, skill), each with its own codec.
- `internals/context-handoffs.md` — portable handoff = compact text of
  eligible history (240-char compact) + file-change filenames + checkpoint
  file counts; provider handoffs can be full history or delta since last
  handoff.
- `internals/resource-telemetry.md` — standalone Rust `sysinfo` child for
  process metrics, sampled continuously only while diagnostics subscribers
  are live.
- `user/html-renders.md` ("Visual replies") — agent answers with a
  self-contained HTML page; theme-aware; images embedded at publish; built
  using a T3-owned headless browser, page can publish without it.
- `user/snap-shot.md` — attach a window screenshot + app name + window title
  + accessibility tree of the window to the draft.
- `user/question-attachments.md` — agent questions (forms) accept file/image
  attachments; per-question attachment state; reverting a thread removes
  discarded answers' attachments.
- `user/outside-agents.md` — any external agent works through the
  environment's MCP server (`/mcp` URL), one sign-in, scoped grants.
- `user/browser-import.md` — one-time cookie import from another browser so
  the preview browser is signed in.
- `user/thread-migration.md` — copy-on-first-launch V1→V2 sqlite migration.

### 5. Crush (charmbracelet/crush) — Go

- **Bash config** (`docs/config/README.md`): `crushrc` is an actual bash file
  with builtin shell commands (`provider add`, `model add`,
  `permissions allow`, `mcp add`), so config is a program; `crush` can
  configure itself via a builtin config skill.
- **Hooks** (`docs/hooks/README.md` + `internal/hooks/`): currently exactly
  one event, `PreToolUse`, matcher regex on tool name, shell executable in
  any language; exit 2 blocks the tool call, exit 49 halts the whole
  turn (a deliberate no-man's-land code); hooks run in parallel but compose
  in config order; hook input carries tool-call JSON, allowing rewrite +
  inject + auto-approve. Claude Code-compatible in/out.
- **Loop detection** (`internal/agent/loop_detection.go`): SHA-256 window of
  the last 10 calls; ≥5 repeats fires a notification. Tiny, portable.
- **File-read tracker** (`internal/filetracker`): per-session `RecordRead` /
  `LastReadTime` / `ListReadFiles` in sqlite — recentness of reads feeds
  staleness checks.
- **Skills** (`internal/skills/`): agentskills.io standard +
  `manager.go`/`tracker.go` — tracking which skills were actually *loaded*
  in-session (name-based, override-aware).
- **Multi-client serve** (`internal/server/`, SSE, first-wins `--yolo`,
  `agent_setmain`/`agent_cancel` endpoints), pubsub broker, `csync`
  versioned maps, `coordinator` with readiness and MCP gates,
  `agentic_fetch_tool`, `awssso_refresh`, `hyper_credits` (provider balance
  surfacing in usage).
- Desktop notification channel types decoupled from UI (`notify` package).

### 6. OpenCode (anomalyco/opencode) — TypeScript, Effect-based v2

- `packages/core/src` layout says the architecture: `codemode/`,
  `persistent-pty/` (bundled pty binary, not Windows), `worktree/`,
  `formatter/`, `ripgrep/`, `credential/`, `oauth/`, `github-copilot/`,
  `modal/`, `vcs/patch.ts`, `skill/` (discovery + instructions),
  `permission/saved.ts` (permissions persisted in DB per project).
- `session/affinity.ts` — **cache-routing affinity**: child sessions share
  the parent's session id for provider cache routing; forks inherit the
  fork source's. A one-file feature with a visible token win.
- `session/run-coordinator.ts` — key-based serialization: different keys run
  concurrently, same key joins the active execution.
- `session/subagent-job.ts` + `Job.Recovery { kind: "subagent" }` —
  subagent runs are durable jobs with typed recovery records; crash
  recovery re-enters at the job, not the session.
- `session/inbox.ts` — per-session inbox with Promotable items (synthetic
  messages delivered between turns).
- `session/transfer.ts` — typed session transfer schema (tool/skill
  references) riding on drizzle/sqlite.
- `V2_HTTP_API_AUDIT.md` — an actually-executed API audit over 139
  endpoints (openapi-generated client); disposition per endpoint.
- v2.0.26 changelog batch: snapshot packing/reslice/recovery hardening,
  Git pathspec-character fixes, batched git ops to avoid untracked-file
  stalls, persistent random remote-access subdomains, OSC 8 hyperlink
  enablement incl. wrapped rows.

---

## PART 2 — Candidate features for glass-core

Verified against glass-core's shipped surface (docs/INDEX.md, the 43-item
roadmap and its gaps table). Items already open in the 43-ledger are marked;
those are delivery notes, not new candidates.

### Tier 1 — small, near-free, high concrete value

| # | Candidate | Source | GC gap verified | Why it is cheap for GC |
| --- | --- | --- | --- | --- |
| 1 | **Provider cache affinity sharing** | opencode `session/affinity.ts` | GC runs each subagent/forked session as its own provider conversation; children do not share the parent's cache prefix even when their system prompts are identical | GC already has three cache breakpoints + cache-warmer; recording the shared "request affinity id" on subagent messages only requires naming scheme + transport reuse |
| 2 | **Loop detection hash window** | crush `loop_detection.go` (10-window / 5-repeats SHA-256) | Nothing detects the model repeating near-identical tool calls except OOM-by-repetition; recovery inspector classifies failures but not loops | ~80 lines in the agent turn loop: hash (tool name + normalized args), ring buffer, one injected reminder; no claims beyond "flagged" |
| 3 | **File-read recency tracker for stale-edit hints** | crush `filetracker` | `/rewind` and edit staleness already exist; but nothing tells the model "you read this file 40 tool calls ago, reread before confident edits" | GC's tool records already log every read; a last-read-time index is a query over existing records |
| 4 | **Pre-tool-use hook (shell, one match event)** | crush `PreToolUse` (exit 2 block, 49 halt, parallel-run/order-compose) | GC has denial rules + policy dry-run but no user shell hook that can rewrite/inject/auto-approve a specific tool call; rules are data, hooks are programs | GC's existing `authorize` hook + policy paths make a single `pre_tool_use` shell hook with JSON stdin + exit-code semantics a small change; ship the same two exit codes |
| 5 | **Terminal-native Mermaid renderer** | codex `mermaid` crate | Rendered JSON exists; diagrams only print as source | Port the bounded subset (flowchart/sequence/class), edges own lanes, fail closed to source above width |
| 6 | **Hyperlink OSC 8 wrapping-safe click** | opencode v2.0.26 | Transcript URLs are plain text; clicking a wrapped URL is impossible | Bounded to the reader renderer; reuse the existing range/selection code |
| 7 | **Managed-hooks / admin requirements layer** | codex `allow_managed_hooks_only` + `requirements.toml` | Config inheritance/attribution exists; no admin-only layer that user/project config cannot override | Settings layering already knows provenance; one more layer + a refuse-type check |
| 8 | **Notification domain types** | crush `notify` package | Notifications exist (OSC 9/777/bell) but are wired into UI directly | Structure-only adoption: publish typed events from turn lifecycle; UI subscribes. Do only if #26-style hooks are added anyway |

### Tier 2 — medium, high value, clearly missing in GC

| # | Candidate | Source | GC gap verified | Notes |
| --- | --- | --- | --- | --- |
| 9 | **Local egress policy proxy** | codex `network-proxy` (HTTP 3128 + SOCKS5 8081, per-profile) + GC roadmap item 1-sandbox-egress | GC's sandbox egress is port-limited TCP scrubbing; no loopback proxy with allow/deny host policy and "limited read-only network" mode | Run in-process proxy child; sandboxed shells get `http_proxy`/`ALL_PROXY`; Landlock already handles the non-TCP side. Do SOCKS5 in the same child |
| 10 | **Process self-hardening at startup** | codex `process-hardening` (ctor: core dump off, ptrace off, `LD_PRELOAD` strip) | None of the three | ~40 lines, `#[ctor]`-equivalent via a tiny `main` prelude; cheap hardening independent of sandbox opt-in |
| 11 | **Branch summarization on tree navigation** | pi branch summarization + OMP `session-tree-plan.md` | 43-ledger item 26 (branch explorer) is open; the summarization half is independently shippable: entering a branch whose parent is huge emits a compact summary entry | Reuses GC's compaction circuit breaker + fingerprint receipts |
| 12 | **Checkpoint→rewind tool pair (model-invoked)** | OMP `checkpoint`/`rewind` (disable-by-default flag, single-shot, tool metadata approval=read) | GC's `/rewind` is user-side; the OMP pattern is a tool the *model* calls to say "I am going exploring, forget all context after this except a retained report" | Gate behind `checkpoint.enabled`; write a receipt on the durable log; rewind collapses exploratory turns into a summary while keeping file changes auditable |
| 13 | **`wait` tool — block until next background result** | OMP `wait` | Background completions currently surface via `/attention` poll; a runaway wait is the failure mode to bound | One tool: block on the existing event source with a timeout; never a busy poll; interacts with existing attention boundedness |
| 14 | **Prewalk (cheap-model implementation handoff)** | OMP `prewalk` | GC has model fallback (failure-mode) but no *planned* cheap-model handoff | Give a `/handoff cheap`-style flow: current model writes the plan/todo then a target model continues; sticky routing from pi virtual models solves the resume semantics |
| 15 | **Sticky routing for model switches** | pi virtual models `request.reason !== "user"` | Fallback restores on failure, but a mid-turn manual switch loses turn-local coherence (tool follow-ups may land on the new model) | Record the answering model per assistant record (GC largely does); make tool-result feedback stick to the answering model until the user speaks again |
| 16 | **Codemode (sandboxed JS over existing tools)** | pi `codemode.md`, opencode `codemode/catalog`, codex code-mode | GC already has codemode (agent.rs mentions the schema) — verify the *implementation quality*: QuickJS-class isolation, output item protocol (`==> text N/M <==`), `@options` line, spill-to-temp-file on overflow | GC parity check: items from existing docs say codemode exists with preview; the deltas worth stealing are the *output-item protocol* and overflow spill semantics |
| 17 | **`apply-patch` standalone format** | codex `apply-patch` (`seek_sequence` context-retry, standalone executable) | GC edits are revision-checked native ops + hashline; no unified patch format for multi-hunk cross-file edits | Port as an extension, not a tool export. Useful for subagent merge-back and handoff artifacts |
| 18 | **Auth broker/gateway split** | OMP `auth-broker-gateway.md` | GC stores keys in env/config; containerized/remote clients would need token visibility | Later, once remote control lands (43-item item 15). The broker-side shape (vault + refresh, gateway proxy) is directly portable |
| 19 | **Reversible secret redaction in provider-bound text** | OMP `secrets.md` | GC redacts diagnostics bundles but not the *conversation stream* to providers | Opt-in flag: redact credential-shaped values in message text, restore in tool args before execution. Interacts with cache (churn risk) — needs fingerprint receipts awareness |
| 20 | **Filesystem scan cache with explicit invalidation** | OMP `fs-scan-cache-architecture.md` (`pi-walker`) | GC greps/globs fresh each time (or at least nothing cached); large repos pay full walk cost per query | Rust-native fit: shared in-process walk cache + `invalidate(path?)` on every mutation tool path; grep stays uncached initially |
| 21 | **Content-addressed blob storage for large payloads** | OMP `blob-artifact-architecture.md` | Output spill-to-disk exists, but no stable content addressing; handoffs store duplicates | `blob:sha256:` addressing of image payloads AND truncated tool outputs; `artifact://` URLs in renders; `range-readable` references (43-item #23) become byte ranges into blobs |
| 22 | **DAP debug tool (lazy)** | OMP `debug.md` (one tool drives one DAP session) | 43-item #35 defers the debugger adapter; the OMP file shows the exact single-tool surface | Keep deferred; the single-`debug`-tool shape beats multi-tool |

### Tier 3 — larger, strategic, mostly already on ledger

| # | Candidate | Source | Ledger status | Notes |
| --- | --- | --- | --- | --- |
| 23 | Provider **capabilities object** | t3 `provider-capability-system.md` | Related to 43-item #13 (adapters) — adopting the *shape* first is new | Declare per-provider ability flags (image, tools, cache, checkpoints, fine-grained streaming) rather than name conditionals; the adapter task becomes "implement the interfaces" |
| 24 | **App-id / provider-id separation** ("provider ids are evidence, app ids are identity") | t3 `entity-ids-and-correlation.md` | GC mostly uses provider ids inside transcripts | Worth adopting for the durable session log before more checksum/citation work lands |
| 25 | **In-message citation format** (text-in-message citations with stable env ids) | t3 `assistant-citations.md` | GC range attachments record source but not in-message citations | GC's range/quote composer could adopt the same in-message representation so quotes survive file changes |
| 26 | **Typed inline composer references** | t3 `composer-context-references.md` | GC supports range attachments (implemented post-foundation) — adopt the codec separation if extending to terminal excerpts/page elements | Each reference = a small codec; composer stays plain text |
| 27 | **Agent questions with file attachments** | t3 `question-attachments.md` | GC modals are text-only | Ship one extension per question, skip re-revert cleanup unless a revert exists |
| 28 | **MCP server as outside-agents surface** | t3 `outside-agents.md` | GC has ACP for editors; exposing the durable session graph to external agents via MCP is one more surface to the same host | One MCP server on the serve/attach socket |
| 29 | **`sysinfo`-class resource telemetry child** | t3 `resource-telemetry.md` | /usage has provider-side costs; no machine metrics | Only if the usage center grows per-process cost; collector-child isolation is the transferable idea |
| 30 | **HTML visual replies (agent renders a page)** | t3 `html-renders.md` | GC renders Markdown/JSON/tables; rich HTML pages would be new surface | Terminal equivalent: render a bounded, theme-aware page to an artifact:// URL plus preview; do not embed a browser |
| 31 | **Orchestration graph entity types** (AppThread → Run → ExecutionNode) | t3 `core-graph-and-data-model.md` | 43-item thread-lineage pieces are landed; Run/node-level receipt structure would unify multi-tool receipts | Adoption over time — record Run-typed entries alongside existing ones |
| 32 | **Snapshot packing/reslice hardening** | opencode v2.0.26 snapshot batch | GC checkpoints exist; hardening semantics (interleaved/concurrent capture recovery, background packing) are interesting but only at scale |
| 33 | **OpenAPI-generated client + executed API audit** | opencode `V2_HTTP_API_AUDIT.md` | GC's ACP + remote protocols are hand-rolled; a spec-driven server/client + audited endpoint list prevents drift | Do for the *remote* protocol when it lands (43-item #15) |
| 34 | **Persistent PTY binary + durable child** | opencode `persistent-pty/`, OMP `pi-shell` | GC's bash tool has PTY handling already (recent work); a resumable *detached* PTY process across attach/detach cycles is the source-of-truth gap | 43-item #10 (background terminal inspector) — this is the missing runtime half |
| 35 | **Drizzle/SQLite-class schema-tooling for the durable log** | opencode `effect-drizzle-sqlite`, codex `state` sqlite | GC uses its own JSONL log — do not migrate; the transferable piece is typed *migration* discipline (codex `state/migrations.rs` + `migrations_tests`) | Forward path: a versioned schema header on the JSONL log + test-covered migrations |
| 36 | **Rollout compression** | codex `rollout/compression` | GC's JSONL log is plain; large sessions cost disk and scan time | Optional feature-flagged compression for both historical and new records |
| 37 | **Receiver-owned compliance manifests** (Codex plugins skill "allow-list" / OpenHands policy) | delta doc 1.9/1.10 | Plugin manifests verify integrity; no *directory-level ship manifest* cross-check. Review only if plugins grow |
| 38 | **Cookie/credential import for paired special clients** | opencode `github-copilot/` package | GC has no OAuth plugin surface yet | Only when the auth broker (#18) lands |

### Explicitly not taken (again, with fresh evidence)

- **Computer use / browser automation.** OMP proves the maintenance cost
  (`tools/computer.md`, `tools/browser.md`, `tools/ida.md` all exist and
  still require per-Vendor adapters). Same verdict as the delta doc: SKIP.
- **Voice.** Codex `voice-host`/`realtime-webrtc` confirms it is a
  subprocess + native-audio stack; the delta doc's verdict stands: SKIP.
- **Crush's bash config file.** GC's `[profile]`/runtime config and trust
  gate already cover the deterministic parts; executing config as a shell at
  startup was deliberately flagged by Crush's own docs as a load-time
  execution risk. GC keeps data-driven config.
- **Windows-first sandboxes (MXC, restricted tokens).** GC's platform is
  Linux-first with sandbox-exec on macOS. Port only after the Linux+macOS
  story is complete.
- **Crush coordinator/MCP gates, csync, herdr.** These are multi-client
  server internals; GC's serve/attach is 0600-socket-single-session. They
  become relevant only if GC grows a server fleet.
- **T3's web/mobile/desktop clients, snap-shots, browser import.** Wrong
  layer for a terminal harness; the portable cores (attachment/question
  types, handoff codecs) are copied at #25–#27 instead.
- **Aider-style repo map PageRank.** Already the established skip; `index`
  (GC's ctags/outline tool) covers the preload win. OMP's `ast-grep` /
  `pi-ast` crates reinforce that text-indexed AST discovery is cheaper than
  tree-sitter-graph ranking without measured need.

## PART 3 — What the six codebases agree on (so GC's core stays right)

1. **Durable, replayable session logs** are now SQLite (codex `state`,
   opencode drizzle, crush db/sqlc) or JSONL+structured entries (pi, OMP).
   GC's plain JSONL is fine, but versioned schema + migration tests should
   land before more record types are added.
2. **Provider ids as evidence, app ids as identity** (t3) — GC's existing
   per-tool `call_id` conventions line up; keep pushing through plugin/MCP
   layers.
3. **Bounded, budget-classified everything** — cold-open byte budgets (t3),
   10-token windows (crush), max-output-token configs (pi codemode),
   wire budget in remote replay. GC's roadmapped bounds match; the
   discipline to state them in the output (not silently truncate) is
   portable everywhere.
4. **Extensions that restrict the toolset are now a normal pattern** (OMP
   vibe mode; pi scoped modes) — GC's extension system can host this kind
   of "tool budget contract" without core changes.
5. **Every serious harness now measures its own perf** against a pinned
   regression contract (t3 perf doc, opencode `perf/`, codex benchmark
   tests). GC's ship-next-20 item 7 does this; keep it a hard CI gate.

## PART 4 — Suggested next wave (if picking from this doc)

One coherent slice, in existing GC architecture (no-tokio, extension-first,
durable-receipt-first):

1. #2 loop detection + #3 read-recency + #15 sticky model routing →
   agent-turn correctness cluster.
2. #9 egress proxy + #10 process hardening + #7 admin config layer →
   security cluster (pairs with open ledger item 1/16).
3. #12 checkpoint/rewind + #11 branch summarization + #14 prewalk →
   context-management cluster (pairs with ledger item 26).
4. #20 fs-scan cache + #21 blob addressing + #5 mermaid →
   representation/perf cluster.
5. #4 pre-tool-use hook + #13 wait + #8 notify types →
   extensibility cluster.

Each item needs its own task file, verified measurement, and PTY evidence
per the repo's delivery rules. Nothing in this doc marks implementation.
