# Native engine browser slice 238: history-backed nested scroll restoration

Status: completed locally.

## Objective

Make nested element scrolling survive browser history traversal through both
same-document state changes and full resource navigation. A native page must
return to the root and element scroll positions associated with the activated
history entry, including when the document is rebuilt by the content process.

## Contract

- Every native history entry retains the root viewport offset together with a
  bounded map of non-zero element-owned scroll offsets.
- Root and nested offsets are updated whenever a committed local or
  content-process mutation changes scroll state. Same-document `pushState`,
  `replaceState`, fragment navigation, and link activation capture the current
  pair without creating a second scroll owner.
- Same-document and full-resource traversal clamp saved offsets against the
  activated document's current layout. Removed scrollers are discarded, and
  changed scroll ranges restore their usable portion rather than making the
  next layout invalid.
- HTTP(S) history activation synchronizes the restored root and nested state
  into the content process before `popstate` or subsequent script evaluation.
  Local and content-process navigation therefore expose the same restored
  `window` and element scroll values.
- The bounded map uses document-local node indexes and is deterministic. It
  does not claim cross-frame scroll propagation, browser session-history
  persistence, scroll anchoring, or unrestricted browser history semantics.

## Implementation

`history.rs` stores the per-entry nested offset map and copies it through
push/replace/update operations. `engine.rs` captures nested state at every
history update, clamps and filters it against the target layout on activation,
and synchronizes the result through the existing content-process scroll wire
before lifecycle events. The existing layout, JavaScript, paint, raster, and
content-process owners remain the only scroll state owners.

Regression witnesses cover local same-document traversal, local full-resource
traversal, HTTP(S) content-process same-document traversal, and HTTP(S)
content-process full-resource traversal. They also assert the parent root
offset, not only the child JavaScript projection.

## Tradeoffs and follow-up

History entries retain only non-zero nested offsets, bounded by the existing
history and DOM limits. Node indexes are sufficient for the deterministic
document representation used by this profile; a future persistent session
history format would need a stronger element identity and storage policy.
Unknown or out-of-range entries are safely dropped or clamped when a target
document is activated. This keeps traversal usable after DOM or content
changes while avoiding stale layout references.

Cross-frame scroll propagation, scroll anchoring, scroll restoration modes,
scrollbar interaction, smooth scrolling, and browser-wide session-history
conformance remain later issue #40 work. They must extend the current scroll
container/history owners rather than introducing a parallel renderer or state
path.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine history_restores_nested_scroll_offsets -- --nocapture` (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_same_document_history_api -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_history_restores_nested_scroll_offsets_across_resource_navigation -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine history -- --nocapture` (16 passed, 0 failed)

Implementation checkpoint: `10090e0b`.

Remote CI, push, release, tag, registry publication, browser-parity, and
production-promotion claims are not made by this local checkpoint.
