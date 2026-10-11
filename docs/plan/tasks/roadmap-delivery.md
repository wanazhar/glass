# Full roadmap delivery ledger

Source: [43-item roadmap](../../../research/t3code-parity-2026-10-10.md).

Status: in progress. All items are authorized. Existing primitives do not count as completed workflows. Root implements the full sweep; Luna medium reviews only after implementation. Slice commits are allowed; builds, regression execution and actual release PTYs run in the final pass. Each delivery requires measured performance where the hot path changes.

## Verified checkpoint

`7e08f2b`: tooling and turn recovery foundation. [Evidence](../reviews/tooling-foundation-001.md). This is partial delivery of items 3 and 38–43.

## Acceptance gaps from source review

| Items | Remaining delivery |
| --- | --- |
| 1 | Unified searchable workspace/thread/team hub, status metadata and background submission |
| 2 | Durable queue and receipts, edit/reorder/delete/promote modal, restart and cancellation checks; implementation in progress |
| 3 | Reasoning/error/export rendering parity and live/scroll acceptance |
| 4 | Attention inbox with exact event targets |
| 5 | Range/quote composer, removable attributed context and size previews |
| 6 | Turn-owned changes separated from imported/branch changes |
| 7 | Conversation-plus-files rewind, keep-files mode and shared workspace protection |
| 8 | Worktree task creation/reuse and cleanup preview |
| 9 | PR/CI/review workflow with explicit publication |
| 10 | Live background terminal inspector and reconnect/ownership UX |
| 11 | Cache/quota/task usage center with reported versus estimated values |
| 12 | Settings inheritance/provenance viewer and reset |
| 13 | Optional external harness client adapters |
| 14 | Portable bounded handoffs with omitted-history references |
| 15 | Authenticated versioned remote control, ownership and bounded replay |
| 16 | Automation run history, host/project/model and missed-run policy |
| 17 | Optional companion client using the event contract |
| 18 | Typed navigable review findings and dispositions |
| 19 | Validated JSON Schema final answers and bounded correction |
| 20 | Parent-selected teammate deliverable contracts and partial artifacts |
| 21 | Deferred request-scoped capability discovery |
| 22 | Prompt-prefix change and invalidation audit |
| 23 | Stable range-readable output artifacts and truncation distinctions |
| 24 | Recovery inspector with failure class, attempts and safe actions |
| 25 | Redacted diagnostic bundle preview |
| 26 | Message branch explorer and retained alternatives |
| 27 | Inspectable compaction receipts and history/file references |
| 28 | Optional stale-checked AST transactions |
| 29 | Realistic hashline versus revision-edit benchmark |
| 30 | Debounced diagnostic deltas and stale-checked multi-file code actions |
| 31 | Policy dry-run and matched-rule explanations |
| 32 | Extension cancellation/shutdown/resource/lineage contracts |
| 33 | Pinned extension bundle update preview and rollback |
| 34 | Optional advisor cadence, usage and dispositions |
| 35 | Lazy optional DAP debugger |
| 36 | Opt-in project memory with provenance, review and forget |
| 37 | Opt-in boundary constraint reminders and deduplicated receipts |
| 38–43 | Remaining read/edit/write/bash/state/shared contracts and representative benchmarks |

## Delivery order

Finish the durable queue, then transcript/context/review contracts. Build artifact, usage and recovery primitives before the hub and attention inbox. Keep optional adapters, language services, debugger and companion UI lazy so they do not add terminal startup work. Checkpoint each verified slice without marking the whole roadmap complete.

## Implemented after the verified foundation (final verification pending)

- `471a0e8`: durable queue and delivery reconciliation; final PTY pending.
- `5f20cc2`: typed findings and revision-checked artifact byte ranges.
- `bdd5da8`: active-turn command access, bounded result contracts, diagnostic previews,
  opt-in attributed memory, range attachments and teammate result schemas.
- `3b3a102`: default framework prompt reduced to 1,093 bytes, tool exclusivity corrected.
- Automation safety: bounded strict sidecar reads, overflow rejection, persistence before
  dispatch, last 64 dispatch receipts via `/loop history`. Corrupt schedules remain intact.
  Receipts describe dispatch, not completed execution. Host/model binding remains open.
- Remote replay: version negotiation (legacy clients accepted), 256 KiB wire budget,
  32 KiB UTF-8 entry previews with explicit source bytes/truncation; ownership remains open.
- Worktrees: detached task creation from a verified commit, inspect/reuse path and
  cleanup preview without deletion; repository identity uses each checkout's own root.
- CLI discovery: contract, diagnostics and memory help plus completion candidates.

