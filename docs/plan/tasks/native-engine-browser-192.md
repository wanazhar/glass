# Native engine browser slice 192: comment and document-type nodes

Status: completed locally.

The script-created comment follow-up identified in this historical checkpoint
was closed by `native-engine-browser-193`.

## Objective

Preserve parsed HTML comments and the document type as real native DOM nodes
through the Rust arena, typed wire, persistent script snapshot, and projected
JavaScript realms. Comments must participate in the supported CharacterData
surface without leaking into visible text or layout; the document type must be
observable through `document.doctype` and frame document projections.

## Contract

- The bounded host tokenizer retains HTML comments and basic case-insensitive
  `DOCTYPE` declarations with a name and optional quoted `PUBLIC`/`SYSTEM`
  identifiers.
- Native arena nodes, content-worker wire values, and script snapshots
  preserve comment (`nodeType === 8`) and document-type (`nodeType === 10`)
  identity and metadata, with the existing size and generation validation
  boundaries.
- Comments use the supported CharacterData node surface, including
  `textContent` mutation through the existing typed native transaction.
- `document.doctype`, comment parentage, `nodeName`, `nodeValue`, and the
  relevant `Comment`/`DocumentType` prototypes are projected in local,
  content-worker, and same-origin frame realms.
- Comments and doctypes are excluded from element visible text, raw-text
  aggregation, layout, and paint while remaining present in supported
  `innerHTML`/document serialization.
- Fragment parsing preserves comments and ignores a doctype declaration, as
  required by the bounded detached-fragment contract.

## Implementation

- Added comment and document-type variants to the native DOM, typed content
  wire, validation path, persistent script snapshot, and serializer.
- Added bounded comment/doctype tokenization and fragment/document parsing.
- Added `Comment` and `DocumentType` JavaScript wrappers, tree accessors,
  parent/owner projections, identity reuse, and frame projection support.
- Routed comment character-data edits through the existing native command and
  observer path while excluding comments from text/layout consumers.
- Added local, content-worker, same-origin-frame, and nested-frame witnesses
  covering type, identity, metadata, parentage, mutation, and visible-text
  behavior.

## Tradeoffs and follow-up

This slice adds the bounded parsed-node surface; it is not a full WHATWG
tree-builder or complete Web IDL implementation. Script-created comments and
document types, `DocumentType` mutation/`DOMImplementation`, malformed-comment
recovery, foreign content, raw-text edge cases, and browser-wide conformance
remain issue #40 promotion work. No CDP path or fallback behavior changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --lib preserves_comments_and_doctype_without_affecting_visible_text`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_comments_and_doctype_nodes -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain -- --nocapture`
  (1 passed, 0 failed)

Implementation checkpoint: `a8c87568`.
