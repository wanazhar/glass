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
