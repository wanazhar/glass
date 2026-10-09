# Twenty-item shipping round

Status: all 20 items implemented and verified locally on 2026-10-09.

## Delivery

All twenty items are required. Keep the two-tool default, lightweight Rust core,
current visual theme and motion, glass-core package and gc alias. New permission
and extension confinement profiles are opt-in. Do not modify account credentials,
user installs or the separate Glass project. Commit verified waves locally; no push.

| # | Deliverable | Acceptance |
|---|---|---|
| 1 | Incremental session index | Append maintains raw history/index; model context and checkpoint lookup avoid full-file parsing each request; external changes invalidate safely. |
| 2 | Checked writes before effects | Tool intent write/flush/fsync failures stop execution and surface an error; no silent durable-log failures. |
| 3 | Writer locks | One writer per session, acquired before repair; read-only logs/export don't mutate or require exclusive lock; contention is actionable. |
| 4 | Atomic partial snapshots | Write private temp then atomic replace; failed writes retain previous snapshot; throttle stays dirty for retry. |
| 5 | Durable custom acknowledgments | Extension receives success only after checked log append; checked failure returns error; namespacing and session isolation preserved. |
| 6 | Durable prompt dedupe | Served request IDs survive restart; duplicate gets explicit response; accepted/crashed status doesn't rerun unknown effects. |
| 7 | Performance CI | Reproducible startup, key latency, frame/long-history and RSS measurements with calibrated generous regression limits. |
| 8 | History search | Search text/tool command/output/file substrings, navigate matches and jump without changing draft. |
| 9 | Persistent reading position | Entry/line anchor survives resize, overlay close and streamed additions; explicit bottom resumes follow. |
| 10 | Expandable tool cards | Command preview, running time, status and searchable full output; expand/collapse navigation. |
| 11 | Diff browser | File/hunk selection, unified color rendering, horizontal/vertical small-screen navigation. |
| 12 | Reconnecting SSH attach | Retry connection with bounded backoff; saved cursor and durable request IDs; explicit connection status and cancel. |
| 13 | Activity dashboard | Tools, subagents, jobs, loops in one modal with selectable details/actions and empty/busy states. |
| 14 | Provider health | Visible connect/wait/stream/retry/fallback/errors in existing live status, with actionable recoveries. |
| 15 | Permission profiles | Unrestricted (existing default), workspace-edit and read-only; clear per-command opt-in approval/session rules, headless refusal when interactive approval required. |
| 16 | Extension isolation | Opt-in kernel confinement and secret environment scrub for extension children and inherited MCP children; unsupported requested strict policy refuses. |
| 17 | Capability manifests | Declared filesystem/network/subprocess needs inspectable before enable; enforced by confinement where possible, honest limitations. |
| 18 | Plugin integrity | Pin resolved version/revision and SHA256 of installed content; verify before use; show changed capabilities before replacing plugin. |
| 19 | MCP input requests | Host routes validated input-required requests to interactive selection/form/URL modal; headless returns actionable structured input-required error; no secret logging. |
| 20 | Doctor | CLI and modal diagnose terminal/provider config/auth/sandbox/extensions/session integrity; redact values and URLs; read-only by default, offline no provider calls unless explicit probe. |

## Waves

1. Reliability #1–6 plus performance #7 and doctor #20.
2. Terminal and SSH #8–14.
3. Permissions, isolation, plugin integrity and MCP input #15–19.

Each wave: independent review, unit/protocol checks, strict Clippy, fresh release,
all offline smoke suites and PTY at 40x24/20x8. Fix blockers before local commit.
Use real host/provider protocol paths with local mocks, not placeholders. Additional
security reviews must cover credential redaction and deny/refusal paths.

## Terminal layout

```
[session / model / connection status]
[conversation: selectable tool cards / searchable reader]
[back to bottom + unread count, only while reading]
------------------------------------------------------
> composer (draft stays independent of reader/modals)
```

Dialogs replace the owned body only; composer remains intact. At 20x8 show one
focusable list/detail row with clipped headings and explicit scroll keys. Search
uses /search or Ctrl+F while reader focused; Esc cancels search before closing
reader. Diff/activity/doctor use palette commands and selectable rows; Enter
opens details, Esc returns. Keep existing motion rather than adding reduced motion.

## Tracked completion

- [x] 1 Incremental session index
- [x] 2 Checked effect intent writes
- [x] 3 Session writer locks
- [x] 4 Atomic partial snapshots
- [x] 5 Durable extension acknowledgments
- [x] 6 Cross-restart submission dedupe
- [x] 7 Performance regression CI
- [x] 8 Conversation search
- [x] 9 Persistent reading anchor
- [x] 10 Expandable tool cards
- [x] 11 Diff browser
- [x] 12 Reconnecting attach
- [x] 13 Unified activity dashboard
- [x] 14 Provider health
- [x] 15 Permission profiles
- [x] 16 Extension isolation
- [x] 17 Capability manifests
- [x] 18 Plugin pins and integrity
- [x] 19 MCP input-request modals
- [x] 20 Doctor CLI and modal

## Verified checkpoints

Wave 1 plus reader and plugin integrity passed independent verification: see
[wave 1 review](reviews/ship-wave1-001-01.md). All remaining terminal and security items passed the
[final verification](reviews/ship-final-001-01.md), including 571 tests, strict
Clippy, 40 release smoke suites, small-screen PTYs and performance budgets.
No release was published.
