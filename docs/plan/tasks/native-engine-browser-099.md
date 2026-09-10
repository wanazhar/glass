---
id: native-engine-browser-099
scope: glass-browser/native-engine/fetch-binary-request-bodies
status: done
depends-on: [native-engine-browser-098]
---

# BE-20: bounded binary Blob/File request bodies

## Objective

Preserve raw bytes when a Blob/File value that already carries a bounded byte
snapshot is used as a Fetch or asynchronous XHR request body.

## Contract

- The page bridge emits an optional bounded base64 request-body field only for
  raw-byte-backed Blob/File values; text-backed bodies retain the existing text
  field and behavior.
- The content process strictly decodes and size-checks the binary field before
  creating a Rust-owned `NativeFetchBody::Bytes` value. Invalid or oversized
  transfers fail closed.
- The resource loader sends text and byte bodies through the same method,
  content-type, CORS/preflight, cookie, redirect, and policy paths; 301/302/303
  method-reset behavior still removes the body.
- Asynchronous XHR inherits the binary body behavior through its existing
  Fetch bridge. The worker test verifies exact non-UTF-8 bytes for both Fetch
  and XHR uploads.
- Multipart `FormData` remains text-backed in this slice. Binary Blob/File
  construction, binary/stream FormData, upload progress, streaming, and full
  Fetch/XHR/Blob Web IDL parity remain open.

## Ownership and sequence

```text
raw Blob/File bytes -> bounded base64 command -> Rust byte body -> HTTP request
```

JavaScript owns the bounded snapshot and wire encoding. The content process is
the trust boundary that decodes and revalidates the size. The resource loader
owns the final HTTP request body and preserves the existing network policy.

## Deliberate boundary and tradeoffs

Reusing the JSON-framed command protocol avoids a second binary IPC channel and
keeps Fetch and XHR on one policy path, at the cost of base64 expansion and a
smaller practical payload ceiling. This slice deliberately does not broaden
Blob construction or multipart serialization, so it closes byte preservation
for already-backed values without implying general binary Web API parity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, Fetch/XHR raw-byte behavior,
full native-engine target, strict-Clippy baseline, documentation, and
release-truth gates complete. The checkout is local-only: no push, remote CI,
release, tag, or registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline; normalized diagnostics match the 094 baseline exactly (32 pre-existing diagnostics), with no new diagnostic
- `python3 scripts/check-documentation-coverage.py` — 749 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  749 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=922; current-claim failures=0
