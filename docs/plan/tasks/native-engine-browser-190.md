# Native engine browser slice 190: DOM node identity and normalization

Status: completed locally.

## Objective

Extend the shared DOM projection with generic node identity, structural
comparison, and text normalization behavior needed by ordinary browser code.

## Contract

- `cloneNode(false)` creates a detached element, text node, or document
  fragment copy with the source element's attributes and no children.
- `cloneNode(true)` recursively copies the supported element/text/fragment
  tree, preserving child order and detached ownership without aliasing the
  source nodes.
- `isSameNode()` reports host object identity and `isEqualNode()` compares the
  supported node kind, element name/attributes, text value, and descendants.
- `compareDocumentPosition()` reports disconnected nodes, ancestor/
  descendant containment, and deterministic document order within one native
  tree using the standard bit flags.
- `normalize()` removes empty text children and merges adjacent text children,
  committing the resulting removal/text writes through the existing typed
  command batch. It also works for a detached constructed parent during the
  same script evaluation.
- Local, HTTP(S) content-worker, and same-origin frame projections expose the
  same bounded behavior without sharing Rust pointers or crossing the process
  boundary with untyped state.

## Implementation

- Added the shared node methods to the local and projected DOM accessor layer,
  including bounded recursive equality and preorder comparison helpers.
- Reused the existing element/text/fragment factories and typed commands for
  clone construction, so cloned nodes participate in the current-turn parent,
  root, and serialization views.
- Updated Rust script-node removal validation to permit nodes created in the
  current batch even when their parent is a detached script-created tree;
  ordinary non-script targets still require an attached native node.
- Added local, content-worker, and same-origin frame witnesses for cloning,
  identity, position flags, and detached-tree normalization.

## Tradeoffs and follow-up

The slice intentionally covers the element/text/fragment node kinds already
owned by the bounded native tree. Document cloning, comments, processing
instructions, namespace-aware equality, shadow-root composition, and complete
Web IDL descriptor parity remain separate issue-40 promotion work. Clones are
constructed with the existing temporary script-index protocol; the next host
refresh reprojects committed attached nodes from the Rust-owned snapshot.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `0041ee1c`
