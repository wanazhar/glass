# Native engine browser slice 164: same-origin frame construction

Status: completed locally.

## Objective

Complete the DOM-construction bridge for projected same-origin frame
documents. A parent page must be able to create detached elements and text
nodes in a child frame document, compose them, and commit the complete batch to
the child frame's own native document owner.

## Contract

- A projected same-origin frame document exposes bounded `createElement()` and
  `createTextNode()` factories plus `appendChild()`/`insertBefore()` on its
  projected elements.
- Parent projections retain same-evaluation parent, text, and markup identity;
  constructed nodes become queryable only after the child owner commits and a
  fresh frame snapshot is observed.
- Consecutive structural frame commands are grouped into one typed
  `FrameScriptBatch`. The child realm queues the bounded inner commands into
  its normal Rust transaction, preserving temporary-node resolution and one
  native revision for the batch.
- Batch routing reuses the existing source/target origin validation and never
  transfers a JavaScript object, arena pointer, or content-process handle.
- Frame construction supports element and text children, validated hierarchy
  cycles, deterministic reference-node ordering, and subtree moves; unrelated
  focus/click event commands retain their existing event-aware route.

## Implementation

- Added a typed frame construction batch and bounded source generation for the
  child realm's command buffer.
- Added local child-projection parent/child state, attachment tracking,
  factories, markup synchronization, and detached text-node hosts.
- Reused Rust `NativeDocument` temporary-index allocation and structural
  mutation, so frame construction has the same quotas and ownership rules as
  top-level local/content-worker construction.
- Added focused HTTP same-origin frame coverage for detached creation, nested
  text insertion, parent identity, markup, and post-commit querying.

## Tradeoffs

Grouping structural commands adds one bounded wrapper and delays child
publication until the parent evaluation's normal frame-effect drain, but it is
necessary to keep temporary node identity meaningful across multiple child
operations. The projected frame remains a snapshot view and therefore does
not promise live collections yet; cross-origin frame access stays rejected by
the existing origin gate. Creation of document fragments, comments, custom
element upgrade reactions, and observer delivery remains separate work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
