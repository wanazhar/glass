# Native-engine browser slice 543: Service Worker FontFace interception

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Route dynamic page-realm `FontFace` network sources through the controlling
native Service Worker before falling back to the direct font loader.

## Scope

- Dispatch private `font` fetches to the existing Service Worker interception
  owner when the document is network-controlled.
- Preserve `font-src`, mixed-content, report-only CSP, response status,
  headers/content type, final URL, and bounded response-body behavior for
  intercepted responses.
- Expose the internal font destination on the Service Worker `Request` object
  so handlers can distinguish font requests from ordinary fetches.
- Keep file/data/blob and uncontrolled requests on the existing direct native
  loader path, and preserve the existing admission acknowledgement.

## Contract

- A controlled `FontFace` request is sent to the Service Worker with
  `Request.destination === "font"`; a handled response is projected with its
  native response metadata and body. A non-handled request uses the existing
  `load_font_async` policy and cache path.
- Service Worker font requests use document `font-src` rather than
  `connect-src`, including report-only violation recording. Suspended Service
  Worker font requests fail through the existing bounded worker error path.
- The private destination is host-authored and bounded to the existing
  `font`/`fetch`/`document` request contexts; page fetch callers cannot select
  arbitrary destinations.
- No CDP fallback, unbounded response buffering, or change to native font
  parser/admission semantics is introduced.

## Verification

- Intercepted Service Worker font integration:
  `native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation`;
  1 passed.
- Direct loader fallback:
  `content_process_font_destination_uses_the_native_font_loader`; 1 passed.
- Service Worker font CSP policy test: 1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- Serialized native library suite: 1268 passed, 1 ignored.
- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo build -p glass-dev --bin glass --locked` passed.
- `cargo metadata --no-deps --format-version 1 --locked` passed.
- `cargo fmt --all -- --check`, documentation depth, coverage,
  release-truth, and `git diff --check` gates passed: 1193 Markdown files,
  93 current guides, 19 substantive contracts, 63 previous-version hits,
  1367 semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
