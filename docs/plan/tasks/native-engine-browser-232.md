# Native engine browser slice 232: absolute positioning

Status: completed locally.

## Objective

Extend the bounded positioning owner from in-flow relative translation to
basic out-of-flow `position: absolute` placement. Absolute children must no
longer consume block, flex, or grid allocation, and their complete emitted
subtrees must move through the same layout geometry consumed by paint,
overflow, scroll projection, capture, and hit testing.

## Contract

- `position` accepts `static`, `relative`, and `absolute`. `fixed` and
  `sticky` remain diagnostic failures until their viewport/scroll and flow
  contracts have dedicated shared metadata.
- `top`, `right`, `bottom`, and `left` retain the bounded signed integer
  pixel/`auto` grammar from slice 231. When both edges on an axis are
  specified, the primary edge wins (`left`/`top`); otherwise the opposite edge
  positions from the containing-block edge. Negative values are handled by
  the existing bounded unsigned document-coordinate policy.
- A direct absolute child is removed from normal block flow. It does not
  advance the flow cursor, affect a flex row/column item list, consume a grid
  cell, or change sibling placement. It remains in source order for the
  existing deterministic paint and hit-test tie break.
- The nearest ancestor with non-static positioning establishes the containing
  block. Flex and grid containers also establish a containing block for their
  absolute children. Without such an ancestor, the initial native viewport
  containing block is used.
- The containing-block origin is its bounded padding-box origin. Explicit
  `left`/`top` offsets include the positioned child's leading margin; explicit
  `right`/`bottom` offsets subtract its trailing margin and resolved outer size.
- An absolute child is initially laid out once, then its box/text range is
  translated after its resolved size is known. Descendant geometry, styles,
  opacity groups, display-list commands, clipping, capture, and hit testing
  therefore share that final translated range.
- Stylesheet and inline declarations, source order, `!important`, CSS-wide
  resets, CSSOM mutation, local documents, and HTTP(S) content-worker style
  transfer continue to use the existing typed computed-style owner.

## Implementation

`css.rs` adds the absolute position enum value and parser recognition while
retaining the existing typed offset/cascade/transfer path. `layout.rs` adds a
bounded containing-block context, removes absolute children from normal flow,
and resolves their edge placement against the context after one subtree layout
pass. Optimized flex and grid item collection filters absolute children and
lays them through the shared positioned-child helper after ordinary placement.
The parent's flow size remains independent of the out-of-flow child while the
global document bounds continue to see emitted boxes for scrollable overflow.

The integration tests cover initial containing-block placement, nearest
positioned padding-box placement, normal-flow non-reflow, deterministic paint
and hit ordering, and flex/grid filtering. No new crate, dependency, renderer,
wire schema, or process boundary is introduced.

## Tradeoffs and follow-up

This slice deliberately keeps the existing integer-pixel offset grammar and
unsigned document coordinates. It supports one bounded absolute layout pass
with source-order painting; full stacking-context ordering, `z-index`,
percentage/viewport-relative insets, auto-size shrink-to-fit rules, static
position rectangles, transforms, containing-block effects from transforms,
and fragmentation remain separate work. `fixed` is not parsed by this slice:
its viewport lock must survive root scrolling in layout projection, display
list, raster, and hit testing, so accepting it as ordinary absolute geometry
would be an incorrect public contract.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_absolute_position -- --nocapture` (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_relative_position -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_background -- --nocapture` (5 passed, 0 failed)

Implementation checkpoint: `63a57b5d`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
