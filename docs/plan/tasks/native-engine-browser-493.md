# Native engine browser-complete slice 493: page FontFaceSet projection

- Status: complete
- Scope: page-realm `FontFace`/`FontFaceSet` projection for CSS faces
- Issue: #40
- Depends on: [native-engine-browser-492](native-engine-browser-492.md)

## Objective

Expose the CSS font faces already admitted by the native style/resource owner
through the page's standard `FontFace` and `FontFaceSet` surfaces. The objects
must remain observable across repeated document bootstraps, report the host's
bounded loaded/error state, and expose the set lifecycle needed by ordinary
page code.

## Contract

- The host serializes a bounded descriptor for each parsed CSS `@font-face`
  rule, including family, normal/bold weight, normal/italic style, and the
  admitted loaded/error result.
- `document.fonts` is a persistent page-realm `FontFaceSet`; CSS faces retain
  declaration order across document snapshots and are removed when their
  descriptor disappears.
- `FontFace` exposes its family, standard descriptor members, status, loaded
  promise, and `load()`; CSS faces use the host-admitted result.
- `FontFaceSet` exposes status, ready promise, size, family-aware `check()` and
  `load()`, add/delete/clear, `forEach`, keys/values/entries, and iteration.
- New or changed CSS descriptors dispatch bounded `loading`, `loadingdone`,
  and/or `loadingerror` events with the affected `fontfaces` list.
- Unknown families preserve the fallback result (`check()` true and `load()`
  resolving an empty list); a matching host error makes `check()` false and
  load reject through the face's error state.
- Script-created faces can be added to a set, but their source load is
  explicitly rejected with `NotSupportedError` until a native loader command
  can validate and admit that source. No false rendering or network behavior
  is claimed.

## Implementation

- Add a serialized Rust descriptor projection from `NativeDocument`.
- Install a persistent bounded `FontFace`/`FontFaceSet` implementation in the
  page bootstrap after the event-target helpers are available.
- Reconcile CSS descriptors after `document` is installed so lifecycle events
  have a valid event owner and existing listeners survive refreshes.
- Add runtime witnesses for CSS-face status, set mutation/iteration, fallback
  checks, and refresh lifecycle events.

## Tradeoffs and remaining scope

The set projects the native owner rather than reimplementing font loading in
JavaScript, which keeps policy, caching, and admitted bytes under Rust
ownership. The current script-created `FontFace` path therefore fails closed
until it has a request/response command contract; pretending that a JS source
was rendered would make `status`, `loaded`, and paint disagree. The slice does
not claim platform-wide installed-font discovery, `font-display` timing,
variable or color-font tables, cross-realm frame exposure, or complete text
and Web IDL parity.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture` (26 passed)
- existing native JavaScript bootstrap smoke (1 passed)
- `cargo fmt --all -- --check`
- `git diff --check`

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
