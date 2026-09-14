# Native stylesheet and page-script cache freshness (348)

Status: implemented locally in the current 0.3.14 checkout.

This slice extends bounded response freshness to HTTP(S) text subresources:

- stylesheet and page-script responses have separate bounded caches so a CSS
  representation cannot satisfy a JavaScript load at the same URL;
- explicit `Cache-Control`/`Pragma` policy and bounded `ETag`/
  `Last-Modified` metadata control fresh reuse and conditional requests;
- stale stylesheet and page-script loads send validators only on the original
  request, and a validated `304 Not Modified` reuses the stored text without
  attempting content-type or body decoding; and
- requested and final redirect keys remain bounded and safe, while worker
  source loading deliberately bypasses the page-script cache so worker and
  Service-Worker update fetches are not frozen by page cache state.

The no-header behavior remains an uncached text-subresource load, preserving
the existing worker/update behavior and avoiding an invented indefinite cache
policy for resources that did not declare freshness. This slice does not alter
CSS/script parsing, execution ordering, or the existing resource security
checks.

## Tradeoffs

- Only responses with explicit cache policy or validators enter the text
  caches. Heuristic `Expires`/`Date` age correction, shared cache age
  accounting, and request coalescing remain outside this bounded owner.
- A validator is sent only on the first request in a redirect chain. A `304`
  after a redirect is rejected rather than applied to the wrong requested or
  final URL key.
- `no-store`, `Vary: *`, `Vary: Cookie`, and response cookies prevent or evict
  reusable text entries. The native owner therefore does not silently reuse a
  representation whose server response declares privacy-sensitive variance.
- Module graph caching, stylesheet `@import` freshness, Fetch/XHR response
  caching, Service-Worker CacheStorage freshness, and complete HTTP cache
  conformance remain issue #40 promotion work.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_revalidates_stylesheet_and_script_subresources --locked` — 1 passed
- `git diff --check`

The HTTP(S) witness loads two copies each of one stylesheet and one classic
page script. Each first request receives `200` with `no-cache` and an `ETag`;
each second request must carry `If-None-Match` and receives `304`. The native
engine still applies the stylesheet and executes the script twice. All
evidence is local; this checkout has not been pushed and has no remote CI
result.
