---
id: native-engine-browser-081
scope: glass-browser/native-engine/indexeddb-versionchange
status: done
depends-on: [native-engine-browser-080]
---

# BE-08: bounded IndexedDB version-change coordination

## Objective

Give the bounded IndexedDB realm the connection lifecycle needed for normal
database upgrades. Existing same-realm connections must receive a
`versionchange` notification; a higher-version open must report `blocked`
while those connections remain open; and an explicit `close()` must unblock
the pending upgrade. The implementation stays inside the existing JavaScript
realm and two-crate architecture.

## Contract

- A persistent JavaScript realm keeps a bounded connection registry and
  pending-upgrade registry in the global native IndexedDB slots. Each
  successful `indexedDB.open()` returns a connection with `version`, sorted
  `objectStoreNames`, `onversionchange`, `onclose`, and idempotent `close()`.
- Opening an existing database at a higher version dispatches a bounded
  `versionchange` event to each live connection. If any connection remains
  open, the request emits one `blocked` event and remains pending; closing the
  final connection schedules the upgrade again.
- The resumed request runs its version-change transaction and
  `onupgradeneeded` callback, then publishes the normal success result. The
  transaction exposes its database, mode, error, sorted object-store names,
  completion/error/abort hooks, and the current upgrade store list.
- Upgrade-time object-store creation/deletion updates the version-change
  transaction’s store list. Index creation/deletion remains restricted to
  that transaction, and the earlier 080 index/query/cursor limits remain in
  force.
- Pending upgrades are bounded by the existing database/name limits and do
  not retain profile file handles. Cross-process connections cannot be
  notified through the profile journal; cross-process version-change blocking
  and live connection identity remain an explicit future boundary.

## Ownership and sequence

```text
open(v1) -> register connection A
                |
open(v2) -> versionchange(A) -> A.close() -> unblock pending open
                |                              |
                +-- still open -> blocked -----+-> upgrade transaction
```

The realm owns connection identity and upgrade scheduling. The parent still
owns durable profile persistence, and the existing journal/delta path remains
the authority for state convergence between independent engines.

## Deliberate boundary and tradeoffs

Blocking an upgrade until a live connection closes matches the important
application-visible part of IndexedDB’s version-change lifecycle and avoids
silently mutating an open connection’s schema. The bounded implementation
does not yet coordinate connection identity across Glass processes or
implement complete `deleteDatabase` blocking, transaction rollback, or every
version-change event detail. A future cross-process connection protocol would
need a separate lease/heartbeat contract; encoding live JS object identity in
the existing durable journal would be incorrect.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, focused behavior, full
native-engine, documentation, and release-truth gates complete. The checkout
is local-only: no push, remote CI, release, tag, or registry-publication claim
is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine indexed_db_ -- --nocapture` — 6 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 375 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 731 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 731 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=904; current-claim failures=0
