---
id: native-engine-browser-088
scope: glass-browser/native-engine/indexeddb-binary-clone
status: done
depends-on: [native-engine-browser-087]
---

# BE-09: bounded IndexedDB binary structured cloning

## Objective

Extend the native engine's bounded IndexedDB structured-clone format from
text-oriented objects to byte-oriented `ArrayBuffer`, common typed-array, and
`DataView` values. Values must survive an API read and profile restart without
being silently reduced to empty ordinary objects.

## Contract

- `ArrayBuffer` values are encoded as bounded byte vectors and reconstructed as
  fresh `ArrayBuffer` instances.
- The shared numeric typed-array constructors (`Int8Array`, `Uint8Array`,
  `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`,
  `Float32Array`, and `Float64Array`) are encoded from their visible byte range
  and reconstructed with the same constructor. `BigInt64Array` and
  `BigUint64Array` are supported when the runtime exposes them.
- `DataView` values are encoded from their visible byte range and reconstructed
  as fresh `DataView` instances.
- Binary values use the existing recursive depth, value-size, profile, and IPC
  limits. The byte representation is validated through the existing tagged
  JSON path before it is persisted or transferred.
- `SharedArrayBuffer` is rejected with `DataCloneError`; transfer-list and
  detached-buffer semantics, arbitrary host objects, binary Blob/File methods,
  streams, and complete prototype/descriptor parity remain outside this
  slice.
- Existing ordinary JSON, selected structured-clone extensions, Blob/File
  handling, indexes, transactions, journal merge, quota, and restart behavior
  remain unchanged.

## Ownership and sequence

```text
ArrayBuffer/view -> byte-vector tag -> JSON profile/IPC -> fresh buffer/view
```

The JavaScript realm owns byte extraction and reconstruction. Rust remains the
profile and IPC boundary owner; the content worker continues to use the shared
bootstrap and existing typed state-transfer protocol rather than opening
profile files.

## Deliberate boundary and tradeoffs

Persisting visible bytes makes binary values deterministic and bounded, but it
does not preserve backing-buffer aliasing, byte offsets, view identity, or
transfer/detachment behavior. The implementation therefore favors safe
restart and cross-realm transport over full browser clone parity. It also does
not turn the existing text-backed Blob/File implementation into a binary file
store; those methods and stream semantics need a separately bounded contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, focused behavior, native
IndexedDB suite, full native-engine target, documentation, and release-truth
gates complete. The checkout is local-only: no push, remote CI, release, tag,
or registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_round_trips_bounded_structured_clone_extensions -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_ -- --nocapture` — 9 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 738 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 738 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=911; current-claim failures=0
