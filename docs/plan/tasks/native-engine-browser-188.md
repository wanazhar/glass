# Native engine browser slice 188: make document titles live

Status: completed locally.

## Objective

Close the document metadata gap where `document.title` was only a bootstrap
snapshot and writes could not become durable native document state.

## Contract

- `document.title` reads the current title element when one exists.
- Assigning `document.title` updates an existing title through the shared
  bounded text-content mutation path.
- Assigning a title to an HTML document without a head/title materializes the
  missing nodes through the existing native DOM command protocol.
- A document without an HTML root still accepts a bounded title write through a
  typed Rust-owned command and exposes the title on the next host refresh.
- Local, HTTP(S) content-worker, and same-origin frame documents share the
  contract and preserve title state across host refreshes.

## Implementation

- Added `SetDocumentTitle` to the typed native script command protocol.
- Added Rust validation and title-node materialization, including bounded
  depth/node-limit checks and the no-HTML-root fallback.
- Replaced local and frame snapshot title fields with live accessors and
  setters that reuse existing DOM preview/commit commands where possible.
- Allowed title commands through same-origin frame routing.
- Added local, HTTP(S), frame, and direct Rust fallback witnesses.

## Tradeoffs and follow-up

The implementation keeps one Rust-owned DOM arena and uses the existing
temporary-node command path, so title changes do not introduce a second
metadata store. A document with no HTML root receives a title node directly
under the document root; this is a bounded recovery behavior rather than a
claim of complete HTML tree-builder conformance. Full Web IDL descriptors,
parser insertion modes, and the remaining issue #40 promotion gates remain
open.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine script_document_title_materializes_missing_title_node -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `5494e7bc`
