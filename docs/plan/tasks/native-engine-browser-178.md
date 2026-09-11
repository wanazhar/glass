# Native engine browser slice 178: DOM fragment construction and child mutation ergonomics

Status: completed locally.

## Objective

Make detached DOM construction useful across every native execution realm
already supported by Glass. A script must be able to build a subtree in a
`DocumentFragment`, insert it into a document, move existing nodes out of the
fragment, and use the standard sibling/child replacement helpers without
leaking stale ownership or projected content.

## Contract

- `document.createDocumentFragment()` returns a node with `nodeType` 11,
  `nodeName` `#document-fragment`, `ownerDocument`, detached parent semantics,
  and `Node`/`DocumentFragment` prototype identity.
- Fragment `appendChild()` and `insertBefore()` flatten nested fragments and
  transfer their children in source order; appending a fragment returns the
  fragment and leaves it empty.
- Elements and fragments expose `append()`, `prepend()`,
  `replaceChildren()`, and the supported node targets expose `before()`,
  `after()`, and `replaceWith()` with text and element arguments.
- Local, process-backed HTTP(S), and same-origin projected frame realms retain
  ownership, parent/sibling links, detached-node behavior, and cache/removal
  semantics after fragment insertion and replacement.
- Moving the final child out of a fragment or removing the final attached child
  synchronizes empty markup/text content instead of retaining a stale snapshot.
- Created-but-never-attached nodes do not generate invalid Rust removal commands;
  existing attached nodes moved through a fragment still commit their removal
  and cache invalidation when appropriate.

## Implementation

- Added local and frame fragment factories with standard child and sibling
  mutation helpers, nested-fragment flattening, cycle rejection, ownership, and
  text/markup accessors.
- Added native `DocumentFragment` construction and shared tree accessors for
  node-type-aware append, insertion, replacement, and removal behavior.
- Extended local/content/frame synchronization to recognize fragment parents,
  avoid calling document-only sync hooks on detached fragments, and explicitly
  clear empty content after the last child is removed.
- Kept fragment staging in the JavaScript host until attachment; no second Rust
  staging tree or untyped cross-process mutation protocol was introduced.

## Tradeoffs and follow-up

This slice prioritizes the construction and attachment workflows used by page
scripts while preserving Rust ownership of attached document state. Fragment
`innerHTML` setters and direct `MutationObserver` records for detached staging
are not implemented in this slice, and the exposed properties are not a claim
of complete Web IDL descriptor parity. Those surfaces, broader parser/layout
integration, and browser-wide parity remain issue #40 promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine document_fragments -- --nocapture`
  (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `2cd5cd7b`
