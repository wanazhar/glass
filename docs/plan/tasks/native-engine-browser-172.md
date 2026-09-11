# Native engine browser slice 172: generation-safe frame lifecycle delivery

Status: completed locally.

## Objective

Preserve child document lifecycle and resource-load observations across the
content-worker boundary, and prevent effects from an outgoing document from
being resolved against the replacement document's DOM projection.

## Contract

- Page-load event order is retained as typed metadata alongside the loaded
  document: resource `load` events, interactive `readystatechange` and
  `DOMContentLoaded`, complete `readystatechange` and window `load`.
- The receiving engine commits those event indexes against its own document
  generation and validates every non-window target before adding it to the
  effect journal.
- Frame bindings and Window snapshots carry the current native document
  generation. A projected frame event with a stale generation is ignored for
  DOM/document targets, while window-level lifecycle events remain deliverable
  across navigation to the stable Window projection.
- The event wire remains bounded and typed; no JavaScript object, native
  document pointer, or unvalidated page string crosses the worker boundary.

## Implementation

- Added page-load event collection to the shared page-script result and
  included the collection in content-worker load responses.
- Decoded load event metadata through the same bounded event validator used by
  mutations, and preserved event effects produced while resolving page-script
  fetches.
- Committed content-worker and local page-load effects at the new document
  revision after outgoing lifecycle effects and before `pageshow`.
- Added document-arena validation for initial event targets and generation
  metadata to frame bindings, parent/top Window snapshots, and projected event
  descriptors.
- Added an HTTP witness for child window `load` delivery and an outgoing
  `beforeunload` DOM effect that must not leak into the replacement document.

## Tradeoffs

Window-level events intentionally survive a document generation change because
the browser's Window object remains the stable frame event surface; DOM and
document targets do not, so stale effects are dropped rather than guessed onto
new nodes. The current page-load event set is explicit and bounded; complete
resource scheduling, observer APIs, cross-realm identity, and the remaining
Web IDL/browser parity work remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_resource_load_events_before_dom_content_loaded -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `git diff --check`
