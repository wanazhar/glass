# Native document cache freshness and revalidation (346)

Status: implemented locally in the current 0.3.14 checkout.

This slice closes the bounded HTTP document-cache freshness boundary in the
native resource loader:

- document cache entries retain a bounded freshness deadline derived from
  `Cache-Control: max-age`, while `no-cache`, `max-age=0`, and `Pragma:
  no-cache` remain stored representations that require a network check;
- `ETag` and `Last-Modified` validators are retained only within bounded
  response-header limits and are sent on the original stale document request;
- a validated `304 Not Modified` reuses the stored HTML representation,
  refreshes its cache metadata, applies any response cookies, and preserves the
  original navigation fragment; and
- `no-store`, `Vary: *`, `Vary: Cookie`, and responses that set cookies are not
  retained as reusable document entries, and an old entry is evicted when a
  replacement response is not safe to store.

The pre-existing no-header bounded session-cache behavior remains intact for
compatibility with the native navigation contract. This slice is scoped to
top-level document navigation; image, script, stylesheet, Fetch, and
Service-Worker CacheStorage freshness still require their own response-policy
owners before the native engine can claim complete HTTP-cache parity.

## Tradeoffs

- Freshness uses monotonic `Instant` deadlines and the explicit
  `Cache-Control` directives supported by this bounded owner. It does not yet
  implement heuristic `Expires`/`Date` freshness, age correction, or shared
  cache age accounting.
- Validators are attached only to the first request in a redirect chain, so a
  validator for one origin is never forwarded to a redirected origin. A `304`
  after a redirect is rejected rather than applied to the wrong cached key.
- A `304` with `no-store` or cookie variance evicts the old entry after
  applying its cookies. This keeps a response-controlled privacy boundary from
  silently widening the lifetime of a previously cacheable representation.
- Concurrent request coalescing, complete subresource response caching,
  partitioned cache ownership, and full HTTP cache conformance remain issue
  #40 promotion work.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_revalidates_stale_http_cache_with_etag --locked` — 1 passed
- `git diff --check`

The HTTP(S) witness serves one `200` response with `Cache-Control: no-cache`
and an `ETag`, then requires `If-None-Match` on the second navigation and
serves `304`; the native engine preserves the title and visible body without
receiving a second HTML representation. All evidence is local; this checkout
has not been pushed and has no remote CI result.
