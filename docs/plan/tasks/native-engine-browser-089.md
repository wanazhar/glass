---
id: native-engine-browser-089
scope: glass-browser/native-engine/blob-file-binary-read
status: done
depends-on: [native-engine-browser-088]
---

# BE-10: bounded Blob/File binary reads

## Objective

Make the existing bounded text-backed `Blob` and `File` objects consumable by
ordinary web code that requests bytes. `arrayBuffer()` and the runtime's
`bytes()` convenience method must return fresh, deterministic UTF-8 byte
copies without changing the existing text/FormData contract.

## Contract

- `Blob.prototype.arrayBuffer()` resolves to a fresh `ArrayBuffer` containing
  the UTF-8 encoding of the bounded text payload.
- `Blob.prototype.bytes()` resolves to a fresh `Uint8Array` over the same
  UTF-8 bytes; `File` inherits both methods.
- Unicode scalar values, surrogate pairs, and unpaired UTF-16 surrogates are
  encoded deterministically; unpaired surrogates become U+FFFD.
- Binary reads retain the existing `storageValueLimit` bound and fail with a
  `RangeError` if the encoded byte vector is outside that bound.
- Existing text-backed `size`, `type`, `name`, `lastModified`, `text()`,
  `slice()`, FormData serialization, and IndexedDB tags remain unchanged.
- The shared bootstrap is exercised in both a local realm and the existing
  content-worker IndexedDB path. Streams, binary Blob/File construction,
  upload progress, and complete Blob/File Web IDL parity remain open.

## Ownership and sequence

```text
text-backed Blob/File -> bounded UTF-8 encoder -> fresh ArrayBuffer/Uint8Array
```

The JavaScript realm owns conversion and result allocation. Rust remains the
network, profile, and IPC owner; no binary file is written to disk and no new
transport format is introduced.

## Deliberate boundary and tradeoffs

Keeping `_text` as the source of truth avoids a second storage representation
and keeps existing multipart behavior stable. It means this slice is a
text-to-bytes bridge, not arbitrary binary Blob support: `size` continues to
report the established text-backed length, binary constructor parts remain
unsupported, and byte-exact file persistence, streams, transfer semantics,
and full Blob/File identity are future work.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, local and worker behavior,
full native-engine target, documentation, and release-truth gates complete.
The checkout is local-only: no push, remote CI, release, tag, or
registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_file_and_blob_form_data_are_bounded_and_text_backed -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_indexed_db_round_trips_through_worker_ipc -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 739 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 739 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=912; current-claim failures=0
