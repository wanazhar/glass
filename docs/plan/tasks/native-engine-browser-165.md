# Native engine browser slice 165: live DOM tree identity

Status: completed locally.

## Objective

Make structural DOM relationships observable through live, owner-backed Web
IDL-style collections in the top-level and same-origin frame script realms.

## Contract

- `childNodes` exposes a live `NodeList`; `children` exposes a live
  `HTMLCollection` filtered to element children.
- A collection retained by script observes later append, insert, remove,
  replace, and subtree-move operations through `length`, indexed access,
  iteration, `item()`, and `namedItem()` without a refresh or a new collection
  object.
- Element and text hosts expose first/last child, first/last element child,
  next/previous sibling, next/previous element sibling, `hasChildNodes()`,
  `contains()`, `replaceChild()`, and `isConnected` over the current owner
  tree.
- The same tree contract is installed for local/content-worker page realms
  and projected same-origin frame documents. Structural commands remain the
  only cross-realm transfer; no JavaScript object or native arena pointer is
  shared.
- Existing Rust-owned DOM mutation, temporary-node quotas, selector filtering,
  and frame-origin validation remain authoritative.

## Implementation

- Added a Proxy-backed live collection view that keeps native `NodeList` and
  `HTMLCollection` prototypes while resolving its contents from the current
  owner child list.
- Added the common tree-accessor installer for child collections, traversal,
  containment, replacement, and connectivity across local and frame hosts.
- Connected document root child lists and frame projected root lists so
  `parentNode`, sibling traversal, and document containment include the root
  boundary without changing `parentElement` semantics.
- Added top-level and same-origin frame integration assertions covering live
  collection updates, text-vs-element filtering, sibling identity,
  replacement, containment, connectivity, and post-commit child identity.

## Tradeoffs

The live view uses a bounded JavaScript Proxy over the existing owner child
array, so retained collections remain current without copying a full snapshot
on every access. Array methods operate over the current view at call time, and
the exposed collection is intentionally limited to the already-supported
element/text node set. Full Web IDL property descriptors, parser-backed text
node identity for all existing markup, mutation observers, fragments, ranges,
and browser-wide parity remain separate work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
- `git diff --check`
