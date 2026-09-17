# Native engine browser-complete slice 499: FontFace BufferSource inputs

- Status: complete
- Scope: page-realm `FontFace` `ArrayBuffer` and `ArrayBufferView` sources
- Issue: #40
- Depends on: [native-engine-browser-498](native-engine-browser-498.md)

## Objective

Accept binary font data through the `FontFace` constructor without coercing a
buffer or view into a string. The native page must copy the caller's selected
byte range, preserve it through the existing bounded install command, and
retain the same host admission and rejection guarantees as URL sources.

## Contract

- The constructor recognizes `ArrayBuffer` and `ArrayBufferView` values and
  copies their bytes immediately. A view contributes only its `byteOffset` to
  `byteOffset + byteLength` range, not unrelated prefix or suffix bytes.
- Binary sources bypass URL parsing, fetch, cookies, CORS, CSP, and object-URL
  policy because the bytes are already owned by the page realm.
- `FontFace.load()` rejects empty or oversized binary sources with a bounded
  `NetworkError`; otherwise the copied bytes cross the existing
  `FontFaceInstall` command and transactional host acknowledgement path.
- Mutating or reusing the caller's buffer after construction cannot change the
  bytes submitted for admission. A detached view fails explicitly as a
  constructor `TypeError`.
- The existing 4 MiB font byte limit, descriptor validation, parser admission,
  and no-partial-font-book behavior remain unchanged.

## Implementation

- Detect `ArrayBuffer` and `ArrayBuffer.isView()` constructor inputs before
  string normalization and copy their exact byte ranges into face state.
- Add bounded binary-source validation and route copied bytes through the same
  installation command used by data, Blob, local, and HTTP sources.
- Add a real Noto TTF witness with padded prefix/suffix bytes and a subarray
  view; assert that the emitted command contains exactly the selected font
  bytes and that host admission loads one font resource.

## Tradeoffs and remaining scope

Copying at construction uses memory proportional to the bounded font payload
but avoids mutable-buffer races and makes later loading deterministic. The
existing 4 MiB limit bounds both the copied state and installation payload.
Binary sources do not carry a MIME type, so native parser admission remains
the authoritative format check. Typed-array constructor identity and complete
FontFace/Web IDL descriptors, `tech()` descriptors, richer CSS tokenization,
variable/color negotiation, installed-font discovery, and full cross-realm
parity remain issue #40 gates.

The content-process owner continues to use the same page-realm bootstrap and
FontFaceInstall transport; no new network or process boundary is introduced.
Service Worker interception, final URL/response metadata fidelity, and the
existing standalone content user-event network boundary remain unchanged.

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
group passed 8 tests; the broader font-related group passed 34 tests on a
serial run; and the content-process destination witness passed 1 test.
Formatting and diff checks passed before the task documentation was added.
The documentation gates are run after this task metadata is added.

The implementation remains local-only at this checkpoint: it is not pushed,
run in remote CI, released, tagged, or published.
