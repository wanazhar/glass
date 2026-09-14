# Native Service Worker CacheStorage persistence (340)

Status: implemented locally in the current 0.3.14 checkout.

This slice gives the Rust-owned native Service Worker runtime a bounded,
profile-backed CacheStorage subset:

- `caches.open`, `caches.delete`, `caches.has`, and `caches.keys` operate on
  origin-scoped cache names;
- `Cache.match`, `put`, `delete`, `keys`, `add`, and `addAll` operate on
  normalized HTTP(S) GET request keys and preserve response status, headers,
  content type, URL, redirect flags, and byte bodies;
- the host validates cache names, request URLs/methods, response metadata,
  entry counts, per-body bytes, and aggregate profile bytes before committing
  mutations;
- install, activate, fetch, and other settled worker events wait for their
  `waitUntil` promises while the host resolves cache commands in bounded turns;
  and
- content-process startup loads the cache state from the existing atomic Rust
  storage profile, while close and subsequent request turns persist changes
  without creating a separate crate or CDP dependency.

## Tradeoffs

- The implementation deliberately supports exact normalized HTTP(S) GET
  matching first. Unsupported `ignoreMethod` and `ignoreVary` options reject
  explicitly; `ignoreSearch` filtering and full Vary semantics remain a later
  conformance gate rather than silently behaving differently.
- Cache data shares the existing web-storage profile and bounded atomic write
  path. This keeps recovery and profile ownership unified, at the cost of
  serializing cache mutations with other profile updates.
- Service-worker registration metadata is still process-owned. A restarted
  content process must register a worker again, but the worker can immediately
  observe cache entries written by the previous owner. Persisting registrations
  is a separate lifecycle/profile slice.
- The JS resolver returns the Rust response envelope unchanged. Keeping one
  payload shape across `match`, enumeration, name queries, and deletion avoids
  primitive/envelope drift at the host boundary.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_persists_service_worker_cache_across_restart --locked` — 1 passed
- `git diff --check`

The integration witness performs install-time `open`/`put`/`keys`/`has`,
cache-first navigation and fetch matching, profile-file persistence, and a
second content-process startup that serves the cached response without a
network request. All evidence is local; this checkout has not been pushed and
has no remote CI result.
