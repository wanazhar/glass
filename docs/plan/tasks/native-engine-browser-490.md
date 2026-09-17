# Native engine browser-complete slice 490: external @font-face resources

- Status: complete
- Scope: `native-engine` / file, Blob, and HTTP(S) font-face sources
- Issue: #40
- Depends on: [native-engine-browser-489](native-engine-browser-489.md)

## Objective

Extend the bounded document-owned `@font-face` path from embedded data URLs to
the existing file, runtime-owned Blob, and HTTP(S) resource owners. Preserve
the native engine's explicit policy and failure behavior: unsupported or
blocked font sources fall back to the existing font book and never become a
silent success or a CDP handoff.

## Contract

- The CSS parser retains the first valid `url(...)` source from a `src`
  descriptor. Relative URLs are resolved against the document or final
  stylesheet URL by the existing URL owner; absolute `file:`, `http:`,
  `https:`, `blob:`, and `data:` URLs are admitted as candidates. `local()`
  remains a skip candidate rather than arbitrary installed-font discovery.
- File fonts are available only to `file:` documents and must resolve inside a
  canonical configured allowed-file root. Bytes are read with the existing
  4 MiB per-face bound; filesystem MIME inference is not required because the
  font parser validates the actual bytes.
- Blob fonts are available through the JavaScript realm's object-URL resource
  transfer. The object URL must belong to the resolved Blob origin, carry an
  admitted font MIME type when present, and remain within the 4 MiB bound.
- HTTP(S) fonts use a bounded GET stream with at most eight redirects. Every
  target is checked for credentials, mixed content, `font-src` CSP and
  report-only violations, and network URL validity. Same-origin cookies and
  referrers use the existing network state; cross-origin responses require
  `Access-Control-Allow-Origin` authorization.
- HTTP(S) responses must be successful, must not exceed 4 MiB from
  `Content-Length` or streamed chunks, and must carry a supported font MIME
  type when a `Content-Type` header is present. Empty bodies and invalid font
  bytes fall back without aborting the document.
- Initial and dynamic stylesheet loading use the new loader in both the
  content-process and inline owners. Admitted bytes are rebuilt into the
  existing document-local font book ahead of system faces and continue through
  the existing metrics, shaping, raster, display-list, and PNG replay paths.

## Implementation

- Broaden bounded `@font-face` URL candidate parsing without giving CSS parser
  code filesystem or network capabilities.
- Add synchronous file/data/Blob font admission and an asynchronous HTTP(S)
  font stream to `NativeResourceLoader`.
- Reuse `NativeSubresourceKind::Font` CSP reporting and the existing redirect,
  CORS, cookie, referrer, and response-chunk resource boundaries.
- Thread runtime-owned object URLs into local and content-process dynamic
  stylesheet rebuilds; load initial network faces in the content process.
- Keep custom resource bytes document-owned and transfer only the validated
  bounded representation already used by protocol v13.

## Tradeoffs and remaining scope

Network font bytes are not yet stored in a dedicated font cache, so repeated
document rebuilds may perform another bounded request; this keeps the slice
small and avoids inventing cache-key semantics before CSS font loading state is
implemented. File fonts intentionally require a file document, and Blob fonts
require a live realm transfer; neither path widens fixture or arbitrary
filesystem access. `local()` resolution, `FontFace`/`FontFaceSet` loading
events, font-display timing, variable-font instances, color fonts, format
descriptors, language/script matching, mixed bidi/writing modes, and complete
browser text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_` (16 passed)
- rooted file and runtime-owned Blob admission are covered by the focused
  loader test
- HTTP(S) font admission with CORS and bounded bytes is covered by the focused
  asynchronous loader test
- process/listener audit found no stale Glass, Cargo, rustc, Chromium,
  Firefox, or native-content-worker targets to terminate
- documentation truth/depth/shortcut/coverage audits (pending final docs gate)
- `git diff --check` (pending final docs gate)

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
