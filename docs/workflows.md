# Coding workflows

GC keeps its two core model tools. Workflow helpers run through the existing shell bridge; opening a local view never calls a model. Optional add-ons start only when enabled. Use `gc help commands` for the complete command catalogue.

## Review changes

Open `/git`, select a changed file, then **Review individual hunks**. Read, stage, mark reviewed, comment and ask the agent to revise, or revert one unstaged hunk. Reverting requires a preview and confirmation. Actions reject a changed diff; reviewed marks and comments belong to the exact patch, so later edits require review again. Renames, binary files, mode changes and untracked files use whole-file review.

A comment becomes an explicit follow-up prompt with a bounded patch excerpt. It does not run a hidden background model request. `gc worktree list` and `gc worktree apply` retain their existing integration preview and conflict checks.

## Tasks, debugging and handoffs

`/workspace` opens native selection screens for tasks, debug evidence, artifacts, role presets, handoffs and hunk review records. Task and hypothesis actions are available in the modal. The same data is accessible over SSH without an interactive terminal:

```sh
gc workspace task add 'Fix parser' --accept 'Parser tests pass' --paths src/parser.rs
gc workspace task add 'Regression coverage' --accept 'Boundary cases pass' --depends 1
gc workspace task claim 1 main
gc workspace task finish 1 main 'cargo test parser: passed'
gc workspace --json
```

Claims use a lock and atomic replacement. Dependencies must refer to existing earlier tasks. Claims wait for unfinished dependencies; overlapping owned files/directories block a different owner. Canonical paths prevent ordinary symlink aliases from bypassing ownership checks. Attribution and locks coordinate cooperating agents; they are not user isolation or proof that a reported check passed. An agent can inspect relevant task entries through the shell instead of receiving the entire board in every prompt.

```sh
gc workspace debug add 'The parser drops a trailing delimiter'
gc workspace debug evidence 1 'Reproducer fails at byte 18'
gc workspace debug instrument 1 src/parser.rs
gc workspace debug resolve 1
```

Instrumentation takes a hypothesis ID and an existing workspace path: `gc workspace debug instrument 1 src/parser.rs`. Record observed evidence before resolving. Registered instrumentation remains visible after resolution: remove temporary instrumentation and verify cleanup before completion. GC does not silently remove source code.

A handoff is a JSON object with exactly these fields:

```json
{"agent":"reviewer","task":1,"changed_files":["src/parser.rs"],"checks":["parser regression passed"],"findings":[],"questions":[],"artifacts":[]}
```

`gc workspace handoff handoff.json` validates and saves locally. Within a running session, `gc agent handoff handoff.json` additionally posts a compact parent notice. Humans can inspect full handoffs from `/workspace`; only the notice needs to enter the main conversation. Unknown fields, tasks, artifacts and excessive collections are refused.

## Role models and review checkpoints

```sh
gc workspace role set reviewer MODEL_ID 10000
gc workspace role set research MODEL_ID 12000
gc agent create parser-review 'Review src/parser.rs' --role reviewer
gc agent checkpoint changes
gc agent checkpoint completion
```

Choose exact model IDs available on your configured connection. Role creation refuses a token request above its configured budget. Per-request output is bounded by remaining cumulative budget after a conservative input estimate. If the provider omits usage, GC keeps a separately labelled estimate rather than reporting zero tokens. Provider accounting can differ from local estimates; these are request guards, not a billing guarantee. The existing explicit `gc agent budget` command can raise a retained agent's budget.

Checkpoints are explicit, read-only teammate requests against the current Git diff. A reviewer role must be configured first. Repeating the same trigger and patch reuses the existing checkpoint instead of paying twice. Different changes produce another checkpoint. Ordinary turns do not start reviewer or watcher models. Use `--session MAIN_ID` when controlling agents from another shell. Persistent teammate transcripts, steering and worktrees remain available through `/agents` and `gc agent --help`.

