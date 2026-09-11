# Native engine browser slice 197: Attr Node semantics

Status: completed locally.

## Objective

Make the native `Attr` object participate in the shared bounded `Node`
contract. Attribute nodes must behave like ordinary detached nodes for the
identity, comparison, cloning, root, and collection APIs used by page code.

## Contract

- An `Attr` exposes the shared node accessors and returns an empty live
  `NodeList` for `childNodes` without becoming an element-tree child.
- `cloneNode()` creates an independent same-document `Attr` with the same
  qualified name and value; `isEqualNode()` compares attribute identity data
  without treating two different nodes as the same object.
- `getRootNode()` and `isConnected` preserve detached attribute semantics,
  while `ownerElement` remains the only ownership link.
- The behavior is shared by local, HTTP(S) content-worker, and same-origin
  frame realms through the common JavaScript host implementation.

## Implementation

- Installed the shared tree-accessor layer on constructed attribute nodes.
- Added the attribute branch to bounded node cloning and equality comparison.
- Extended the node-kind dispatch to include node type 2 while retaining the
  existing element-only mutation and selector operations.
- Made the live `NamedNodeMap` surface lazily adopt its constructor prototype,
  so parsed elements created before constructor bootstrap and elements created
  afterward expose the same `instanceof NamedNodeMap` contract.
- Expanded the local integration witness for clone, equality, root, parsed
  collection, and collection behavior; the frame and content-process
  regressions remain green.

## Tradeoffs and follow-up

Attributes remain host-backed objects rather than independent Rust arena
nodes, and their empty child collection is intentionally immutable from the
tree mutation surface. Namespace-aware attribute storage, XML documents,
complete Web IDL descriptors, and browser-wide conformance remain issue #40
work. No CDP or fallback path changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine recovered_malformed`
  (2 passed, 0 failed; confirms the HTML-recovery contract exercised by the
  shared integration target)

Implementation checkpoint: `77eb3fee`.
