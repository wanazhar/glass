# Native engine browser-complete slice 471: media timeline progression

- Status: complete locally
- Scope: `native-engine` / bounded media clock, played ranges, and completion
- Issue: #40
- Depends on: [native-engine-browser-470](native-engine-browser-470.md)

## Objective

Make the admitted finite-duration media state useful across multiple host
turns. `currentTime`, `played`, pause/resume, playback-rate changes, seeking,
and end-of-resource state must share one live clock rather than remaining a
static metadata projection.

## Contract

- A playing finite-duration media element advances `currentTime` from the
  realm's monotonic `performance.now()` clock and clamps at its duration.
- `pause()` freezes the current position; a later `play()` resumes from that
  position, while a completed resource restarts from zero.
- `playbackRate` rebases an active clock without losing elapsed position.
- Setting `currentTime` performs bounded seeking, emits `seeking`,
  `timeupdate`, and `seeked`, and rebases an active playback clock.
- `played` exposes merged, ordered intervals accumulated by playback;
  `buffered` and `seekable` retain the admitted full-duration range.
- Reaching the finite duration sets `ended`, returns to paused, and dispatches
  one `ended` event. Unknown-duration and decoder-unavailable resources retain
  the explicit `NotSupportedError` play rejection.

## Implementation

- Added a realm-owned media clock anchored to native `performance.now()`.
- Added interval merging for played ranges and distinct buffered/seekable
  projections.
- Added pause/resume, seek, playback-rate rebasing, and end-of-duration event
  transitions.
- Added integration coverage for elapsed time, played-range accumulation,
  accelerated completion, and the `ended`/paused terminal state.

## Tradeoffs and remaining scope

The clock is observable and deterministic relative to the native host clock,
but it does not yet schedule independent media task-source turns when the page
is otherwise idle. It also does not decode PCM/video frames, route audio to an
output device, render video frames, implement byte-range transport, or claim
complete media/Web IDL parity. Those remain issue #40 production work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked` (passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked media` (8 passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked native_local_dynamic_blob_media_play_resolves_for_known_duration` (1 passed)
- `git diff --check`
