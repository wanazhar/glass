# Native Fetch and XHR response freshness (349)

Status: implemented locally in the current 0.3.14 checkout.

This slice extends bounded HTTP response freshness to page Fetch, XHR, and
worker Fetch through the native resource owner:

- GET and HEAD responses with explicit Cache-Control/Pragma policy or
  ETag/Last-Modified validators use a bounded response cache; POST and other
  body-bearing requests retain the existing network path;
- the cache key partitions by owner origin, target URL, method, request
  headers, credentials, same-origin/CORS visibility, and the current cookie
  header so a response cannot cross an incompatible security partition;
- Request.cache supports default, no-store, reload, no-cache, force-cache, and
  only-if-cached. Stale validator responses issue conditional requests and a
  validated 304 reuses the stored body, while unsafe no-store, Vary, and
  response-cookie representations are not retained; and
- eligible responses are collected into the bounded cache, while responses
  without cache metadata remain incremental streams. Worker and Service Worker
  Fetch use the same owner and do not bypass native policy.

## Tradeoffs

- Only explicit cache metadata or validators create entries. Heuristic
  Expires/Date age correction, shared cache-age accounting, persistent disk
  storage, and request coalescing remain outside this bounded owner.
- The cache is scoped to the current native content owner and is not mixed with
  Service-Worker CacheStorage. A Service Worker intercepted response remains
  owned by its explicit Cache API path.
- Cross-origin readable responses retain their CORS/credential partition, while
  same-origin CORS and same-origin modes share their equivalent visibility.
  Opaque responses and partial responses are never stored.
- Cacheable responses are fully buffered before delivery so they can be reused;
  uncached responses preserve the existing incremental stream path and its
  response-size limit.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_revalidates_fetch_and_xhr_responses --locked` — 1 passed
- `git diff --check`

The HTTP(S) witness verifies fresh reuse without a second request, forced
reload, conditional no-cache/304 reuse, no-store network isolation, force-cache
and only-if-cached reads, and a credential-compatible XHR read from the same
native response cache. All evidence is local; this checkout has not been pushed
and has no remote CI result.
