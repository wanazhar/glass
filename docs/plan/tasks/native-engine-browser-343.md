# Native Service Worker CacheStorage matching options (343)

Status: implemented locally in the current 0.3.14 checkout.

This slice expands the Rust-owned CacheStorage matching contract:

- `Cache.match()`, `Cache.delete()`, `Cache.keys()`, and `caches.match()` carry
  the `ignoreSearch`, `ignoreMethod`, and `ignoreVary` options through the
  native owner;
- cached entries retain bounded request headers so response `Vary` fields are
  compared against the request that was stored;
- query-insensitive matching removes the URL search component, method-ignored
  matching can find a stored GET for a non-GET request, and Vary-ignored
  matching bypasses Vary comparison;
- default Vary matching rejects `Vary: *` and requires each selected request
  header to match; and
- filtered `Cache.keys()` returns the original request URL, method, and headers
  while the existing GET-only `Cache.put()` boundary remains enforced.

## Tradeoffs

- CacheStorage still persists only entries created by GET `Cache.put()`. The
  `ignoreMethod` option makes non-GET lookup/delete behavior useful without
  pretending the bounded store can persist arbitrary request bodies.
- Matching parses both stored and candidate URLs for deterministic query
  removal and fragment normalization. This keeps the Rust owner independent of
  JavaScript URL string quirks within the existing URL and entry limits.
- Vary matching uses the bounded request-header snapshot captured at `put()`;
  `Vary: *` is a miss unless `ignoreVary` is explicit. HTTP freshness,
  revalidation, opaque-response persistence, and broader Cache Web IDL parity
  remain separate issue #40 gates.
- `Cache.keys()` returns filtered request metadata rather than exposing the
  profile directly. The profile remains Rust-owned, bounded, and backward
  compatible with entries written before request headers were added.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_matching_options_filter_entries --locked` — 1 passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_persists_service_worker_cache_across_restart --locked` — 1 passed
- documentation coverage/depth/release and TUI shortcut validators passed
- `git diff --check`

The integration witness stores query- and Vary-sensitive entries, exercises
exact, query-insensitive, method-insensitive, and Vary-insensitive lookup,
checks filtered keys, deletes a matching entry, and confirms the worker serves
the result without a network request. All evidence is local; this checkout has
not been pushed and has no remote CI result.
