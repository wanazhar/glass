---
id: native-engine-browser-087
scope: glass-browser/native-engine/indexeddb-blob-file-clone
status: done
depends-on: [native-engine-browser-086]
---

# BE-08: bounded IndexedDB Blob/File cloning

## Objective

Carry the native engine’s existing bounded text-backed `Blob` and `File`
objects through IndexedDB reads, profile persistence, worker transfer, and
restart. A page should not lose the object identity or metadata that the
existing FormData surface already supports.

## Contract

- Text-backed native `Blob` values persist their bounded text payload and MIME
  type; reads reconstruct fresh `Blob` instances with working `size`, `type`,
  `text()`, and `slice()` behavior.
- Text-backed native `File` values persist their payload, MIME type, name, and
  non-negative `lastModified`; reads reconstruct fresh `File` instances.
- Blob/File parts remain subject to the existing bounded text/value limits and
  are encoded as validated tagged JSON at the Rust profile and IPC boundary.
- Existing JSON, selected structured-clone extensions, indexes, transaction
  queue, rollback, and StorageManager behavior remain unchanged. Indexes do
  not treat Blob/File objects as valid primitive keys.
- Unsupported binary buffers, typed arrays, SharedArrayBuffer, transfer lists,
  and arbitrary host objects continue to fail closed or remain outside the
  declared profile.

## Ownership and sequence

```text
Blob/File -> tagged text payload -> JSON profile/IPC -> new Blob/File instance
```

The JavaScript realm owns object reconstruction and methods. Rust validates
the bounded tagged JSON representation and remains the persistence authority;
content workers continue to receive values only through existing typed IPC.

## Deliberate boundary and tradeoffs

Reusing the existing text-backed Blob/File implementation avoids a second
binary storage format and keeps memory/disk use predictable. It does not claim
byte-exact binary Blob/File semantics, stream/arrayBuffer support, typed-array
interoperability, transfer semantics, or complete prototype/descriptor parity.
Those require a separately bounded binary representation and remain future
work, along with full IndexedDB/Web IDL parity and the remaining browser gates.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_round_trips_bounded_structured_clone_extensions -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_ -- --nocapture` — 9 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 737 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 737 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=910; current-claim failures=0
