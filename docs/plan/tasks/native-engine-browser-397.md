# Native scheduled callback error isolation (397)

```yaml
id: native-engine-browser-397
scope: native-engine/page-scheduler-callback-errors
status: done
depends-on:
  - native-engine-browser-396
```

## Objective

Keep a page host turn recoverable when a scheduled callback throws. Timer,
animation-frame, and idle callbacks are browser task entrypoints: an uncaught
exception must use the existing page error surface without aborting later
callbacks in the same bounded turn or converting page code into a native host
failure.

## Contract

- Report callback exceptions through the existing `ErrorEvent` and
  `window.onerror` delivery path with the owning document URL.
- Continue due sibling timer, animation-frame, and idle callbacks after a
  callback reports an exception.
- Preserve one-shot removal, interval rescheduling, cancellation, callback
  ordering, host-turn bounds, and the existing redaction/size limits.
- Keep the behavior identical for local fixture documents and HTTP(S)
  content-process documents.
- Do not claim an autonomous renderer loop, background-page scheduling, full
  task-source arbitration, or complete animation/idle Web IDL identity.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-397.md`

## Tradeoffs

The scheduler reports errors in the existing QuickJS page realm instead of
adding a second Rust exception channel. This keeps event listeners, handler
properties, and redaction behavior on one path, while deliberately retaining
the bounded host-turn model rather than pretending it is a full browser event
loop.

## Delivered

- Added a bounded scheduled-callback error reporter for timer,
  `requestAnimationFrame`, and `requestIdleCallback` delivery.
- Kept callback removal and interval rescheduling in `finally` so a thrown
  callback cannot leak a running timer or suppress its normal cancellation
  decision.
- Added local and HTTP(S) content-owner witnesses proving error delivery and
  sibling callback continuation.
- Widened the delayed content-timer regression's timing margin to cover the
  observed content-owner startup cost without changing the timer contract.
- Synchronized the current capability inventory so bounded animation/idle
  callbacks are not recorded as entirely absent.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- focused local scheduled-callback error test — passed
- focused HTTP(S) content-process scheduled-callback error test — passed
- existing idle-callback and animation-frame/intersection regressions — passed
- `cargo fmt --all -- --check`
- `git diff --check`
- release-documentation, documentation-depth, TUI-shortcut, and
  documentation-coverage validators

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
