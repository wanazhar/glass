# Native engine browser-complete slice 496: FontFace host admission acknowledgement

- Status: complete
- Scope: page-realm `FontFaceInstall` host acknowledgement
- Issue: #40
- Depends on: [native-engine-browser-495](native-engine-browser-495.md)

## Objective

Keep a script-created `FontFace` in `loading` until the native document owner
has admitted its bounded bytes. The page promise must not report `loaded` merely
because JavaScript emitted a transport command; the document parser, aggregate
font limits, and font book must accept the command first.

## Contract

- The page realm keeps one bounded pending-install record per positive request
  id and emits `fontFaceInstall` before the face's `load()` promise settles.
- The native owner applies the install command to the document clone first,
  then dispatches a private acknowledgement into the same page realm. A
  successful acknowledgement releases the face promise and the associated
  `FontFaceSet` loading cycle.
- Promise continuations released by an acknowledgement are evaluated and
  applied in bounded follow-up batches. DOM effects are committed before the
  owner returns; page-script owners retain the supported fetch, WebSocket,
  EventSource, scroll, and navigation follow-ups.
- The acknowledgement protocol is used by initial and dynamic page scripts,
  content-process script mutations, and local native-engine script/event paths.
  A runtime without JavaScript retains the existing direct document-command
  path.
- The page bootstrap keeps an error-capable resolver for host admission
  failures. The current owner acknowledges only after successful document
  admission; an admission error aborts the owning mutation instead of marking
  the face loaded or mutating the font book partially.

## Implementation

- Add the page dispatch variant and runtime resolver for
  `__glassResolveFontFaceInstall`.
- Add a shared document-command helper that detects install requests, applies
  each batch, acknowledges admitted requests, drains bounded Promise
  continuations, and returns follow-up commands to the owner.
- Thread the runtime/document URL/origin/viewport through page-script
  evaluation and replace direct document application in the content-process
  and local native-engine paths that can observe page work.
- Record follow-up local history/scroll state and reject unsupported local
  network transport using the existing two-owner boundary.
- Add inline-data, local-system-font, and content-process destination
  witnesses through the real QuickJS/document/resource paths.

## Tradeoffs and remaining scope

The acknowledgement is deliberately host-owned: a face cannot become loaded
until the document font parser and aggregate resource owner accept its bytes.
The current success path uses an internal `{ ok: true }` payload; a future
failure-reporting slice can use the existing error branch to settle a page
promise as `error` when the host can return a recoverable admission failure.

Acknowledgement continuations are applied recursively, but arbitrary network
continuations from standalone content user-event mutations remain outside that
mutation's existing DOM/event response contract. Page-script and dynamic-script
owners route their supported network/navigation effects; expanding every event
transport to carry arbitrary continuation effects remains an issue #40 gate.
Service Worker interception for dynamic FontFace requests, final URL and
response-header fidelity, ArrayBuffer sources, source lists, `format()`
descriptors, installed-font discovery, font-display timing, variable/color
tables, cross-realm projection, and complete FontFace/Web IDL parity remain
open issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Observed results: the scoped library check passed; the focused JavaScript
FontFace group passed 5 tests; the content-process font destination witness
passed 1 test; and the broader font group passed 31 tests on serial, two-worker,
and final default-parallel reruns. One earlier default-parallel run had a
single resource-pressure failure before the same group passed on rerun.
Formatting and diff checks passed. Documentation gates are run after this
task's metadata is added.

The implementation remains local-only at this checkpoint: it is not pushed,
run in remote CI, released, tagged, or published.
