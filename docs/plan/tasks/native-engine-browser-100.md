---
id: native-engine-browser-100
scope: glass-browser/native-engine/blob-file-binary-construction
status: done
depends-on: [native-engine-browser-099]
---

# BE-21: bounded binary Blob/File construction

## Objective

Allow native `Blob` and `File` values to preserve exact bytes when constructed
from bounded `ArrayBuffer` or typed-array parts, so those values can feed the
binary Fetch/XHR request-body bridge.

## Contract

- `Blob` and `File` accept bounded `ArrayBuffer` and `ArrayBuffer.isView`
  parts, including typed arrays and `DataView`, alongside the existing string
  and native Blob parts.
- A value containing a binary part retains a bounded raw byte snapshot and
  reports byte length; `arrayBuffer()`, `bytes()`, `slice()`, `text()`, and the
  099 request-body path consume that snapshot without lossy conversion.
- Text-only parts retain their established bounded text-backed behavior,
  including existing MIME normalization and File metadata.
- Oversized or unsupported parts fail closed at construction. Multipart
  `FormData` remains text-backed in this slice; binary FormData, streams,
  upload progress, and complete Blob/File Web IDL parity remain open.

## Ownership and sequence

```text
ArrayBuffer/view parts -> bounded Blob/File byte snapshot -> reads or upload
```

The page realm owns construction and a private copy. The existing Fetch/XHR
bridge owns transfer encoding and the content process remains the byte-size
validation boundary before network transmission.

## Deliberate boundary and tradeoffs

The implementation recognizes only bounded buffer/view parts and copies them,
which prevents aliasing and detached-buffer identity claims but costs a bounded
allocation per value. Text-only construction remains on its existing path so
this slice does not silently redefine unrelated Blob/Web IDL behavior. Binary
multipart serialization and stream APIs require separate ownership and
backpressure work.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, constructed Blob/File byte
behavior, full native-engine target, strict-Clippy baseline, documentation,
and release-truth gates complete. The checkout is local-only: no push, remote
CI, release, tag, or registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline; normalized diagnostics match the 094 baseline exactly (32 pre-existing diagnostics), with no new diagnostic
- `python3 scripts/check-documentation-coverage.py` — 750 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  750 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=923; current-claim failures=0
