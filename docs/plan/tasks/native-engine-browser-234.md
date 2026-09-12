# Native engine browser slice 234: sticky positioning

Status: completed locally.

## Objective

Complete the first scroll-anchored member of the CSS positioning family. A
`position: sticky` element must retain its normal-flow allocation, move with
the document until its inset reaches the root scrollport, and stop at the
nearest layout ancestor's boundary. Its entire emitted subtree must use the
same projected geometry for layout inspection, paint, raster, capture, and
hit testing.

## Contract

- `position` accepts `static`, `relative`, `absolute`, `fixed`, and `sticky`
  through the existing typed cascade, CSS-wide reset, stylesheet, inline,
  `!important`, CSSOM, and content-process transfer paths.
- `top`, `right`, `bottom`, and `left` retain the bounded signed integer
  pixel/`auto` grammar. Leading and trailing insets are applied to their
  corresponding root-scroll viewport axes; omitted insets leave that axis at
  its normal-flow position.
- Sticky roots remain ordinary block, flex, and grid items. They consume the
  same flow height and cross-axis allocation as their unshifted geometry, so
  following siblings do not reflow when the root scroll offset changes.
- Layout records one contiguous sticky subtree range after final sizing and
  line/cross-axis placement. The range includes descendant boxes and text,
  while fixed descendants retain their independent viewport projection.
- Each root-scroll snapshot computes a signed translation from the normal
  root rectangle, the nearest emitted layout ancestor, the root scrollport,
  and the authored insets. Translation is clamped so the sticky box stays
  within both the scrollport inset region and its containing-block boundary.
- After projected geometry is applied, aggregate overflow clips are rebuilt
  from the shifted boxes. Clips owned inside a sticky subtree move with it;
  clips owned by stationary ancestors remain stationary. The existing common
  document-to-viewport subtraction is still applied exactly once.
- Native layout, display-list, software raster, PNG capture, script geometry,
  and point hit testing all consume this one snapshot. No renderer, wire
  schema, dependency, crate, process, or CDP path is added.

## Implementation

`css.rs` adds the typed `sticky` position value. `layout.rs` records the
outermost sticky range, exposes sticky membership on emitted boxes and text
runs, and stores the normal rectangle plus nearest layout-ancestor constraint
for projection. `NativeLayoutSnapshot::with_scroll_offset` applies the fixed
projection first, applies the signed sticky clamp to non-fixed members, and
recomputes overflow clips through the existing document-aware owner. The
engine and document snapshot callers pass the document into projection so
clip ownership remains correct after movement.

## Tradeoffs and follow-up

This slice intentionally models one root scrollport and uses the nearest
emitted layout ancestor as the bounded sticky containing block. Nested
scroll-container ownership, scroll-padding, percentage/viewport units,
logical writing-mode insets, transformed containing blocks, margin collapsing,
multiple independent sticky roots with full stacking order, compositor layer
promotion, animation, and print-media behavior remain separate browser-profile
work. Those follow-ups must extend the same projection/clip owner rather than
introducing a second sticky renderer.

The bounded integer grammar keeps this projection deterministic and cheap, but
it does not imply browser-wide CSS conformance. In particular, fixed
descendants are not double-shifted when an outer sticky range moves, and the
range remains flow-preserving even when the projected box is temporarily
outside the viewport.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_sticky_position -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_ -- --nocapture` (17 passed, 0 failed)

Implementation checkpoint: `c93d7955`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
