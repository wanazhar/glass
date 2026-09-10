---
id: native-engine-browser-091
scope: glass-browser/native-engine/xhr-blob-body
status: done
depends-on: [native-engine-browser-090]
---

# BE-12: bounded XHR Blob/File request bodies

## Objective

Let asynchronous native `XMLHttpRequest.send()` preserve a text-backed
`Blob` or `File` body when it delegates to the existing bounded fetch bridge.
This closes the immediate object-to-string coercion gap for upload code while
keeping the XHR transport intentionally bounded.

## Contract

- `XMLHttpRequest.send()` recognizes native Blob/File values and forwards the
  object to the existing fetch bridge instead of coercing it to a string.
- The Blob/File payload and normalized MIME type follow the same bounded rules
  as direct `fetch()` bodies; an explicit supported `Content-Type` header
  remains authoritative.
- The content-worker path is covered with an HTTP assertion for exact method,
  payload, and content type, and existing response callback behavior remains
  unchanged.
- Only asynchronous GET/POST XHR remains supported. Binary response types,
  upload progress, timeout/abort, streaming/backpressure, synchronous XHR,
  binary Blob construction, and full XHR/Blob Web IDL parity remain open.

## Ownership and sequence

```text
XHR.send(Blob/File) -> fetch bridge -> bounded request command -> HTTP owner
```

The JavaScript XHR wrapper preserves the Blob/File marker. The shared fetch
bridge and Rust/content-worker owners continue to enforce request method,
header, URL, origin, CORS, credentials, redirect, and size policy.

## Deliberate boundary and tradeoffs

Delegating to the existing fetch bridge avoids duplicate request serialization
and keeps direct fetch and XHR payload behavior aligned. It still sends the
text-backed payload as one bounded request body; it does not create a binary
stream, expose upload lifecycle events, or implement cancellation once the
network owner has accepted the request.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 741 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 741 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=914; current-claim failures=0
