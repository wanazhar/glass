# Native Service Worker waiting-worker arbitration (351)

```yaml
id: native-engine-browser-351
scope: native-engine/service-worker-arbitration
status: done
depends-on: [native-engine-browser-350]
```

## Objective

Close the next Service Worker lifecycle boundary in the native owner:

- make `skipWaiting()` emit a validated worker-owned host command;
- retain an existing active worker while an update that does not request
  `skipWaiting()` reaches the installed/waiting state;
- expose `registration.waiting`, preserve `registration.active`, and publish
  the bounded installing/installed transition for a waiting update;
- activate a matching waiting worker before the next native navigation,
  settle its activate `waitUntil()` work, and then replace the active worker;
- keep the old active worker and its routes intact when installation fails or
  an update remains waiting; and
- preserve the existing immediate activation behavior for workers that call
  `skipWaiting()`.

The page-facing update Promise continues to resolve `undefined`; lifecycle
events are delivered by the 350 state/event contract. This slice covers one
content-process owner and one active navigation client. Multi-client
controller ownership, durable waiting-worker persistence, exact task-source
arbitration, and complete `clients.claim()` behavior remain separate gates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-342.md`
- `docs/plan/tasks/native-engine-browser-350.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-351.md`

## Contract and tradeoffs

An installed update is kept in an in-memory waiting-worker map keyed by its
canonical registration scope. The active worker continues to serve matching
Fetch and navigation requests until a later matching navigation promotes the
waiting worker. A successful `skipWaiting()` request bypasses that wait and
promotes the candidate after install work settles. Waiting workers are not
profile-persisted in this slice; restart restoration continues to restore the
last durable active registration only.

The native owner still serializes lifecycle work and does not claim full
browser task-source or multi-client controller parity. `clients.claim()` keeps
its existing bounded one-client behavior; exact claim arbitration remains an
explicit follow-up rather than being implied by waiting-worker state.

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
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — 7 passed
- `python3 scripts/check-documentation-coverage.py` — passed
- `python3 scripts/check-documentation-depth.py` — passed
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed
- `python3 scripts/check-tui-shortcuts.py` — passed
- `git diff --check`
