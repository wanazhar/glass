# Native engine browser slice 243: sequential frame focus traversal

Status: completed locally.

## Objective

Make native keyboard focus move through the page's sequentially focusable
controls when a browser shortcut sends `Tab` or `Shift+Tab`. This closes the
gap after slice 241's frame focus ownership: a pointer click can establish a
focused child frame, and the next keyboard action can now move focus within
that child document without selecting the frame publicly.

## Contract

- `Tab` advances through attached, visible, enabled native controls in
  document order; positive `tabindex` values precede the natural zero-order
  controls and are ordered numerically.
- `Shift+Tab` traverses the same sequence in reverse, including bounded
  end-to-start wrapping when the sequence has a focused control.
- Negative `tabindex` values and unsupported, hidden, disabled, or detached
  nodes are excluded from the sequential sequence.
- The default action preserves the existing keydown/default/keyup transaction
  and emits blur/focus effects through the existing local event or
  content-process event bridge, so preventDefault still suppresses traversal.
- A focused parked frame receives the shortcut through the backend's existing
  frame route; the public selected frame and context identity do not change.
- Local fixture documents and external HTTP(S) content-process documents use
  the same DOM focus algorithm and event-effect ordering.

## Implementation

`NativeDocument::apply_tab_focus` builds a bounded sequential focus list from
the attached semantic controls, applies the positive-`tabindex` ordering, and
delegates state changes to the existing `focus_element` owner. Local shortcut
dispatch calls it for `Tab` defaults, while the content worker applies the
same operation after the bridged keydown has allowed its default action.

The nested-frame backend test now clicks into a grandchild input, types into
it, advances to a second input with `Tab`, and returns with `Shift+Tab`. The
HTTP content-process keyboard test covers the same two directions and checks
the active element plus the existing modifier/event contract.

## Tradeoffs and follow-up

The sequence is intentionally bounded to the native semantic control roles
already owned by the engine. It does not yet model shadow-root delegates,
contenteditable editing hosts, radio-group tab stops, platform-specific
focus rings, or a single global tab order that crosses browsing-context
boundaries. Those are separate browser-complete focus/interaction work; this
slice makes the current frame-local default action coherent and observable.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_routes_point_clicks_into_nested_frame_content -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_keyboard_actions_preserve_event_and_modifier_contract -- --exact --nocapture` (1 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: local changes after `ee8596cb`; commit follows the
documentation gate.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
