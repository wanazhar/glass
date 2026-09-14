# Native Service Worker Cache API admission and atomic batches (355)

```yaml
id: native-engine-browser-355
scope: native-engine/service-worker-cache-api-conformance
status: done
depends-on: [native-engine-browser-354]
```

## Objective

Close the bounded Service Worker Cache API response-admission and batch-
commit gap in the native browser owner:

- reject response types and statuses that `Cache.put()` and `Cache.addAll()`
  must not store;
- reject `Vary: *` responses at the Cache API boundary;
- fetch bounded `Cache.addAll()` request sets before committing any entry;
- commit a successful `addAll()` through one validated host batch operation;
- make Cache API work during install, activate, and message turns use the same
  Service Worker Fetch/resource policy as page-triggered worker fetches; and
- keep CacheStorage's explicit application-managed lifetime distinct from HTTP
  response freshness.

The platform response-admission and atomicity requirements are anchored to the
[Service Workers specification](https://w3c.github.io/ServiceWorker/). This
slice does not claim complete Cache API conformance, HTTP-cache eviction of
CacheStorage entries, cross-process client messaging, `clients.openWindow()`,
browser-wide registration arbitration, exact task-source scheduling, or final
production certification.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-354.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-355.md`

## Contract and tradeoffs

`Cache.put()` now rejects `error`, `opaque`, and `opaqueredirect` responses,
HTTP status `206`, and responses whose `Vary` header contains `*`. `Cache.add`
and `Cache.addAll` apply the same response-admission rules and require a
successful GET response. `addAll` converts and validates its bounded iterable,
fetches all requests through the existing Service Worker loader, serializes
the responses, and sends one `PutAll` command. The host stages the entries
against the existing cache and replaces the cache only after the whole batch
passes request, response, byte, entry-count, and cache-state validation.

The single host batch makes failure atomic and prevents a later failed fetch or
malformed entry from leaving a partial CacheStorage update. It also increases
peak memory and work for a batch, so the existing request, entry, body, and
aggregate budgets remain hard limits. Promise-based parallel fetching reduces
avoidable serial latency but intentionally retains the bounded `addAll`
contract rather than introducing unbounded concurrency.

Lifecycle and message turns now route worker-owned Fetch commands through the
same loader and Service Worker bootstrap used for ordinary worker Fetch. This
keeps CORS, redirect, credential, cookie, cache-mode, resource, and host-
command policy in one path, at the cost of threading the asynchronous loader
through worker installation, activation, message delivery, and navigation
promotion. CacheStorage itself remains explicit application state; HTTP
`Cache-Control` freshness and eviction continue to belong to the separate
resource-loader cache owner.

The host repeats validation even though the bootstrap validates inputs. This
protects the Rust-owned profile and storage boundary from malformed or
unexpected script commands and keeps direct `put` and batched `putAll`
semantics aligned.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo build --quiet -p glass-browser --bin glass-native-content-worker --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_rejects_partial_and_commits_add_all_atomically --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_matching_options_filter_entries --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked`
- `git diff --check`
- repository documentation coverage, depth, release-truth, and TUI shortcut
  validators

All evidence is local unless a later explicitly authorized push produces
remote CI evidence.

## Local evidence

- `cargo fmt --all -- --check` — passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked` — passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo build --quiet -p glass-browser --bin glass-native-content-worker --locked` — passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_rejects_partial_and_commits_add_all_atomically --locked` — passed; forbidden `206` and `Vary: *` writes reject, a successful batch commits, and a later failing batch leaves the prior keys intact
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_matching_options_filter_entries --locked` — passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — passed; 10 tests, 0 failures
- `git diff --check` — passed
