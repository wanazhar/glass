# Native engine browser slice 248: binary upload bodies

Status: completed locally.

## Objective

Deliver selected native files through the existing HTTP(S) request owners, so
file inputs are useful for real browser workflows rather than only observable
inside page JavaScript. `fetch(FormData)` and native multipart form navigation
must preserve file names, content types, boundaries, and raw bytes across the
content-process bridge.

## Contract

- `NativeRequestBody` carries either bounded UTF-8 text or bounded raw bytes.
- The shared navigation loader sends both request-body variants through POST,
  including redirect handling and the existing content-type policy.
- The content-process load and fetch wires use either `body` or `body_base64`,
  reject ambiguous dual representations, reject binary GET bodies, and apply
  the same bounded body limit before decoding or dispatch.
- A native multipart form containing selected files emits raw multipart bytes;
  each file part carries its field name, leaf filename, media type, and exact
  in-memory bytes. Text controls retain their prior URL-encoded and text/plain
  behavior.
- JavaScript `fetch` accepts `ArrayBuffer` and `ArrayBufferView` request bodies
  in addition to the existing string, Blob, URLSearchParams, and FormData
  surfaces. FormData continues to choose its own multipart boundary.
- The body limit is derived from the native upload aggregate limit plus bounded
  framing overhead. File paths never cross into the page realm or IPC payload.

## Implementation

`NativeRequestBody` is the shared transport value in `resource_loader.rs`.
Navigation POST requests, fetch requests, and content-worker load messages use
the same typed representation. The parent serializes bytes as base64 only for
the bounded IPC frame; the worker decodes them once before invoking reqwest.

`NativeDocument` now retains file-valued form pairs through encoding instead
of flattening every control to text. Multipart encoding is byte-oriented and
checks boundary collisions against field names, file names, and file bytes.
The JavaScript host uses the same body bound for FormData, Blob, and typed
array request bodies, while keeping UTF-8 text as a separate convenience
representation for diagnostics and text-only endpoints.

The reusable element refresh path resolves `File` and `FileList` constructors
through the current global realm. This preserves `instanceof` identity when
the host rehydrates a document after an action, rather than leaving reused DOM
wrappers attached to constructors from an earlier bootstrap.

## Tradeoffs and follow-up

The implementation copies and base64-encodes bounded bodies, which makes the
parent/worker contract deterministic and avoids exposing filesystem paths, at
the cost of peak memory and CPU overhead. Streaming uploads, chunked transfer,
multipart parsing for responses, PUT/PATCH/DELETE request methods, richer
`Request` body locking, and full Fetch/HTML form conformance remain separate
promotion work. Redirects retain the existing native method/body policy and do
not silently replay a body outside that policy.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_selected_file_reaches_fetch_and_form_navigation -- --exact --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine upload -- --nocapture` (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_submission -- --nocapture` (2 passed, 0 failed)
- `git diff --check`

Remote CI, push, release, tag, registry publication, and final production
parity claims are not made by this local checkpoint.
