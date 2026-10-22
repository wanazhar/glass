# Terminal shipping review 10–14

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

**Reviewed:** `fbc56a0`  
**Scope:** transcript/tool reader, diff pager, activity actions, provider status, and durable serve/attach recovery.  
**Conclusion:** **pass** after the attach recovery fix below.

## Verification

- `python3 scripts/smoke-reader.py` — passed: exact tool output search, explicit spill loading and marker behavior, draft retention, diff file/hunk/long-line navigation and colors, activity job actions, 40×24 and 20×8 PTY layouts.
- `python3 scripts/smoke-serve.py` — passed: replay, streaming, broadcast, cancellation, duplicate IDs, cursor reattach/reset, restart recovery with one execution, stop/logs/respawn.
- `python3 scripts/smoke-chat.py` — passed: streamed formatting, draft resize, compact tools/errors, cancellation, and five PTY sizes.
- `python3 scripts/smoke-inline-scroll.py` — passed: offline reading, retained draft, wheel/PgUp/Ctrl-Home, five sizes, and resize.
- Updated `python3 scripts/smoke-serve.py` — passed in the debug integration run after corrupting auth after attach startup: the factory error clears the durable pending ID, makes no provider call, and the same PTY successfully submits a later prompt after auth is repaired.

The reader smoke exercises the selected card through search and Enter, including the explicit 64 KiB spill path and body-marker search. The diff fixture covers multiple files/hunks, semantic row tones, and horizontal navigation. These are focused implementation checks; the repository-wide tests, strict Clippy, release build, remaining smokes, and performance gate are owned by the integration run.

Before the attach follow-up change, all four smoke scripts passed against the then-current integration release binary using `GLASS_CORE_TEST_BINARY=/home/ubuntu/work/glass-core/target/release/glass-core`. The new provider-construction regression has been verified against the updated debug binary; the release rebuild is part of the integration gate.

## Findings

### Resolved P2 — Provider construction errors left an attached prompt pending

The original implementation in [`serve.rs`](../../../../../crates/glass-core/src/serve.rs#L481) durably accepted a request before provider construction, but a `factory()` error omitted the request ID and cursor. The attach client only clears a matching durable pending request when an `error` carries that ID ([`update_attach_state`](../../../../../crates/glass-core/src/serve.rs#L89)).

The fix now includes the correlated request ID and cursor, persists `failed` for the accepted request, and sanitizes the provider construction detail. A repeated failed ID is reported as failed before execution, with instructions to submit a new request, rather than incorrectly described as completed. The regression above verifies that pending clears, no provider request runs, and the same attach client can submit successfully afterward.

## Reviewed paths without additional findings

- Tool cards are keyed by call ID and turn in reader state; search traverses loaded full output, and explicit spill loading is capped. Transcript anchors retain text/role/turn/occurrence identity across width changes and view switches.
- Diff parsing retains file and hunk boundaries, with distinct add/remove/hunk/context tones and pager navigation.
- Activity rows are rebuilt after each action, so job and loop actions return to a refreshed list; stop/remove operations require confirmation.
- Retry, fallback, and provider failure phases are distinct and show sanitized recovery hints. Provider fallback is emitted only for stream setup errors where no events were produced; authentication/config errors do not silently fall through.
- Attach persists the raw-record cursor and pending request ID/text, bounds reconnect delay, and the serve smoke confirms one execution after durable restart recovery.
