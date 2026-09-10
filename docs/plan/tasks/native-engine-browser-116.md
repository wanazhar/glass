---
id: native-engine-browser-116
scope: glass-browser/native-engine/response-readable-stream
status: done
depends-on: [native-engine-browser-115]
---

# BE-30: bounded Fetch Response ReadableStream

## Objective

Expose a bounded native `ReadableStream` body for ordinary Fetch responses
without changing the existing independent text, JSON, Blob, ArrayBuffer, or
bytes readers.

## Contract

- A non-filtered Fetch response exposes `body` as a native
  `ReadableStream`, and `body instanceof ReadableStream` is true.
- The stream owns one bounded raw-byte chunk. Its default reader returns that
  chunk once and then `{ done: true }`; empty bodies complete immediately.
- `locked`, `getReader()`, reader `read()`, `releaseLock()`, `cancel()`,
  `closed`, and the stream/reader async-iterator hooks retain the bounded
  owner state. A released reader rejects later reads, and cancellation makes
  later readers complete without a chunk.
- Opaque and `opaqueredirect` responses expose `body === null`; their existing
  filtered metadata and rejected body-method behavior remain unchanged.
- Full transport streaming, backpressure, incremental chunk delivery, body
  disturbance/`bodyUsed`, BYOB readers, transport-level cancellation,
  trailers, and complete ReadableStream/Response Web IDL parity remain open.

## Ownership and sequence

```text
Fetch payload bytes -> bounded ReadableStream owner -> one reader chunk
                                  |-> lock/release/cancel state
```

The response keeps its independent body-method projection. The stream owns a
separate bounded byte copy so reading or cancelling it does not invalidate the
existing response readers or Blob/XHR consumers.

## Deliberate boundary and tradeoffs

This makes the common response-body stream shape observable to page scripts
and preserves binary bytes through the reader, while avoiding a transport or
task-source redesign. The one-chunk owner is deliberately finite: it proves
stream identity and basic reader lifecycle without claiming progressive
network delivery or browser-grade backpressure.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_opaque_no_cors_response -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` — passed after the final assertion formatting patch
- `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 766 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  766 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=940; current-claim failures=0
