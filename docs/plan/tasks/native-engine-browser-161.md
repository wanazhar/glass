# Native engine browser slice 161: subtree text mutation

Status: completed locally.

## Objective

Make script-visible `textContent` and `innerText` writes commit to the native
document owner. The mutation must update the real document tree used by
selectors, layout, visible-text evidence, and later evaluations in local,
content-process, and same-origin frame realms.

## Contract

- Setting `Element.textContent` or `Element.innerText` replaces the element's
  existing child subtree with one bounded native text node, or an empty child
  list for an empty value.
- Replaced element and text descendants become detached from the document;
  they no longer appear in selectors, semantic projections, layout, frame
  discovery, or later page snapshots.
- Unaffected nodes retain their arena identities and revision-bound references.
- Values are capped by the native text limit and are committed transactionally
  with the surrounding script batch. The same command is used by local,
  content-worker, and same-origin frame paths.
- The frame bridge keeps its existing same-origin and process-boundary checks;
  no JavaScript object or native engine pointer crosses a realm.

## Implementation

- Added `SetTextContent` to the typed script command protocol and the native
  frame command allow-list.
- Added bounded subtree replacement to `NativeDocument`, including stable
  arena tombstones and attached-node filtering for stale descendants.
- Added live host accessors for `textContent` and `innerText`, including the
  projected same-origin frame elements and the internal frame command entry
  point.
- Kept the content-process wire shape compatible by representing detached
  arena entries as unlinked nodes while snapshots expose only attached nodes.

## Tradeoffs

Stable arena indices avoid invalidating already-held unaffected element
objects, at the cost of retaining bounded detached tombstones until the next
document generation. Mutations of detached objects, dynamic element creation,
`innerHTML`, and general live child-node collections remain separate DOM
behavioral units so this slice does not invent a second parser or silently
claim full tree-mutation parity.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --lib script_text_content_replaces_subtree_and_detaches_old_nodes`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
