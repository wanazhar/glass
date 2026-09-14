# Native Service Worker waiting-worker restoration (352)

```yaml
id: native-engine-browser-352
scope: native-engine/service-worker-waiting-restoration
status: done
depends-on: [native-engine-browser-351]
```

## Objective

Carry the Service Worker update contract across a native content-process
restart:

- persist bounded metadata for an installed/waiting worker alongside the
  activated registration;
- restore the active and waiting worker realms without replaying installation
  or activation side effects;
- keep the restored waiting worker visible as `registration.waiting` while the
  current client is outside its scope; and
- preserve the 351 navigation promotion and failure behavior after
  restoration.

The profile remains owned by Rust and stores script URLs and worker types, not
worker runtime state or page data. Waiting-worker persistence is limited to one
waiting candidate per registration scope and remains subject to the existing
profile, script, and cache quotas. Multi-client controller ownership, exact
task-source scheduling, and full Service Worker lifecycle conformance remain
separate issue #40 gates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-351.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-352.md`

## Contract and tradeoffs

The persisted registration record continues to describe the last activated
worker and gains an optional waiting-worker descriptor. Restoration loads each
script through the existing native resource policy and recreates the isolated
QuickJS realm, but does not rerun install or activate events. A failed waiting
script restoration leaves its descriptor available for a later retry while
the active worker remains usable. A later matching navigation still settles
activate `waitUntil()` before replacing the active worker.

This is durable registration arbitration, not multi-client ownership: the
current content process still has one top-level navigation client. The profile
does not serialize pending event promises, MessagePort queues, or arbitrary
worker globals; those remain bounded live-process state.

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
