# Native-engine browser slice 554: FontFaceSet event handlers

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the missing `FontFaceSet` event-handler IDL surface for the native
page-realm font owner while preserving the existing loading event dispatch
and keeping font-display timing as a separate gate.

## Scope

- Expose persistent `document.fonts.onloading`, `onloadingdone`, and
  `onloadingerror` handler properties on the native `FontFaceSet` instance.
- Route each assigned callable through the existing bounded EventTarget listener
  owner, replace the prior callable transactionally, and remove it when set to
  `null` or a non-callable value.
- Install the properties only once across repeated page bootstrap refreshes so
  runtime state and handler identity survive host snapshot updates.
- Keep actual font-display block/swap/fallback/optional timing, installed-font
  discovery, media output, variable-axis completeness, hinting, and complete
  FontFace/Web IDL parity as separate issue #40 gates.

## Contract

- Each handler property returns the assigned callable or `null`; non-callable
  assignments clear the handler. Reassignment removes the previous listener
  before registering the replacement.
- Native `loading`, `loadingdone`, and `loadingerror` dispatches invoke the
  corresponding handler with the same event object and `fontfaces` payload
  delivered to `addEventListener` listeners.
- Repeated document bootstrap does not redefine or discard the properties.
  No new command, wire field, dependency, CDP path, or fallback is introduced.

## Implementation

- `javascript.rs` installs guarded handler properties after the persistent
  `FontFaceSet` is created, reusing the existing EventTarget handler-property
  helper and owner/listener tables.
- The static CSS refresh regression covers generated loading/loadingerror
  events through both listener styles; a dedicated regression covers handler
  replacement and removal.

## Verification

- Static generated-event dispatch passed:
  `css_font_face_refresh_dispatches_loading_error_for_new_rules`.
- Handler replacement/removal passed:
  `font_face_set_handler_properties_replace_and_remove`.
- The locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1283 passed, 1 ignored, and 1282 filtered across
  the two emitted test suites.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1204 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1204 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.
