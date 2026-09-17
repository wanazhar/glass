# Native engine browser-complete slice 470: bounded media play admission

- Status: complete locally
- Scope: `native-engine` / media play-promise and lifecycle-event admission
- Issue: #40
- Depends on: [native-engine-browser-469](native-engine-browser-469.md)

## Objective

Connect the admitted media-resource state to the first useful `HTMLMediaElement`
playback contract. A resource with validated metadata and a finite duration
must be able to resolve `play()` and enter the playing state; resources that
cannot yet be decoded must reject explicitly rather than appearing to play.

## Contract

- A loaded, error-free media element with a finite duration resolves its
  `play()` promise and dispatches `play` followed by `playing` once when it
  transitions from paused to playing.
- `pause()` transitions the element back to paused and dispatches `pause` only
  when the element was playing.
- Media with a load error, no admitted resource, or unknown duration rejects
  `play()` with `NotSupportedError`; the native path does not claim decoder
  support from MIME admission alone.
- The bounded state remains shared by local fixture and content-process media
  owners through the existing DOM projection. No CDP or network/cache fallback
  is introduced.

## Implementation

- Changed the native media host to validate error, readiness, and finite
  duration before resolving `play()`.
- Added the `play`/`playing` transition and idempotent pause event behavior.
- Added an integration witness for known-duration WAV playback admission and a
  rejection witness for an admitted-but-unknown-duration WebM resource.

## Tradeoffs and remaining scope

This is promise and lifecycle-state admission, not a decoder. It deliberately
does not advance a playback clock, emit `timeupdate`/`ended`, decode PCM or
video frames, produce audio/video output, implement range-backed seeking, or
provide complete media/Web IDL parity. Those capabilities remain explicit
issue #40 work. Keeping the rejection boundary strict prevents a successful
metadata load from becoming a false browser-compatibility claim.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked` (passed)
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_media_play_resolves_for_known_duration` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_media_selects_supported_source` (1 passed)
- `git diff --check`