These are implementation checkpoints, not acceptance or full-roadmap completion.

- History inspection and handoffs: bounded summary and state previews, raw history retrieval references, typed branch lineage; branch explorer and receiving-host import remain open.

- Attention: searchable provider/tool failures and teammate event previews with exact
  session/raw-record/turn targets, at most 128 entries, 32 KiB UTF-8 previews.
- Workspace hub: shared workflow entry point for conversations, teammates, attention,
  tasks/findings, requests and history. Cross-workspace status search and background
  submission remain open; this navigation entry point alone does not complete item 1.
- Automation polling is throttled to four reads per second per active input thread,
  with a single cached session identity and no service startup.

- Usage center: bounded recent request totals, reported cache ratio, known cost subtotal,
  explicit completeness and quota unknowns, live teammate usage separated from estimates.
- Rendering: live tool status now preserves failed outcomes with the same classification
  used by completed printing and replayed extension tool cards. Final PTY parity pending.
- Contract correction: reject a correction whose input exceeds the safe context window;
  retain the original answer and record invalid corrected results as failed diagnostics.

- Effective settings: on-demand layer attribution for default/global/account/addon/
  profile/runtime changes, explicit equal-value ambiguity, hashes instead of prompt,
  endpoint and executable argument copies. Workspace reset uses the existing profile UI.
- Constraint reminders: `/constraints add TEXT`, explicit on/off, clear and list;
  max 16 items / 512 bytes, durable turn/fingerprint receipts deduplicate retries.
  Delivery occurs before a turn at a safe boundary, with no cached system-prefix change.
  Corrupt sidecars refuse delivery rather than silently running without requested constraints.
- Memory attribution now says explicitly saved facts rather than asserting a human
  reviewed data written through a CLI the model can also invoke.

- Pinned plugin updates: `gc plugin update NAME --preview` stages and reports hashes,
  versions and permission changes without installation; `--expect-hash HASH` enforces
  the reviewed candidate. Updates retain a previous integrity-pinned snapshot;
  `gc plugin rollback NAME` restores it after checking installed/snapshot hashes.
  Concurrent CLI mutations use a private lock, corrupt lock files fail closed, and
  hidden staging/rollback trees are excluded from normal plugin discovery.
  No running extension is restarted; the next session uses the installed version.

- Policy dry-run: `gc policy explain COMMAND` reports matched global rules,
  configured/runtime confinement and exact exceptions without shell execution;
  compound-command limits are illustrated explicitly. `gc policy extension SPEC`
  validates capability declarations without launching or activating the extension.

- Format benchmark fixture: `scripts/bench-edit-formats.py` compares process-inclusive
  native revision and bundled hashline reads/edits on small, 1 MiB, repeated Unicode
  and CRLF files; reports byte-exact correctness, request/response size and medians.
  It does not claim model success rates. Execution remains deferred to final validation.

- Turn changes and rewind: completed checkpoint receipts retain observed interval
  boundaries; `/turns` opens native diffs. `/rewind` retains the original thread,
  defaults to keeping files, and offers confirmed file restoration with stale-edit
  checks and a recovery checkpoint. Active indexed sessions and writing teammates
  block file restoration. External editors and newly created sessions are outside
  this coordination guard; receipts do not claim exclusive agent authorship.
  Checkpoint labels are validated, reads bounded and writes private.
  Regression fixtures added; build and PTY execution remain deferred.

- Follow-up robustness: plugin receipts are prepared before replacing installed
  content and restoration failures report their actual outcome and backup path.
  Automation saves enforce the same 1 MiB limit as reads, including JSON escaping.
  Handoff previews no longer associate an older answer with a newer failed turn.

- Resume queue: `/queue-run` sends saved work immediately, with provider readiness
  checked before the durable delivery receipt is written. It reuses the existing
  queue delivery transaction and does not append a duplicate user message.
  Saved text bypasses slash-command interpretation; empty queues stay idle.

- Command discovery: prompt-history selection is `/prompts`; `/history` is reserved
  for retained conversation and compaction receipts. Removed the duplicate palette
  name and unreachable prompt-history handler. Ctrl+R remains prompt search.

## Wave verified 2026-10-10: items 24, 30, 32, 42

Four roadmap items landed as disjoint worktree tasks and merged to master; each
was verified independently and again on the integrated tree.

