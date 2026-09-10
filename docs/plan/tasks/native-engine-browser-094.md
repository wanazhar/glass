---
id: native-engine-browser-094
scope: glass-browser/native-engine/fetch-response-bodies
status: done
depends-on: [native-engine-browser-093]
---

# BE-15: bounded fetch response body variants

## Objective

Expose the common response-consumption methods used after native `fetch()`:
`blob()`, `arrayBuffer()`, and `bytes()`, alongside the existing `text()` and
`json()` methods.

## Contract

- `Response.blob()` resolves to a fresh bounded text-backed `Blob` carrying
  the response body and response content type.
- `Response.arrayBuffer()` and `Response.bytes()` resolve to fresh UTF-8
  copies over the bounded response body.
- Existing `ok`, `status`, `url`, content-type lookup, `text()`, and `json()`
  behavior remain unchanged; each body method can be called independently
  because the current response is an immutable bounded payload snapshot.
- The local/content-worker fetch bridge is covered by one response-consumer
  test and keeps the existing response-size, UTF-8-lossy host decoding, CORS,
  origin, credentials, redirect, and error policies.
- Streaming `Response.body`, byte-preserving non-UTF-8 responses, BYOB
  readers, trailers, and complete Fetch/Response Web IDL parity remain open.

## Ownership and sequence

```text
bounded host response -> immutable page payload -> fresh Blob/ArrayBuffer/bytes copy
```

The JavaScript realm owns response method results and fresh-copy allocation.
Rust/content-worker owners continue to validate and bound the network response;
the current payload transfer remains UTF-8-lossy text.

## Deliberate boundary and tradeoffs

Reusing the bounded text-backed Blob path keeps all response variants aligned
with existing limits and avoids a new binary IPC format. The tradeoff is that
arbitrary non-UTF-8 response bytes are not recoverable at this boundary, and
streaming/backpressure is not represented. A byte-preserving network payload
contract is required before claiming general binary response parity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, worker response-consumption
behavior, full native-engine target, documentation, and release-truth gates
complete. The checkout is local-only: no push, remote CI, release, tag, or
registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 744 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 744 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=917; current-claim failures=0
