# Native engine browser slice 174: projected frame mutation observation

Status: completed locally.

## Objective

Carry the mutation-observation contract through same-origin frame projections
so a parent realm can observe DOM changes it performs through
`contentDocument`, while preserving child-frame ownership and origin
boundaries.

## Contract

- Same-origin projected frame documents and elements participate in the shared
  `MutationObserver` registry; cross-origin documents remain undisclosed.
- Frame-qualified node identity includes the frame ID, so identical child and
  parent node indexes cannot alias in observer records or shadow state.
- Projected attribute, character-data, child-list, reparenting, removal, and
  text/HTML replacement commands produce records against the projected frame
  targets before the typed command is batched to the child owner.
- Detached projected nodes may be observed directly and remain valid record
  payloads after removal; the parent document observer does not receive
  detached-only mutations unless the node is attached beneath its target.
- Child command batching and same-origin validation remain unchanged; observer
  delivery does not transfer JavaScript objects or native document pointers.

## Implementation

- Added frame-qualified projected-node registries for attributes, text, parent,
  and child-order shadow state.
- Routed inner frame DOM commands through the observer record owner before
  `FrameScriptBatch` coalescing.
- Anchored frame root nodes to their projected frame document for correct
  subtree matching and registered detached frame construction nodes.
- Extended the existing same-origin frame integration with a non-destructive
  observer witness and a content-worker observer witness.

## Tradeoffs

This slice observes mutations made through the current realm's projected frame
objects immediately, which gives parent scripts a useful cross-realm contract
without pretending that a JavaScript object crosses the child owner boundary.
Effects caused by an independently running child task and layout/resource
observer APIs still need explicit delivery routes. The record queues retain
the existing native bounds and do not become an unbounded cross-process journal.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --exact`
  (1 passed, 0 failed, 441 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --exact`
  (1 passed, 0 failed, 441 filtered out)
- implementation commit `895ac196`
- worker witness commit `8e4a8f2e`
