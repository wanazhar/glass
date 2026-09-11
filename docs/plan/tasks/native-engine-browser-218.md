# Native engine browser slice 218: image event handler properties

Status: completed locally.

## Objective

Support the normal `HTMLImageElement` handler-property form for resource
events. Page code using `image.onload = callback` or
`image.onerror = callback` must observe the same native resource lifecycle as
code using `addEventListener`.

## Contract

- `IMG` projections expose writable `onload` and `onerror` properties whose
  default value is `null`.
- Assigning a callable handler installs one target-local listener for that
  event; assigning a different callable replaces the previous handler;
  assigning `null` or a non-callable value removes it and leaves the property
  as `null`.
- Handler callbacks receive the same event object and target as
  `addEventListener`. Image `load` and `error` remain non-bubbling and
  non-cancelable, and handler replacement does not remove independent
  `addEventListener` listeners.
- Initial and script-mutated external image attempts use the existing typed
  terminal resource-event path. Handler callbacks run in the owning page realm
  without exposing loader or decoder error details.
- No executable handler or page string crosses the content-process boundary;
  the existing host-side event metadata remains the only cross-process input.

## Implementation

`javascript.rs` adds a small event-handler-property installer backed by the
existing listener registry and owner lookup. `IMG` setup installs `onload` and
`onerror`; the registry's removal/replacement behavior preserves independent
listeners and the persistent element identity across authoritative snapshot
refreshes. The external image error witness now exercises initial delivery,
script-driven source replacement, handler replacement, and the retained
`complete`/`currentSrc` lifecycle state.

## Tradeoffs and follow-up

This slice intentionally covers only the two image resource handler
properties. It does not compile inline `onload`/`onerror` content attributes,
add media/script/frame handler families, expose handler reflection on every
`EventTarget`, or claim complete Web IDL descriptor/prototype parity. Handler
assignment remains synchronous and bounded by the existing listener limit; a
future general event-handler owner should reuse this mechanism rather than
duplicating per-element slots.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_external_image_error_event -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `26fcbf71`.

The broader Issue #40 browser profile, event-handler families, and native-only
promotion gates remain active work.
