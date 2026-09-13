# Glass native engine browser slice 287: worker Fetch bytes

Status: completed locally.

## Objective

Preserve binary identity across the classic dedicated-worker Fetch boundary.
Worker request bodies must reach the shared native loader as the bytes the
worker supplied, and response body consumers must not replace non-UTF-8 bytes
with replacement characters before worker code reads them.

## Contract

- Worker `fetch()` accepts bounded string, `ArrayBuffer`, typed-array,
  `Blob`, and `File` bodies. Blob/File MIME type is used when no explicit
  content type was supplied.
- Worker response payloads carry bounded base64-encoded raw bytes in addition
  to the existing diagnostic text view.
- Worker responses expose `bytes()`, `arrayBuffer()`, and `blob()` alongside
  `text()` and `json()`. Each response body is one-shot; `clone()` creates an
  independent bounded body owner before consumption.
- Binary transfer remains worker-owner-tagged and uses the existing script,
  body, response, IPC, and worker-message limits. The shared Rust loader stays
  the single URL, cookie, CORS, redirect, and transport-policy owner.
- Unsupported or oversized values fail closed. No CDP fallback or page-realm
  resolver is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker response serializer now carries base64-preserved bytes. The worker
bootstrap decodes those bytes under the existing bound, provides deterministic
UTF-8 conversion for text/JSON, and installs bounded Blob/File byte helpers.
Fetch recognizes raw buffer views and Blob-like values, encodes their bytes for
the typed command, and retains explicit MIME metadata. Response body
consumers copy their bounded snapshot, mark the response disturbed, and
reject later reads; clones receive independent snapshots.

The host loader and page Fetch path are unchanged. This keeps URL resolution,
request policy, response limits, and transport behavior centralized while
extending only the worker bridge fidelity.

## Tradeoffs and follow-up

This slice uses bounded base64 over the existing serialized worker boundary, so
it adds allocation and transfer overhead but avoids byte corruption and keeps
the current two-crate architecture. Worker `ReadableStream` bodies, full
`Request`/`Response` Web IDL identity, transfer lists, XHR, module/shared/
service workers, and complete native/CDP parity remain Issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine native_content_process_worker_fetch_preserves_binary_request_and_response_bodies -- --nocapture` (1 passed)
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine worker -- --nocapture` (13 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
