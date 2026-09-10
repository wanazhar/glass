---
id: native-engine-browser-098
scope: glass-browser/native-engine/fetch-binary-responses
status: done
depends-on: [native-engine-browser-097]
---

# BE-19: bounded byte-preserving Fetch responses

## Objective

Preserve the raw bounded HTTP response bytes across the native
content-process-to-JavaScript boundary so binary response consumers do not
inherit the existing lossy text conversion.

## Contract

- The content process transfers a bounded base64 byte payload alongside the
  existing replacement-decoded text view.
- `Response.arrayBuffer()`, `Response.bytes()`, and `Response.blob()` return
  fresh byte-preserving copies for arbitrary bytes within the existing native
  response transfer limit.
- Binary response `Blob.slice()` uses byte offsets and retains raw bytes;
  `Blob.text()`, `Response.text()`, and `Response.json()` remain UTF-8
  replacement-decoded views.
- Existing response status, URL, headers, CORS/origin, credentials, redirect,
  size, and request-header policies remain authoritative.
- Streaming bodies, BYOB readers, progress/backpressure, transfer/detach
  identity, binary `Blob` constructor/FormData/request-body parity, and full
  Fetch/Response/Blob Web IDL parity remain open.

## Ownership and sequence

```text
HTTP Vec<u8> -> bounded base64 IPC payload -> page byte snapshot -> response body reads
```

The Rust/content-worker network owner retains the raw bytes and enforces the
existing response bound. JavaScript decodes a fresh private byte snapshot for
each response body consumer; the public text APIs keep the established lossy
UTF-8 contract.

## Deliberate boundary and tradeoffs

Base64 keeps the existing JSON-framed IPC protocol and avoids a new binary
transport, at the cost of roughly 4/3 wire expansion and a smaller practical
payload ceiling under the existing script-transfer limit. Fresh snapshots
prevent aliasing but do not provide streaming or transferable-buffer identity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, binary response behavior, full
native-engine target, documentation, and release-truth gates complete. The
checkout is local-only: no push, remote CI, release, tag, or registry-
publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; normalized diagnostics match the 094 baseline and no new diagnostic was introduced
- `python3 scripts/check-documentation-coverage.py` — 748 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  748 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=921; current-claim failures=0
