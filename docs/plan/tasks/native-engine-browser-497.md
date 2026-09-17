# Native engine browser-complete slice 497: FontFace admission rejection

- Status: complete
- Scope: page-realm `FontFaceInstall` admission failure acknowledgement
- Issue: #40
- Depends on: [native-engine-browser-496](native-engine-browser-496.md)

## Objective

Make a rejected script-created `FontFace` settle as an error when the native
document owner cannot admit its bounded bytes. A page promise must not remain
permanently pending, report `loaded`, or leave a partially installed font
behind when parser or aggregate-resource validation rejects the install.

## Contract

- A batch containing one or more `FontFaceInstall` commands is applied to a
  cloned document first. The clone becomes the document only after every font
  install in the batch is admitted successfully.
- A successful batch receives the existing `{ ok: true }` acknowledgement
  after the document commit. No acknowledgement is emitted before the font
  book has accepted the bytes.
- If the batch fails specifically during FontFace admission, the owner keeps
  the original document unchanged and acknowledges the pending request with a
  bounded error payload. The page `FontFace` transitions to `error`, its
  `load()` promise rejects, and the owning `FontFaceSet` loading cycle settles
  with the same failure.
- The admission-failure classifier replays only the font-install commands on a
  document probe. Errors from unrelated DOM/script commands still propagate
  normally instead of being misreported as font failures.
- Both the page-script evaluator and the shared local/content-process
  document-command helper use the same transactional acknowledgement behavior.
  Existing bounded continuation and owner handoff limits remain in force.

## Implementation

- Add a document helper that probes all `FontFaceInstall` commands against a
  cloned `NativeDocument` using the existing parser, byte-limit, descriptor,
  and font-book validation path.
- Make acknowledged script-command batches transactional and select success
  or rejection payloads only after the owner has classified the result.
- Route both the host acknowledgement rejection branch and the source-fetch
  rejection branch through one page-realm failure finalizer. It updates face
  state, replaces the settled promise with a rejected promise, and completes
  the bounded `FontFaceSet` loading cycle.
- Add a real page-evaluation witness using an invalid data font, asserting the
  `NetworkError:error` result and the absence of a font-book resource.

## Tradeoffs and remaining scope

The clone-and-commit transaction adds bounded document-copy and parser work
only to command batches that carry a FontFace install. That cost prevents
partial mutation when a later font in the same batch fails and keeps the
acknowledgement contract deterministic. The error payload intentionally uses a
stable internal message; the page-facing projection currently maps admission
failure to `NetworkError` rather than exposing native parser details.

The shared helper still preserves the existing response boundary: page-script
owners route supported fetch, WebSocket, EventSource, scroll, and navigation
continuations, while arbitrary network effects from standalone content
user-event mutations remain outside that mutation's response contract. Service
Worker interception, final URL and response-header fidelity, ArrayBuffer and
source-list inputs, `format()` descriptors, installed-font discovery,
font-display timing, variable/color tables, cross-realm projection, and full
FontFace/Web IDL parity remain open issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Observed results: the scoped library check passed; the focused native FontFace
group passed 6 tests; and the broader FontFace-related group passed 32 tests
on a serial run. The content-process destination witness and documentation
gates are run after this task metadata is added. Formatting and diff checks
passed before the task documentation was added.

The implementation remains local-only at this checkpoint: it is not pushed,
run in remote CI, released, tagged, or published.