### Model-managed worker limits

The main model receives a small delegation guide in its normal system prompt. `gc agent --help` exposes the controls; `create`, `inspect`, `wait`, and `list` return structured receipts. There is no transcript injection or model polling.

```sh
gc agent create parser-review "Review parser.rs and its focused tests" --tokens 200000 --output 24000 --rounds 30
gc agent inspect parser-review
gc agent limits parser-review --output 32000 --reasoning 1024 --rounds 40
gc agent budget parser-review 300000
gc agent resume parser-review "Continue from recorded findings; do not repeat completed actions"
```

Defaults are 100,000 cumulative tokens, 16,000 output tokens per response and 24 tool rounds. The main model can explicitly change these controls. Output allowances include provider reasoning where the provider counts it as output. `--reasoning` is a model-dependent hint, not a guarantee; `0` restores provider default and does not disable thinking. Limits changes apply when the next worker turn starts, including after steering. Existing requests retain their accepted reservation. Physical provider ceilings still apply. If the remaining cumulative allowance would clamp output below configured reasoning, GC refuses locally with recovery guidance instead of sending an invalid request.

Receipts distinguish cumulative allowance, reported usage, missing-usage estimates, active reservations, remaining tokens, adapter context window, and the latest estimated request input. Output headroom is an estimate, not a promise that another request will fit. Context window is unknown until the worker constructs a request. A low-allowance notification is emitted once at 20% remaining (reset when budget changes). Completion notices include usage and remaining allowance. Failure receipts identify context allowance, exhausted cumulative allowance, exhausted output, or exhausted rounds and suggest the appropriate control.

A context-allowance refusal attempts deterministic compaction once if it can actually free history; it does not make a paid summarization call or replay completed tools. Output-limit failures return control to the main model, which decides whether to increase output, adjust reasoning or split work. There is no blind retry, automatic budget increase or silent reset of cumulative usage. Raising the budget counts all prior usage; main-model allocations use the existing explicit agent controls, not a new global billing guarantee.

## Requests, context and cache

`/requests` and `gc requests SESSION_ID --json` show the last 100 request records from a bounded tail read. Each record includes requested model, wall duration, first event latency, event byte/count totals, retries, outcome and reported usage. Cost is `null` when exact pricing or usage is unavailable.

Context deltas show model/system/tool fingerprints, the unchanged message prefix, changed instruction/attachment sources and estimated token growth. Source paths are labels; content is hashed, not copied. A fingerprint change describes GC's request, not the provider's private cache key. Cache reads/writes are reported only from actual provider usage. Fingerprints cannot establish why a provider missed its cache.

Opt-in cache refreshes have `kind: cache_warmer` in the same ledger, including their usage and cost when available. They consume quota even while idle. A warmer does not replace the active request's comparison baseline. Provider response identity, exposed finish diagnostics and the complete existing request manifest remain in `/context`. Diagnostics never persist API keys, prompt bodies or tool results a second time.

## Outputs and artifacts

`/outputs` selects recent response/tool outputs. JSON receives a native overview with tables and nested fields, plus a raw view. `gc inspect report.json` uses the same structured projection in CLI presentation; `--json` emits raw JSON. Empty objects and arrays are labelled explicitly. Parsing occurs only after selection; Markdown wrapping is cached until resize. Views cap nodes, depth, rows and output bytes. `/details` retains full recorded tool output.

```sh
gc workspace artifact add 'Test report' reports/tests.json
gc workspace artifact preview 1
```

Artifacts reference existing regular workspace files of at most 8 MiB. They are hashed at registration, checked before preview, and never injected wholesale into context. Text previews are bounded; binary files show their path. A changed or moved artifact must be registered again.

## Code intelligence

