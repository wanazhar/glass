---
id: native-engine-browser-104
scope: glass-browser/native-engine/xhr-binary-responses
status: done
depends-on: [native-engine-browser-103]
---

# BE-25: bounded XHR binary response types

## Objective

Expose the native Fetch bridge's byte-preserving response body through bounded
asynchronous XHR `arraybuffer` and `blob` response types.

## Contract

- XHR accepts `responseType` values `""`, `"text"`, `"arraybuffer"`, and
  `"blob"`; unsupported values fail closed at `send()`.
- Text mode retains the existing `responseText` and `response` behavior.
  `arraybuffer` returns a fresh byte-preserving `ArrayBuffer`; `blob` returns a
  fresh native Blob with the response MIME type and raw bytes.
- Binary modes clear `responseText` and preserve status, URL, response headers,
  abort/stale-continuation behavior, and the existing bounded response limit.
- Response bytes are not converted through replacement-decoded text. Timeout,
  streaming, upload progress, synchronous XHR, and full XHR/Blob Web IDL parity
  remain open.

## Ownership and sequence

```text
HTTP Vec<u8> -> Fetch response payload -> XHR responseType -> ArrayBuffer/Blob
```

The resource loader and Fetch response bridge retain byte ownership and bounds.
The JavaScript XHR wrapper selects the public response variant and keeps fresh
value semantics; no second transport is introduced.

## Deliberate boundary and tradeoffs

The slice supports only asynchronous binary response modes that can be backed
by the existing bounded response snapshot. It avoids streams, progress, and
detached/transfer identity, which keeps the IPC contract stable but does not
claim full XHR response Web IDL parity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, text and binary XHR response
behavior, full native-engine target, strict-Clippy baseline, documentation,
and release-truth gates complete. The checkout is local-only: no push, remote
CI, release, tag, or registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline; normalized diagnostics match the 094 baseline exactly (32 pre-existing diagnostics), with no new diagnostic
- `python3 scripts/check-documentation-coverage.py` — 754 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  754 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=927; current-claim failures=0
