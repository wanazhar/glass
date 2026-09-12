# Native engine browser slice 236: pointer-events hit testing

Status: completed locally.

## Objective

Make visual overlays and pointer targeting obey one CSS-controlled contract.
`pointer-events:none` must allow a point click to reach a lower painted
element, while a descendant with an explicit `pointer-events:auto` can remain
interactive through the inherited disabled ancestor.

## Contract

- The typed CSS cascade accepts inherited `pointer-events:auto|none` values in
  stylesheet and inline declarations, including `initial`, `unset`, `revert`,
  `revert-layer`, `inherit`, and author `!important` through the existing
  inherited declaration path.
- Computed style walks carry the resolved value through local, HTTP(S)
  content-worker, and same-origin frame document projections without changing
  the two-crate boundary or wire protocol shape outside the existing computed
  style record.
- Every emitted layout box records whether pointer targeting is allowed. A
  `none` box is skipped by point hit testing even though its background,
  border, text, and descendants remain eligible for their own computed values.
- Because the property is inherited, descendants remain non-targetable unless
  they explicitly resolve `pointer-events:auto`; the explicit descendant
  override is still targetable through the parent box.
- Paint, raster, capture, semantics, scrolling, layout flow, and z-index paint
  order remain unchanged. Only the shared layout hit-test candidate filter is
  extended.

## Implementation

`css.rs` adds `NativePointerEventsValue`, inherited-style propagation,
declaration parsing/diagnostics, stylesheet and inline cascade application,
and the computed-style accessor. `dom.rs` carries the value through the
existing ancestor style walk. `layout.rs` stores targetability beside the
shared stacking metadata and rejects non-targetable boxes before rounded-shape
candidate selection. Integration witnesses prove pass-through of a painted
topmost overlay and explicit re-enablement of a descendant.

## Tradeoffs and follow-up

This slice intentionally covers the HTML-style `auto|none` interaction values
needed by native Glass pointer targeting. SVG-specific `visiblePainted`,
`visibleFill`, `bounding-box`, pointer capture, event propagation details,
hover/active pseudo-state, and full CSS hit-region geometry remain later
browser-profile work. They must extend the same computed-style and hit-test
owners rather than adding a parallel pointer path.

The inherited value keeps the common overlay behavior predictable: authors can
disable a whole surface and opt a specific control back in. Non-targetable
boxes are still painted, so visual output and pointer routing do not silently
diverge.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_pointer_events -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_ -- --nocapture` (21 passed, 0 failed)

Implementation checkpoint: `487a93cd`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
