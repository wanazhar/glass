# Native engine browser slice 193: script-created comment nodes

Status: completed locally.

## Objective

Close the remaining comment-construction gap after parsed comment projection.
Page script must be able to create, attach, mutate, serialize, clone, and
reuse comment nodes in every native JavaScript realm that already supports
Rust-owned element and text construction.

## Contract

- `document.createComment(value)` returns a `Comment`/`CharacterData`/`Node`
  with node type 8, `#comment` naming, reflected `data`, `nodeValue`, and
  `textContent`, bounded by the existing native text limits.
- Created comments can be inserted into elements and detached fragments,
  including comments parsed by detached `innerHTML`; the typed command batch
  preserves creation, ordering, attachment, removal, and text mutation.
- Comment text is excluded from element `textContent`, visible text, layout,
  and paint while comment markup remains in supported serialization.
- Persistent local and HTTP(S) content-worker realms reuse the same comment
  object after host projection refresh, preserving native ownership and
  `===` identity; same-origin frame realms provide the corresponding
  constructor and command route.
- Fragment parsing retains comments and continues to ignore a doctype in the
  bounded fragment contract.

## Implementation

- Added the typed `CreateComment` command and validated native detached-node
  creation in the Rust transaction.
- Added local and frame `Comment` factories, `createComment()`, CharacterData
  accessors, identity refresh, serializer markup, and frame-batch support.
- Changed the shared detached-fragment parser to materialize comment nodes
  instead of silently dropping them; enabled comment insertion in element and
  fragment helpers and comment cloning.
- Updated the existing fragment test oracles to reflect retained comments and
  canonical double-quoted attribute serialization.
- Added local, content-worker, and same-origin-frame witnesses for creation,
  metadata, mutation, attachment, serialization, and cross-evaluation identity;
  nested-frame regression remains green.

## Tradeoffs and follow-up

The implementation remains bounded by the existing native node, command, and
text limits. Full malformed-comment recovery, comment token edge cases,
script-created document types, `DOMImplementation`, detached garbage
collection, complete Web IDL descriptors, and browser-wide conformance remain
issue #40 promotion work. No CDP path or fallback behavior changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_creates_and_persists_comment_nodes -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain -- --nocapture`
  (1 passed, 0 failed)
- Full native integration target: 451/452 passed before correcting one stale
  canonical-markup expectation; the corrected `native_content_process_evaluates_persistent_script_realm`
  witness then passed 1/1 in a focused rerun.

Implementation checkpoints: `010a8c67`, `64975240`.