- `8811ea5` (item 30): LSP diagnostic deltas — the last-shown set is the
  baseline, so a tool result leads with newly introduced, resolved and
  reworded problems and collapses an unchanged file to one line. Multi-file
  write checks iterate every written path under stated file-count and
  wall-clock bounds. Code actions and renames carry the SHA-256 revision of
  every target and refuse to apply when any revision moved, reusing the
  revision-checked edit path. [Smoke](../../../scripts/smoke-lsp.py) PASS.
- `77bd85a` (item 24): recovery inspector — one durable attempt record per
  retry (attempt number, applied backoff, caller-safe cause, session record
  index, provider request id when one came back); per-failure class to one
  bounded safe action with no replayed-mutation promise; transport reset
  appends exactly one marker and preserves the session log; navigable
  `/recovery` with the raw JSON export kept, and the shareable bundle still
  omits ids, prompts, credentials and endpoints while the local inspector
  shows them. [Smoke](../../../scripts/smoke-recovery.py) both phases PASS.
- `50f407a` (item 32): extension lifecycle — a cancel token outlives the
  startup handshake so a cancel reaches a running call within one poll
  interval; `CallOutcome` separates completed, unavailable, cancelled (with
  the path taken) and failed; nested calls carry the originating `call_id`
  and `CallLineage` with cyclic, re-parented and over-deep chains refused
  explicitly; per-extension shutdown outcomes stay idempotent behind the
  process-group kill and `Drop` guarantee. Wiring the agent loop's per-call
  scope is the remaining follow-up for item 43.
- `d82ffd3` (item 42): state receipts — explicit request ids recorded
  durably; a replayed id returns its recorded receipt without re-applying
  and the same id with different content fails loudly; receipts are written
  only after the log append succeeds; legacy receipt-free logs restore
  unchanged; the revision read-back reports computed ownership instead of a
  hardcoded string. [Smoke](../../../scripts/smoke-state-receipts.py) PASS.

Integrated verification on master at a fresh release build: 629 + 13 + 91
tests pass, 0 fail; strict Clippy clean; smoke-lsp, smoke-recovery and
smoke-state-receipts all PASS. Items 24 and 30 are complete; item 32 is
delivered except the agent-loop call-scope wiring; item 42 receipts are
delivered and the remaining state-ownership surface stays open.

## Waves A–E delivered 2026-10-11 (six-codebase feature wave)

These waves use candidate numbers from `research/six-codebases-2026-10-10.md`,
not the original 43-item roadmap. They do not close identically numbered rows
in the acceptance table above.

Five parallel task branches merged to master, each behind its own task file
with disjoint path ownership, each green on strict clippy and the full test
suite before merge. Commit per wave:

- `e06417d` (wave A; six-codebase candidates 1–3, 15): loop detection module, read recency /
  staleness hints, sticky routing, cache affinity.
- `3e8fc78` (wave B; six-codebase candidates 7, 9, 10): process hardening (ctor hardening
  strip), admin `requirements.toml` layer, egress policy proxy with per-
  permission-profile allow/deny. [Smoke](../../../scripts/smoke-egress.py) PASS.
- `06ba3b2` (wave C; six-codebase candidates 11–14): model-invoked checkpoint/rewind with the
  durable `Record::Checkpoint` marker and projection drop; extension-tool
  `wait` on the attention rows; prewalk cheap-model handoff with a headless
  `gc prewalk --check` gate; branch summarization past the 60-record gap.
  [checkpoint/rewind](../../../scripts/smoke-checkpoint-rewind.py) and
  [wait](../../../scripts/smoke-wait.py) PASS.
- `1b8ea54` (wave D; six-codebase candidates 5, 6, 20, 21): bounded mermaid flowchart
  renderer gated by `mermaid = true` (caps fall back to source); bounded
  fs-scan walk cache for plugins/skills with counters in `gc usage`;
  content-addressed spill blobs (`blob:<sha256>`, atomic rename, dedup);
  OSC 8 hyperlinks behind a conservative TERM gate.
  [mermaid](../../../scripts/smoke-mermaid.py) and
  [blob addressing](../../../scripts/smoke-blob-addressing.py) PASS.
- `74bb707` (wave E; six-codebase candidates 4, 8, 16, 25): pre-tool-use shell hook (exit 2
  block with stdout as the refusal body, 49 cancels the turn, others
  warn-and-allow, 3 s bounded, process-group killed, sequential, never in
  read-only mode); typed `TurnEvent` publish from the turn boundaries with
  sequences unchanged; codemode parity audit recorded in the task file
  (deltas listed; no engine swap).
  [Smoke](../../../scripts/smoke-hooks-shell.py) PASS.

