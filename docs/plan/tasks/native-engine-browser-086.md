---
id: native-engine-browser-086
scope: glass-browser/native-engine/indexeddb-structured-clone
status: done
depends-on: [native-engine-browser-085]
---

# BE-08: bounded IndexedDB structured-clone extensions

## Objective

Extend the JSON-backed IndexedDB model with useful non-JSON values while
keeping profile persistence, worker transfer, and resource bounds explicit.
Values written by a page should retain selected structured-clone identities
after reads and profile restart rather than being silently stringified or
dropped.

## Contract

- The native realm persists tagged JSON representations for `undefined`,
  `NaN`, positive/negative infinity, negative zero, `Date`, `RegExp`, `Map`,
  and `Set`; reads and cursors decode those tags into fresh JavaScript values.
- Plain objects and arrays recursively preserve `undefined` members and nested
  supported values. Cyclic values, functions, symbols, BigInts, invalid dates,
  and user objects containing the reserved native tag are rejected with
  `DataCloneError`.
- Encoding is bounded by a nesting depth of 64 and the existing 8 KiB value
  limit. Tagged values remain ordinary validated JSON at the Rust profile and
  IPC boundaries, so existing persistence, merge, journal, and recovery paths
  remain unchanged.
- Index key-path extraction decodes stored values before reading fields;
  IndexedDB keys remain the existing finite-number/string subset. RegExp
  `lastIndex` is reset on clone, matching the selected bounded contract.
- Stored values are decoded only at API return/key-path boundaries; internal
  state remains serializable and content workers continue to access it only
  through typed IPC.

## Ownership and sequence

```text
page value -> bounded tag encoder -> JSON profile/IPC -> tag decoder -> page value
                     |-> reject cycles, unsupported types, oversize values
```

The JavaScript realm owns identity reconstruction. Rust continues to validate
the tagged JSON shape and aggregate size; it does not execute or interpret
JavaScript value identities.

## Deliberate boundary and tradeoffs

Tagged JSON keeps the two-crate architecture and existing profile protocol
stable, but it is not full HTML structured-clone parity. ArrayBuffer,
SharedArrayBuffer, typed arrays, Blob/File binary payloads, BigInt, transfer
lists, host objects, and complete clone/prototype semantics remain open.
Reserved-tag collisions are rejected to avoid ambiguous persisted values, and
recursive encoding adds bounded CPU work on writes. Full IndexedDB/Web IDL
parity, cross-process connection identity, quota permission policy, and the
remaining browser-complete gates remain future work.

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
- `python3 scripts/check-documentation-coverage.py` — 736 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 736 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=909; current-claim failures=0
