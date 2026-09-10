---
id: native-engine-browser-102
scope: glass-browser/native-engine/formdata-binary-parts
status: done
depends-on: [native-engine-browser-101]
---

# BE-23: bounded binary FormData parts

## Objective

Preserve raw-byte-backed Blob/File values when they are appended to bounded
multipart `FormData` submitted through Fetch or asynchronous XHR.

## Contract

- Text-only FormData keeps the established string serializer and boundary,
  filename, MIME, and text-field behavior.
- If a FormData file part already carries a bounded byte snapshot, the
  serializer emits UTF-8 bytes for multipart framing/text fields and the raw
  bytes for the file part, then transfers the bounded multipart body through
  the existing request `body_base64` bridge.
- Oversized aggregate multipart bodies fail closed before command delivery;
  content type remains the generated multipart boundary and caller-supplied
  Content-Type remains rejected.
- Fetch and asynchronous XHR share the serializer and transport body path.
  Streaming FormData, upload progress, iterator identity, and complete
  FormData/Blob Web IDL parity remain open.

## Ownership and sequence

```text
FormData entries -> bounded multipart bytes -> base64 command -> HTTP request
```

JavaScript owns multipart formatting and the private byte assembly. The
content process decodes and size-checks the command, while the resource loader
owns the final request method, CORS, cookie, redirect, and policy behavior.

## Deliberate boundary and tradeoffs

The existing text-only path is retained to avoid changing established body
serialization for non-binary forms. Binary multipart bodies are copied into a
bounded aggregate snapshot and base64-encoded, which costs memory and wire
space but keeps one JSON IPC protocol and one network-policy path. Streaming,
backpressure, and upload progress require a separate transport contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, exact binary multipart
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
- `python3 scripts/check-documentation-coverage.py` — 752 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  752 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=925; current-claim failures=0
