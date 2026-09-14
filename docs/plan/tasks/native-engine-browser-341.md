# Native Service Worker registration persistence (341)

Status: implemented locally in the current 0.3.14 checkout.

This slice makes Service Worker registrations survive replacement of the
content-process owner:

- the existing Rust profile stores bounded canonical script URL, scope, and
  classic/module metadata for each registration;
- registration and unregistration update that metadata with the same atomic
  profile commit used for web storage, IndexedDB, and CacheStorage;
- before each HTTP(S) navigation, matching persisted registrations are
  restored through the existing worker script and module/import-graph loader;
- restored workers rebuild an isolated QuickJS realm and listeners without
  replaying install/activate lifecycle events, while their durable CacheStorage
  remains available; and
- a temporarily unavailable persisted script is non-fatal to page loading and
  remains eligible for restoration on a later same-origin navigation.

## Tradeoffs

- The profile stores registration metadata, not source text. Restoration keeps
  one resource-policy path for script freshness, cookies, MIME, redirects, and
  module graphs, but an unavailable script cannot control the page until a
  later retry. The profile itself remains intact instead of being silently
  discarded.
- Restoration is scoped to the document origin being loaded. This avoids
  fetching unrelated origins on every navigation while retaining all profiles
  for later same-origin use.
- Restored active workers skip install/activate replay. That matches a durable
  active registration and avoids duplicating side effects; complete worker
  update/version transitions remain a separate conformance gate.
- Registration metadata and CacheStorage share one bounded profile commit.
  This keeps owner recovery atomic but serializes their writes with the other
  profile state.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_persists_service_worker_cache_across_restart --locked` — 1 passed
- `git diff --check`

The integration witness registers a worker, persists both its cache and
metadata, closes the content process, and starts a new owner. The new owner
reloads the worker script before `/reopen`, exposes an active controller to the
page, and serves the prior cached response without a `/durable` network
request. All evidence is local; this checkout has not been pushed and has no
remote CI result.
