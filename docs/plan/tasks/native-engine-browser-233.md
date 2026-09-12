# Native engine browser slice 233: fixed positioning

Status: completed locally.

## Objective

Complete the basic CSS positioning family by adding viewport-anchored
`position: fixed` semantics to the shared native layout projection. Fixed
subtrees must remain at their viewport coordinates across root scrolling,
must not consume normal block/flex/grid allocation, and must remain visible
outside ordinary ancestor overflow clips.

## Contract

- `position` accepts `static`, `relative`, `absolute`, and `fixed` in the
  bounded native positioning profile. `sticky` remains a separate flow-
  dependent slice because it changes behavior at a scroll threshold.
- `top`, `right`, `bottom`, and `left` retain the bounded signed integer
  pixel/`auto` grammar. Primary edges win when both axis edges are present;
  the opposite edge resolves against the viewport edge when the primary edge
  is `auto`. Margins are included using the same edge-placement helper as
  absolute positioning.
- A fixed child is out of normal block flow and is filtered from optimized
  flex and grid item collection. It is laid out after ordinary children in
  source order without changing sibling flow allocation.
- Fixed positioning uses the initial native viewport containing block even
  when the fixed element is nested under a positioned, flex, or grid ancestor.
  Its complete descendant box/text range inherits the fixed projection bit.
- When a layout snapshot receives a root scroll offset, every fixed box and
  fixed text origin is rebased by that offset before the existing common
  document-to-viewport projection. The public viewport rectangle, script
  geometry, display list, raster surface, capture, and hit test therefore keep
  fixed content at the same viewport coordinate.
- A fixed root does not inherit ordinary ancestor overflow clips. Clips owned
  by the fixed subtree itself remain active and move with the fixed geometry.
- Inline and stylesheet declarations, source order, `!important`, CSS-wide
  resets, CSSOM mutation, local documents, and HTTP(S) content-worker style
  transfer continue to use the existing typed computed-style owner.

## Implementation

`css.rs` adds the fixed position enum value and parser recognition. `layout.rs`
tracks the initial viewport containing block and a fixed-subtree bit on layout
boxes and text runs. Fixed children use the initial context during positioned
layout; `NativeLayoutSnapshot::with_scroll_offset` rebases the marked ranges
and cached clips. Overflow traversal stops at the outermost fixed root so
ordinary document ancestors cannot clip viewport content.

The existing display-list and raster owners remain unchanged: they continue to
consume document-space commands and subtract the layout scroll offset once.
Because fixed boxes are rebased before those consumers run, no command variant,
renderer, wire schema, dependency, crate, or process boundary is required.

The integration witness covers initial fixed placement, nested overflow
isolation, root-scroll anchoring, fixed metadata, viewport geometry, hit
testing, display-list coordinates, and software raster output.

## Tradeoffs and follow-up

This slice deliberately supports fixed positioning against the initial
viewport only. Transform-established fixed containing blocks, viewport units,
percentage insets, auto-size shrink-to-fit rules, `z-index`/stacking-context
ordering, compositor layers, print-media fixed behavior, and `position: sticky`
remain separate work. A fixed subtree is represented in the same bounded
integer document coordinate space and rebased at projection time, which keeps
the current render pipeline compact while making root-scroll behavior explicit.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_fixed_position -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_absolute_position -- --nocapture` (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_relative_position -- --nocapture` (2 passed, 0 failed)

Implementation checkpoint: `f7bbb297`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
