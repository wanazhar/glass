# Native-engine browser slice 545: FontFace response metadata fidelity

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Preserve native response metadata when a dynamic page-realm `FontFace` source
uses the direct native network loader rather than a Service Worker response.

## Scope

- Carry the final URL, status, status text, content type, exposed headers,
  redirect bit, and bounded body through the direct native font response path.
- Keep font-specific CSP, mixed-content, CORS, cookies, redirect limits, cache
  validation, and byte limits unchanged.
- Preserve `load_font_async` as the byte-only API used by CSS `@font-face`
  loading while projecting the richer response to dynamic `FontFace` fetches.
- Preserve response metadata on fresh font-cache and HTTP 304 revalidation
  hits.

## Contract

- Direct dynamic `FontFace` loads use the same native font admission policy but
  return a `NativeFetchResponse`; the page fetch payload therefore observes
  the final URL and Fetch-exposed metadata instead of a synthetic request URL
  and empty headers.
- Cached successful font responses retain bounded URL, status/status text,
  redirect, content-type, and exposed-header metadata. Fresh hits and 304
  revalidation reuse that response metadata and body after normal cache-policy
  checks.
- Local data/file/blob responses remain bounded synthetic `200 OK` responses;
  Service Worker-handled responses remain unchanged. No CDP fallback or
  unbounded response buffering is introduced.

## Verification

- Direct response metadata test:
  `network_font_response_preserves_url_headers_and_body`; 1 passed.
- Fresh cache metadata and retained byte API test:
  `network_font_cache_reuses_fresh_response_without_refetching`; 1 passed.
- 304 cache regression:
  `network_font_cache_revalidates_with_etag_and_304`; 1 passed.
- Dynamic Service Worker/FontFace integration:
  `native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation`;
  1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- Serialized native library suite: 1269 passed, 1 ignored.
- `cargo check -p glass-dev --lib --bins --locked`, `cargo build -p glass-dev
  --bin glass --locked`, and locked metadata passed.
- `cargo fmt --all -- --check`, documentation depth, coverage, release-truth,
  and `git diff --check` gates passed: 1195 Markdown files, 93 current
  guides, 19 substantive contracts, 63 previous-version hits, 1367
  semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
