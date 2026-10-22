# Original roadmap acceptance map

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: complete for the original 43-item roadmap, within the explicit limits below. This maps the original 43 candidates,
not the separate six-codebase research numbering. Root implemented the sweep;
two GPT-6-Luna medium agents reviewed it after implementation.

## User workflows and release coverage

| Item | Delivered workflow | Acceptance coverage |
| --- | --- | --- |
| 1 | `/hub`: searchable project/branch/model/status/unread metadata and background follow-up | roadmap-completion, inbox, index |
| 2 | Durable editable/reorderable/deletable follow-ups and steer promotion with receipts | queue, ext-steer-pty |
| 3 | Shared native transcript blocks, semantic tool outcomes, reasoning sections and stable scroll anchors | presentation, blocks, scroll-submit |
| 4 | Attention: failures, exact findings, handoff questions, team events, owned job completions and reviewer checkpoints | recovery, roadmap-completion, workspace unit fixtures |
| 5 | Removable attributed file/range/quote context with previews and size bounds | context, polish, roadmap-completion |
| 6 | Turn checkpoint intervals and native diff inspection; observed authorship separated from whole-branch state | workflows, turn-recovery |
| 7 | Conversation/file rewind preview, keep-files mode, stale checks and indexed shared-workspace guard | checkpoint-rewind, turn-recovery |
| 8 | Named/detached task worktrees, reuse inspection and cleanup preview | worktree, roadmap-completion |
| 9 | Existing stage/commit plus `/pr` status/checks/reviews, preview fingerprint and explicit draft publication | workflows, roadmap-completion |
| 10 | Live owned job inspector, bounded output, stdin/EOF, close/reopen and actual exit status | native-files-jobs, job-unicode |
| 11 | Usage/cache/cost completeness and per-request/team attribution, quota unknowns explicit | context, workflow-next |
| 12 | Settings layer attribution, admin constraints, profile inheritance reset | workflows, workflow-next |
| 13 | Optional named ACP client adapter, initialization capabilities, tickets and bounded polling | adapter-bounds |
| 14 | Private create-only portable export; preview-first import into new conversation; retained history paging | roadmap-completion |
| 15 | Versioned local authenticated serve/attach, observer/control ownership, durable request IDs and bounded replay | serve, turn-recovery |
| 16 | Bound host/project/model schedules, explicit rebind/missed policy, durable acceptance and reconciliation | loop, automation unit fixtures |
| 17 | Optional authenticated loopback browser observer/control client, bounded coalescing and explicit reconnect | companion protocol fixture |
| 18 | Typed findings with locations/evidence/dispositions and exact inspect commands | workspace unit fixtures, roadmap-completion |
| 19 | Optional final JSON Schema contract with bounded correction and partial answer preservation | workflow-next |
| 20 | Parent-selected teammate schema/artifact deliverables with preserved partial results | agent-limits |
| 21 | Deferred request-scoped extension discovery | tooling-foundation, workflow-next |
| 22 | Stable request-prefix/source fingerprints, deltas and invalidation explanations | context, workflow-next |
| 23 | Retained output artifacts with bounded revision-checked ranges and explicit truncation | tooling-foundation, native-files-jobs |
| 24 | Recovery classes, durable attempts, safe recommendations and preserved mutations | recovery |
| 25 | Redacted diagnostic bundle preview/export | workflow-next |
| 26 | Retained branch family navigation with lineage/fork turns, distinct file rewind | checkpoint-rewind, roadmap-completion |
| 27 | Compaction boundaries/history references, observed file sets, summary request usage, preservation focus | turn-recovery, compact unit fixtures |
| 28 | Optional bounded Python AST previews and stale-checked transaction application | adapter-bounds, transaction-recovery |
| 29 | Native/hashline edit benchmark: repeated Unicode, CRLF, mixed endings, no final newline and stale rejection | bench-edit-formats |
| 30 | Debounced LSP diagnostic deltas and stale-checked multi-file actions | lsp |
| 31 | Policy explanation/dry-run with matched rules and compound-shell limitations | permissions-pty, security |
| 32 | Scoped extension cancellation, shutdown, nested lineage and shared cards | ext-steer-pty, elicitation-pty |
| 33 | Pinned bundles, permission/update previews, integrity and rollback | plugin-integrity |
| 34 | Opt-in advisor cadence, deduplicated tracked patches, separate usage and dispositions | roadmap-completion, agent-limits |
| 35 | Lazy named DAP adapter with bounded tickets/capabilities and explicit execution opt-in | adapter-bounds |
| 36 | Opt-in scoped memory with provenance, bounded retrieval and review/forget | memory, workflow-next |
| 37 | Opt-in safe-boundary constraints with retry-deduplicated receipts | workflow-next |
| 38 | Revision/range reads, Unicode continuation, targeted reread and explicit errors | tooling-foundation, native-files-jobs |
| 39 | Batch validation, stale/ambiguous hints, native preview and transaction recovery | tooling-foundation, transaction-recovery, edit benchmark |
| 40 | Create-only/expected-revision writes, metadata policy and validation before replacement | tooling-foundation, native-files-jobs |
| 41 | Streaming/cancellable shell processes, owned background jobs and signal/timeout outcomes | native-files-jobs, job-unicode |
| 42 | Validated state operations, ownership/revisions and durable duplicate request receipts | state-receipts |
| 43 | Stable call IDs/nested lineage, structured outcomes/artifacts, bounded repair and shared native cards | tool-repair, ext-steer-pty, presentation |

