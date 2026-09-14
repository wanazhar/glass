# Native image cache freshness and revalidation (347)

Status: implemented locally in the current 0.3.14 checkout.

This slice extends the native response policy to decoded external images:

- image-cache entries retain bounded `Cache-Control` freshness metadata and
  bounded `ETag`/`Last-Modified` validators;
- stale image entries send validators only on the original request, and a
  validated `304 Not Modified` reuses the decoded image without attempting
  MIME or body decoding;
- `no-cache`, `max-age=0`, and `Pragma: no-cache` responses can be retained for
  revalidation, while `no-store`, `Vary: *`, `Vary: Cookie`, and response
  cookies prevent reusable image state; and
- requested and final redirect keys continue to share the decoded image on a
  successful safe response, while unsafe replacement responses evict stale
  keys.

The existing no-header process-lifetime image cache and bounded duplicate-image
behavior remain compatible. This slice is limited to decoded external images;
stylesheet, script, Fetch, and Service-Worker CacheStorage freshness still
require their own response-policy owners.

## Tradeoffs

- Image freshness uses the same monotonic explicit `Cache-Control` subset as
  document navigation. Heuristic `Expires`/`Date` age correction and shared
  cache age accounting remain outside this bounded owner.
- Validators are attached only to the first request in a redirect chain. A
  `304` after a redirect is rejected rather than applied to the requested or
  final key incorrectly.
- A `304` carrying `no-store`, cookie variance, or a response cookie evicts
  the old decoded entry after cookie application, preserving the response
  privacy boundary.
- Concurrent image request coalescing, responsive image selection, CSS image
  layers beyond the existing path, and complete HTTP cache conformance remain
  issue #40 promotion work.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_revalidates_no_cache_external_png_with_etag --locked` — 1 passed
- `git diff --check`

The HTTP(S) witness serves a page with two identical external PNGs. The first
image response is `200` with `Cache-Control: no-cache` and an `ETag`; the
second request must carry `If-None-Match` and receives `304`, while both image
display commands are produced. All evidence is local; this checkout has not
been pushed and has no remote CI result.
