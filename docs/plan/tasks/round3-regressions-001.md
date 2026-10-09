---
id: round3-regressions-001
scope: confirmed defects in 8ca97bd and follow-up commits
status: done
depends-on: [round3-ports-001]
---

## Objective

Independently test the new shipping changes and fix confirmed contract failures.
Preserve user sessions, credentials, installations and unrelated edits.

## Confirmed contracts

- `gc logs` reads raw JSONL without repair, synthetic interrupted results,
  sidecar recovery or any filesystem mutation, including for active sessions.
- Resumed compacted sessions use the latest compaction's saved system prompt.
  A later compaction with no checkpoint clears the older checkpoint. Read-only
  review agents retain their restricted prompt and never adopt a writable one.
- Durable extension custom entries seed from raw history, independent of
  compaction, in headless, interactive, served and ACP constructors. When ACP
  selects another session, empty state must clear prior session custom entries
  and pending queues so state never crosses session boundaries.
- `gc stop` parses complete JSON frames, acknowledges only `t=stopped`, and
  reports success only after shutdown completion rather than a timeout.
  Unrelated output and frames split across reads cannot spoof acknowledgment.

## Paths

crates/glass/src/{session,agent,main,serve,acp}.rs and extension integration
regression tests; applicable smoke fixtures; round3 docs/review.

## Verification

Deterministic fixtures for live-session logs with unchanged bytes/sidecar;
checkpoint resume under changed prompt config; raw custom seeding after
compaction and all entrypoints; split stop frames/false acknowledgment/timeout.
Workspace tests, strict Clippy, fresh release, all smoke scripts and additional
40x24 and 20x8 PTYs for new commands. Local mocks and disposable configs only.

## Result

The confirmed lifecycle, resume and extension-state defects are fixed.
Independent verification passed 521 workspace unit tests, strict Clippy,
release build and all 33 smoke scripts. Additional VT-projected PTYs at
40x24 and 20x8 passed; tiny dialogs were scrolled to inspect offscreen rows.
Runtime fixtures checked explicit/default read-only logs, actual resumed
provider prompts, precompaction custom state in all entrypoints and ACP
cross-session clearing. See [the review](../reviews/round3-regressions-001-01.md)
for exact binary and evidence. Only local mocks and disposable data were used.
