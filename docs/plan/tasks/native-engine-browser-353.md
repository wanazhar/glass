# Native Service Worker top-level client control (353)

```yaml
id: native-engine-browser-353
scope: native-engine/service-worker-client-control
status: done
depends-on: [native-engine-browser-352]
```

## Objective

Make the current top-level Service Worker client contract explicit instead of
deriving control from registration scope alone:

- assign a bounded identity to each native document client;
- keep an uncontrolled page's ordinary Fetch/XHR requests on the network even
  when a registration scope matches the request target;
- let a matching navigation be intercepted by an active worker and establish
  control for the newly committed document;
- honor `clients.claim()` after activation for the current client;
- route controlled-client requests through the registration that controls the
  client, including requests whose target URL is outside that registration's
  scope; and
- expose the same explicit control and identity through registration state,
  `navigator.serviceWorker.controller`, `controllerchange`,
  `clients.matchAll()`, and `FetchEvent.clientId`.

This slice deliberately covers the current native content process's one
top-level client. Multiple tabs, frames, client focus/visibility ownership,
exact task-source scheduling, and full Service Worker conformance remain
issue #40 promotion gates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-352.md`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-353.md`

## Contract and tradeoffs

The registry owns a per-document opaque client identity and a current
controlling registration scope. A new document starts uncontrolled; a
successful navigation commits control from the active registration matching
the final URL, and an activating worker's validated `clients.claim()` may
claim the current in-scope client. Non-navigation interception uses that
client controller, not the request target's scope, so cross-origin and
out-of-scope target requests preserve controlled-client routing.

Page-facing registration snapshots carry a non-persistent `controlled` bit.
The durable profile still stores registration and waiting-worker descriptors,
not client identity or control state; restart creates a new client and derives
its control from the committed URL and restored active registrations. The
owner remains single-client and serialized, so no multi-client claims are
implied by this slice.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked`
- `git diff --check`
- repository documentation coverage, depth, release-truth, and TUI shortcut
  validators

All evidence is local unless a later explicitly authorized push produces
remote CI evidence.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — 8 passed
- `python3 scripts/check-documentation-coverage.py` — passed
- `python3 scripts/check-documentation-depth.py` — passed
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed
- `python3 scripts/check-tui-shortcuts.py` — passed
- `git diff --check`
