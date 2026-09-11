# Native engine browser slice 194: document-type construction

Status: completed locally.

## Objective

Close the document-type construction gap after parsed doctype projection.
Native page code must be able to create a `DocumentType`, attach it to the
document root, clone it, and observe the same object after a host projection
refresh in local, content-worker, and same-origin frame realms.

## Contract

- `document.implementation.createDocumentType(name, publicId, systemId)`
  returns a bounded `DocumentType` with node type 10, name, public identifier,
  system identifier, null node value/text content, and the existing native
  node identity semantics.
- A created document type can be inserted into the document with
  `appendChild`/`insertBefore`; the Rust transaction validates root ownership,
  rejects non-document parents, and rejects a second document type.
- Document-type metadata and serialization survive the typed command,
  persistent snapshot, content-worker wire, and same-origin frame batch.
- `cloneNode()` creates an equivalent detached document type, and a committed
  created type is reused as `document.doctype` across the next host refresh.
- Parsed doctype removal updates live root-child projections before a replacement
  type is inserted; comments, visible text, layout, and paint behavior remain
  unchanged.

## Implementation

- Added validated `CreateDocumentType` and document-root insertion handling to
  the native command transaction, including duplicate and hierarchy guards.
- Added local and frame `DocumentType` factories, bounded
  `DocumentImplementation.createDocumentType`, root append/insert methods,
  clone support, serializer metadata, and persistent alias refresh.
- Added local, HTTP(S) content-worker, and same-origin-frame witnesses for
  creation, replacement, metadata, cloning, attachment, and cross-evaluation
  identity; nested-frame regression remains green.

## Tradeoffs and follow-up

This slice implements the bounded document-type creation path, not the entire
DOM Implementation API. Additional DOMImplementation factories, complete
document tree-builder constraints, malformed markup recovery, detached garbage
collection, complete Web IDL descriptors, and browser-wide conformance remain
issue #40 promotion work. No CDP path or fallback behavior changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_creates_and_persists_document_types -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_comments_and_doctype_nodes -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain -- --nocapture`
  (1 passed, 0 failed)

Implementation checkpoint: `487a4df3`.
