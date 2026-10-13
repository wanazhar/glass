# Integrated roadmap implementation review

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: implementation review complete; release checks passed.

## Scope

Reviewed the merged six-codebase waves and unfinished extension steering work
at `e9154f7`, plus the earlier transaction, adapter and remote-control foundations.
Root implemented fixes; two GPT-6-Luna medium agents reviewed after implementation.
This review does not close every acceptance gap in the original 43-item roadmap.
The six-codebase candidate numbers are a separate numbering scheme.

## Corrected findings

- Extension steering: scoped workers keep the registry borrowed until completion,
  preserve interactive elicitation on the UI thread, and record one extension
  result rather than a shell result. Nested state writes and receipts are checked
  and persisted on the session thread before acknowledgement. Nested shell work
  watches the turn cancellation token. Cancelled unfinished serial exchanges
  stop their owned process group so late replies cannot contaminate later calls.
- Proxy: honor the requested CONNECT port, bound header time, reserve worker slots
  before spawn, release slots on spawn failure, and stop the listener on drop.
  DNS lookup still uses the system resolver; it is not a hard DNS deadline.
- Rewind: preserve UTF-8 boundaries when eliding long Unicode reports. Retained
  byte counts describe the actual omitted range.
- Subagents: explicit personas take precedence over inherited raw base prompts.
  Raw-prompt reuse alone does not establish provider cache affinity or savings.
- Read recency: attach notices to observed shell write results instead of an
  impossible `state.path` request. Loop detection includes extension tool names.
- Remote control: a fifth client cannot evict the input owner; invalid control
  actions cannot claim ownership.
- Transactions: application and recovery share a private regular-file lock;
  concurrent recovery is refused and recovery receipt size/count are bounded.
- Adapters: reject malformed frames/tickets, clean up failed startup and protocol
  errors, and make owned process cleanup idempotent. AST input is capped at
  512 KiB before parsing; comments and Unicode spelling remain intact.
- Settings: show enforced admin requirements as the highest-precedence source,
  including explicitly equal values. Invalid requirements abort launch.
- Tests: honor `NO_COLOR`; the edit-format benchmark reports correctness across
  every repeat, rather than only its last run. The unfinished PTY fixture now
  launches valid Python and verifies durable nested state receipts.

## Evidence

- Workspace suite: 708 core + 13 mod API + 93 TUI tests, zero failures;
  one ignored doctest. `/tmp/gc-roadmap-final-tests.log`.
- Strict workspace/all-target Clippy: `/tmp/gc-roadmap-final-clippy.log`.
- LSP diagnostic deltas / multi-file stale protection, codemode output protocol,
  Mermaid fallback and adapter/AST bounds smokes passed.
- Actual optimized release binary SHA-256:
  `69b2b9038017488d0a11517d39d42699517c39f4ec30d8b18cb0b5e44a57219d`.
- Release smoke coverage: transaction recovery/locking/admin provenance, native
  files and jobs, turn recovery, egress, state receipts, checkpoint rewind, wait,
  shell hooks, crash recovery, CLI formatting/JSON/non-Git/resume, and extension
  steering with nested state receipts. PTY sizes: 80x30, 40x24 and 20x8.
  Results: `/tmp/gc-release-review-results.json` and per-test logs listed there.
- The initial 20x8 steering assertion incorrectly required contiguous raw terminal
  bytes across wrapped text. The corrected assertion checks the durable final
  assistant record while continuing to pump the PTY; the rerun passed.
- Performance budgets passed: help startup 1.14 ms median, interactive startup
  5.62 ms, key visibility 0.12 ms, redraw 0.28 ms, 12,000-record history frame
  118.18 ms, search 1.85 ms, child peak RSS 3,748 KiB. These are local fixture
  measurements, not provider/network latency claims.
- Native throughput passed: ~100 KB read 1.48 ms, ~1 MB read 3.42 ms, ~8 MB
  read 17.54 ms, ~1 MB edit 25.46 ms median; no unsolicited checker process.
  Logs: `/tmp/gc-roadmap-reviewed-perf.log` and
  `/tmp/gc-roadmap-reviewed-native-perf.log`.
- Three-repeat edit comparison preserved exact expected bytes for every native
  revision fixture. The bundled hashline comparison still normalizes CRLF;
  that limitation remains open. Timing comparisons include process startup and
  do not establish model edit success rates. `/tmp/gc-roadmap-reviewed-edit-formats.json`.
- Both reviewers' concrete blocking findings were corrected; their final static
  spot checks identified no additional concrete blockers in the reviewed paths.
  Installed binaries and live services were not replaced or interrupted.

## Remaining roadmap truth

The acceptance table in `docs/plan/tasks/roadmap-delivery.md` remains authoritative.
Do not infer full workspace-hub, companion, advisor, PR/CI, capability-discovery,
full prefix-affinity or other workflow acceptance from a primitive or from these
merged waves. Current unit and smoke passes verify their tested contracts only.
