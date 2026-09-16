# Native compact binary form-body transport (417)

status: complete
scope: native-engine/compact-binary-form-body
issue: 40
depends-on: [native-engine-browser-416]

## Objective

Keep bounded multipart and file-backed form POST targets usable across the
content-process boundary. The typed `Bytes` request body must use a compact
wire representation rather than a JSON number array, so serialization overhead
does not consume the IPC frame budget before the existing form-body limit is
reached.

## Contract

- `NativeRequestBody::Bytes` crosses the content-process navigation envelope as
  a standard base64 string and round-trips byte-for-byte.
- Text form bodies retain their existing string representation and behavior.
- The decoded byte body remains bounded by `MAX_NATIVE_FORM_BODY_BYTES`; a
  malformed, oversized, or wrong-kind body fails closed before navigation.
- Multipart form target navigation preserves its method, content type,
  boundary, and bytes through `_blank` popup creation without a duplicate GET.
- No unbounded buffering, streaming claim, fallback backend, or change to
  form encoding is introduced.

## Implementation path

- Add a serde adapter for the binary request-body variant.
- Keep payload validation at content-process decode, engine queue admission,
  and browser-effect reconstruction boundaries.
- Add a resource-loader round-trip unit witness and an HTTP multipart target
  witness that inspects request bytes and request count.
- Synchronize the architecture, plan, analysis, task record, and issue #40
  checkpoint.

## Tradeoffs

- Base64 adds about one third to raw bytes, but is substantially smaller than
  JSON's per-byte number-array overhead and remains portable across the typed
  process protocol.
- The body is still materialized once per bounded owner; demand-driven upload
  streaming and progress events remain separate promotion work.
- Standard base64 is intentionally boring and interoperable; the protocol does
  not expose a custom binary framing dependency.

## Evidence

- `cargo fmt --all` — passed.
- `cargo check --quiet -p glass-browser --tests --locked` — passed.
- `cargo test --quiet -p glass-browser --lib binary_navigation_body_uses_compact_base64_wire_encoding --locked -- --nocapture` — passed.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_multipart_form_post_target_preserves_binary_body -- --nocapture` — passed; the server observed one initial GET followed by one multipart POST and inspected the boundary, field name, and value bytes.
- `git diff --check` — passed.

Documentation coverage, depth, and release-truth checks remain the final
checkpoint after this task is staged. Remote CI, push, release, tag, and
registry publication are outside this local checkpoint.
