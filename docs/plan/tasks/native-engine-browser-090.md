---
id: native-engine-browser-090
scope: glass-browser/native-engine/fetch-blob-body
status: done
depends-on: [native-engine-browser-089]
---

# BE-11: bounded fetch Blob/File request bodies

## Objective

Allow ordinary `fetch()` callers to send the native engine's bounded
text-backed `Blob` and `File` values as request bodies. The request must carry
the payload rather than the JavaScript object's default string representation,
and should inherit the value's MIME type when the caller did not supply one.

## Contract

- A text-backed native `Blob` or `File` passed as `fetch()` `body` contributes
  its bounded text payload to the request body.
- When no `Content-Type` header is supplied and the Blob/File has a non-empty
  normalized `type`, that type is transferred as the request content type.
- An explicit supported `Content-Type` header remains authoritative; the
  existing header policy and GET/POST method limits remain in force.
- The behavior works through the shared local/content-worker fetch bridge and
  retains the existing request-size, response-size, origin, CORS, credentials,
  and redirect policies.
- FormData and URLSearchParams serialization remain unchanged. XHR Blob-body
  coercion, binary Blob constructor parts, streams, upload progress, and full
  Fetch/Blob Web IDL parity remain open.

## Ownership and sequence

```text
Blob/File -> bounded text payload + MIME -> fetch command -> HTTP request
```

The JavaScript bootstrap identifies the native Blob/File and emits its
text-backed payload. Rust and the content worker retain ownership of URL,
header, origin, CORS, credentials, redirect, and network policy.

## Deliberate boundary and tradeoffs

This reuses the existing text-backed representation, so it fixes the common
request-body coercion failure without introducing arbitrary binary transport.
The body is therefore text payload data, not a byte-preserving file stream;
binary Blob parts, streaming/backpressure, progress, abort, and XHR parity
remain separate contracts.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, worker HTTP behavior, full
native-engine target, documentation, and release-truth gates complete. The
checkout is local-only: no push, remote CI, release, tag, or registry-
publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_uploads_bounded_file_blob_form_data -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 740 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 740 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=913; current-claim failures=0
