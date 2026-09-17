# Native engine browser-complete slice 494: script FontFace source admission

- Status: complete
- Scope: page-realm script-created `FontFace.load()` source admission
- Issue: #40
- Depends on: [native-engine-browser-493](native-engine-browser-493.md)

## Objective

Make script-created page `FontFace` objects useful for bounded native
rendering. A face added to `document.fonts` must be able to load a supported
source, report its settled state, and transfer the admitted bytes into the
same document-owned font book used by CSS `@font-face` resources.

## Contract

- A bounded string source must contain one `url(...)` or `local(...)` function.
- `local(...)` performs a case-insensitive, weight/style-aware lookup in the
  deterministic system font book. A missing local face rejects with
  `NetworkError` and does not expose host filesystem paths to the page.
- `data:` font URLs support bounded base64 and percent-encoded bytes. Runtime
  Blob URLs use the existing object-URL registry and require a supported font
  media type.
- Other URL sources use the existing native Fetch event-loop bridge with
  same-origin credentials and CORS mode. Successful responses require a 2xx
  response, supported font media type, non-empty bytes, and the 4 MiB per-face
  limit.
- Successful source resolution emits one bounded `fontFaceInstall` command
  containing the family, normal/bold descriptor pair, and base64 bytes. The
  native document validates the request id, metadata, aggregate resource
  limits, base64 envelope, and font parser before rebuilding its font book.
- `load()`, `loaded`, `status`, and FontFaceSet loading lifecycle state settle
  from the source resolution result. Repeated loads reuse the settled promise.
- The existing CSS-face projection and dynamic set membership remain intact;
  dynamic resources are appended to the document resource owner and therefore
  participate in native shaping, rasterization, capture, and hit-test output.

## Implementation

- Add the `NativeScriptCommand::FontFaceInstall` transfer and consume it in
  `NativeDocument`.
- Add the bounded native font-byte parser admission check without changing the
  historical wire-snapshot behavior that skips malformed static entries.
- Extend the page bootstrap with source parsing, data/blob decoding, local
  system-face lookup, URL response checks, promise/lifecycle updates, and the
  command envelope.
- Increase response-body decoding to use the existing bounded form-response
  limit so a fetched font can cross the page response projection before the
  stricter 4 MiB font command limit is applied.
- Add inline-data and local-system-face witnesses through the real JavaScript
  runtime, document command application, and font-resource wire projection.

## Tradeoffs and remaining scope

The page realm currently reuses Fetch for URL sources. This preserves the
existing event-loop, CORS, response, cache, cookie, and Service Worker wiring,
but its CSP decision is `connect-src` rather than the dedicated `font-src`
decision already used by CSS font loading. The install command also has no
host acknowledgement after the realm marks the face loaded; a host-side
admission failure therefore surfaces at the command application boundary
rather than changing the already-settled page promise. A follow-up slice must
provide a dedicated font request/response path and acknowledgement.

The bounded source contract is string-oriented; `ArrayBuffer` and
`ArrayBufferView` FontFace sources, source lists, `format()` descriptors,
font variation ranges, platform-wide installed-font discovery, font-display
timing, variable/color font tables, cross-realm FontFace projection, and
complete text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Observed results: the focused page batch passed 4 tests; the broader font
batch passed 28 tests; the scoped library check and all documentation gates
passed with no diagnostics.

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