Enable the optional `lsp` add-on from `/extensions`. Existing `lsp.json` server mappings stay valid. The `lsp` tool accepts `operation` values `diagnostics`, `definition`, `references`, `hover`, `symbols`, `rename`, and `code_actions`. Positions supplied to GC are **1-based Unicode character columns**; GC converts to UTF-16 for the server. Server-returned locations retain the LSP convention of zero-based UTF-16.

Rename and code-action responses are **previews**. GC executes neither workspace edits nor arbitrary server commands. Inspect affected URIs, then apply accepted edits with `gc file` revisions. Queries have file, frame, deadline and result caps. Servers launch only for enabled, configured language support when used.

## Plugins and policies

```sh
gc plugin scaffold example ./example-plugin
gc plugin validate ./example-plugin
gc plugin trace --json
```

Scaffolding creates a new directory and never overwrites one. Validation reads the manifest/skills without running plugin code. Set `GC_EXTENSION_TRACE=1` before starting GC to record hook names, latency and whether a reply arrived; arguments/results are omitted. Profiling is off by default, adds no model calls, and shows a bounded recent tail. Trace files remain local and can grow with use.

User-level `~/.config/glass-core/policy.json` (or the XDG equivalent) supports `deny_providers`, `deny_endpoint_hosts` and `deny_shell` wildcard pattern arrays. `gc policy --json`, `gc policy shell COMMAND`, and `gc policy provider KIND ENDPOINT` explain decisions. Malformed or non-regular policy files fail closed. Project settings and approval exceptions cannot bypass these checks.

Shell patterns match command text, not shell semantics; third-party extension processes are outside this guard. Use GC's kernel sandbox for filesystem/network confinement. Same-user files are not an immutable security boundary.

## Optional browser checks

Enable the `browser` add-on from `/extensions`. Install Python `playwright` yourself and explicitly expose a local Chromium CDP endpoint; `GC_BROWSER_CDP` defaults to `http://127.0.0.1:9222`. GC does not install dependencies or launch a browser. Exposing CDP grants browser control, so keep it local and use a dedicated browser profile.

`browser_verify` takes an HTTP(S) `url` and optional `expected_text`. It creates and closes its own isolated context, collects HTTP status, console/page errors, and a viewport PNG, and writes private files below workspace `.gc-artifacts/browser-*`. It never closes the connected browser. Register returned report/screenshot paths with `gc workspace artifact add` if they should appear on the artifact shelf. A `passed:false` result is a failed check, not a successful visit.

## Session reuse and skills

`gc resume --all` searches across project threads; `gc resume --browse` adds preview and fork actions for the current project. Previews read a bounded transcript tail. Forking selects a positive source turn and preserves source workspace/attachment metadata. Ordinary `gc resume` keeps fast Enter-to-resume selection. Resuming a cross-project thread switches to its recorded workspace.

`/capture` selects a turn range, previews a skill draft and asks before saving. Headless equivalents separate capture and save:

```sh
gc skill capture SESSION_ID local-check 1 3 > draft.md
# Review and edit draft.md before saving.
gc skill save local-check draft.md
```

Capture includes requests and successful commands, excluding private reasoning and tool-output dumps. Captured content is untrusted reference material. Review secrets and project-specific paths; sanitization is not a complete secret detector. Saving refuses an existing skill and executes nothing. New skills appear in the next session's catalogue.

## Completions and performance verification

`gc completions bash|zsh|fish` prints shell source without ANSI styling. Model/session candidates are local and never query a provider. Install the output using your shell's completion conventions.

`scripts/smoke-workflow-next.py` runs an actual revision-checked coding task with tests against an isolated deterministic streaming provider. It measures latency, usage, edit correctness and PTY output volume, exercises local workflow failures, and checks overlays at 20×8, 40×24 and 80×30. This establishes harness regressions; it does not measure real-model coding quality. `check-performance.py` and `check-native-performance.py` retain startup, input/frame, history, memory and file-throughput gates. The optional real-browser smoke uses `GC_BROWSER_TEST_EXECUTABLE` and otherwise tests browser contracts with a fixture.