Script names refer to `scripts/smoke-NAME.py` unless a benchmark or unit fixture
is named. This table describes shipped bounded contracts, not unlimited upstream
feature parity. Live provider/authentication, external harnesses and GitHub
publication are not required to navigate or inspect the workbench.

## Review corrections

- Retry automation reconciliation after both load and save errors; persist
  acceptance before advancing schedules; binding mismatches cannot block later
  matching schedules. Legacy schedules require deliberate rebind.
- Retain accepted background prompts across provider setup retries without adding
  a second user record. Inbox receipts include file identity and a 256-byte
  delivered boundary hash; replaced/truncated/rewritten boundaries fail visibly.
- Require exact PR preview fingerprints and unchanged HEAD/upstream before
  publication. The CLI does not push, merge or implicitly publish from preview.
- Disclose bounded thread catalogue omissions and branch completeness.
- Keep handoff failure state scoped to the latest response and imported content
  as attributed project data. Retrieve original retained entries, not just the
  compacted active projection.
- Preserve hashline CRLF/mixed terminators and fail invalid UTF-8 reads rather
  than replacing file bytes. Deleting a line preserves untouched preceding
  terminators; final newline removal is a separate explicit edit.
- Avoid per-reviewer RPC fanout in advisor roster snapshots. Full remote notes
  load through exact inspector/transcript selection.
- Coalesce companion text frames, disclose display caps and reconnect as an
  observer. No browser UI work is added to terminal startup.

## Explicit limits

- The hub catalogue is the most recent 512 indexed threads; older IDs remain
  reachable through session search/resume. Branch completeness is disclosed.
- Shell-derived file observations and checkpoint intervals are evidence hints,
  not complete filesystem tracing or exclusive authorship claims.
- Indexed sessions/writing teammates guard coordinated file rewind. External
  editors and concurrently newly created sessions are outside that guard.
- Background processes belong to the running GC process; reconnect means reopen
  its inspector or reattach to its serving runtime, not a new job daemon.
- Summarization usage is provider-reported when available. Diagnostic tails and
  previews are bounded; omitted usage remains unknown, never free.
- Advisor is manual by default, reviews tracked patches and consumes its own
  configured allowance. Unknown prices remain unknown.
- AST support is optional bounded Python syntax transformation; this does not
  claim arbitrary-language refactoring. DAP/ACP capability support is negotiated
  and unsupported reverse execution remains refused.
- Companion is an optional loopback client, not a hosted relay or replacement
  terminal renderer. Its private access link authorizes its local API.
- Performance benchmarks measure deterministic local operation costs and safety,
  not model success rates or guaranteed cache/token savings.

## Final evidence

Implementation commit: `b28abe9`.

- Optimized `target/release/glass-core`, SHA-256 `be10a0da26f422481b49af5f392feb7c37c5dfaa9b1658c2f6c3bc42f14707d4`; unchanged throughout all acceptance runs.
- 819 unit tests passed (713 core, 13 mod API, 93 TUI); one documentation example remains explicitly ignored.
- Strict workspace/all-target Clippy passed with warnings denied; `git diff --check` passed.
- 36 release acceptance runs passed: 33 workflow/security/regression scripts and three explicit extension steering PTYs at 80×30, 40×24 and 20×8. The workflow suite includes another 15 modal PTYs and advisor launch/deduplication; presentation asserts native colors, tables, summaries, scroll footer and composer at all three sizes.
- Both edit formats passed all six byte-preservation and stale-edit fixtures (12 combinations, three repeats). This sweep fixed native mixed-line-ending replacement and made byte equality a mandatory benchmark assertion.
- Measured small-screen worst medians: startup 5.59 ms; first key 0.113 ms; redraw 0.258 ms. For 12,000 history records, reader frame 138.46 ms and search 1.73 ms. These are local fixture measurements, not remote-provider latency guarantees.
- Native 1 MB file read median 3.61 ms and edit median 35.30 ms; ordinary Python writes did not spawn a checker.
- [Machine-readable results](roadmap-completion-results-2026-10-11.json) retain every script outcome and benchmark row. Reproduce with `python scripts/check-roadmap-release.py --report /tmp/gc-roadmap-results.json` using a Python environment with `pyte`, plus the two performance scripts and `bench-edit-formats.py --repeats 3`.
- External providers, real third-party ACP/DAP implementations and GitHub publication were not exercised; mocks and local protocol fixtures cover the implemented contracts. No publication or installation was performed.

Final verification also corrected CLI arity for exact finding/handoff inspection,
persistent serve teammate initialization, and incomplete turn navigation statuses.
