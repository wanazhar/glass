# Native engine browser slice 244: frame-subtree locator actions

Status: completed locally.

## Objective

Make ordinary targeted actions work inside embedded native browsing contexts
without requiring the caller to select a frame first. Point input and focused
keyboard input already had frame routes; this slice gives semantic locators
the same browser-facing ownership.

## Contract

- Targeted `Click`, `Type`, `Clear`, `Check`, `Uncheck`, and `Select` actions
  search the currently selected frame and its attached descendant frames.
- A locator must resolve to exactly one element across that selected subtree;
  a duplicate match is rejected rather than routed by arbitrary frame order.
- A root-document match remains with the ordinary selected-frame dispatcher;
  a child-document match is executed through the existing parked-frame route.
- Child actions retain the existing revision, focus, event, navigation,
  popup, script-effect, and browser-effect ownership; the public Glass context
  ID remains unchanged.
- The existing locator grammar and stale-reference behavior remain the
  authority. Frame discovery/reconciliation occurs before the cross-frame
  lookup so detached child documents cannot receive an action.
- Point targets and key actions continue through their dedicated frame-tree
  routing paths; this slice does not duplicate either path.

## Implementation

`NativeEngineBackend::dispatch_locator_action` converts the targeted semantic
action once, reconciles the selected frame subtree, and asks each live native
engine to resolve the same locator. It rejects ambiguity across frame
documents and dispatches a unique child match via `apply_action_to_native_frame`.
Accepted child actions establish that frame as the focused owner and drain
the existing event, frame-script, popup, navigation, and window-effect queues.
`NativeEngine::resolve_target` exposes the document resolver to this adapter
without exposing document state.

The nested-frame integration witness now clicks into a grandchild input,
types, traverses focus forward and backward, and then clicks a second
grandchild input by `id=next` while the parent context remains selected.

## Tradeoffs and follow-up

Lookup is deliberately scoped to the selected frame subtree, matching the
existing public selected-frame model; it does not silently reach an ancestor
frame or another page target. Locator resolution still uses the native
engine's declared ID/name/role/text/CSS grammar and does not add shadow-tree
or cross-origin DOM access. Global browsing-context focus order, richer
locator semantics, and frame-aware preflight/evidence are subsequent Issue
#40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_routes_point_clicks_into_nested_frame_content -- --exact --nocapture` (1 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: local changes after `f7bb63c8`; commit follows the
documentation gate.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