Integrated verification on master: 684 unit + 13 doc + 93 integration tests
pass, 0 fail; strict Clippy (`-D warnings`) clean; all five new smoke
scripts PASS against the merged binary. Citation markers (item 25) are a
recorded audit with the dedupe helper in place, not a shipped user surface
— the statement is in the task file, not a silent skip.

### Post-merge follow-up 4: agent-loop call-scope wiring (item 32 closed)

The one remaining item-32 gap — the agent loop never built a `CallScope`
for extension tool calls — is now wired: every extension dispatch carries
`CallScope::from_call(call_id, turn_cancel)`, so lineage is real for
nested calls and the per-turn token cancels a running exchange the moment
a steer arrives. The transcript reports cancelled calls through
`CallOutcome::describe` — cooperative stop vs. killed process group —
instead of the old generic "did not return a result" error. Scope-bearing
helpers became the only live path; `run_tool`/`run_tool_with_input` are
test-only, and the `#[allow(dead_code)]` annotations the wiring made
unnecessary are gone. Test: `ext_cancel_tests::a_steer_mid_turn_cancels…`
against a real Python extension process. Suite 706 unit + 13 doc + 93
integration; clippy `-D warnings` clean.

### Post-merge follow-up: wave E limitations fixed (this commit)

- **Citations (wave E item 2)**: `@path#L10-L24#hash8` markers now ship.
  `polish::attach_excerpt` records `Citation{first,last,revision}`;
  `polish::with_citation_markers` stamps uncited markers into the next
  submitted user message (interactive loop, once per excerpt);
  `session::dedupe_citation_markers` dedupes at the `compact::project`
  boundary. Old `.meta.json` and old logs load unchanged (serde default;
  no new Record variant). Also corrects the wave E delivery note, which
  claimed a session.rs dedupe helper existed when it did not.
- **Blob GC (wave D item 3 remainder)**: `gc blobs sweep [--json]`
  removes unreferenced blobs older than 300 s, keeps referenced and
  recent ones, counts unknown names as errors. Found and fixed a latent
  reference-scan bug that would have deleted live blobs
  (`hex_of` whole-string validation vs. quoted trailing characters).
- Evidence: `blobs::sweep_tests` + `sweep-e2e` synthetic data-dir run;
  `polish::tests::excerpt_marker…`, `session::tests::dedupe…`,
  `citation_marker_parses…`; full suite 689+13+93 green, clippy
  `-D warnings` clean.

### Post-merge follow-up 2: codemode protocol + sequence diagrams (this commit)

- Codemode reaches pi's output-item contract at protocol level (no engine
  swap): `==> text N/M <==` items, `<console_output>` blocks,
  `// @options:` first line with warn-on-unknown, bounded overflow spill
  naming the temp file, timeout enforced as a tool result (the old code
  crashed the extension process on vm timeouts). Smoke:
  `scripts/smoke-codemode.py`, 12 checks, all green.
- Mermaid renders sequence diagrams natively: participants/actors,
  four arrow families + `x` marks, self-messages, `Note over`, one
  nesting level of loop/opt/break, 8/64/1/88-col caps, source fallback
  outside them. 4 new unit tests; `smoke-mermaid` rerun green.
- Full suite after both: 693 unit + 13 doc + 93 integration, clippy
  `-D warnings` clean.

### Post-merge follow-up 3: state diagrams + rank propagation fix (this commit)

- Mermaid grows its third family: bounded stateDiagram rendering
  (`[*]` pseudostates, labeled `-->` transitions, `state "X" as Alias`,
  `ID: desc` lines), reusing the flowchart tree and caps.
- Fixed the flowchart rank bug the third family exposed: single-pass
  rank assignment left tail-first chains at stale ranks and silently
  fell back; ranks now propagate to a fixpoint (bounded by node count)
  and same-rank nodes render with their own edges.
- Evidence: 3 new state tests + existing mermaid tests green; full suite
  696+13+93; clippy -D warnings; smoke-mermaid rerun green.

## Integrated implementation audit, 2026-10-11

[Review and corrections](../reviews/merged-roadmap-2026-10-11.md): fixed extension
steering, nested state receipt persistence, cancelled-exchange isolation, Unicode
rewind reports, proxy lifecycle/port/worker bounds, subagent persona precedence,
read-recency wiring, extension loop detection, remote ownership, transaction
recovery locking and adapter cleanup/bounds. Admin requirements are now shown in
settings provenance. Raw prompt reuse is not a verified full-prefix cache-affinity
implementation; the earlier equal-cache-prefix/savings claim is withdrawn.

Original-roadmap acceptance gaps are still open unless explicitly verified above.
Six-codebase candidate numbers are not interchangeable with original roadmap IDs.
