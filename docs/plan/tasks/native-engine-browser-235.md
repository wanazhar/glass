# Native engine browser slice 235: z-index stacking

Status: completed locally.

## Objective

Give positioned overlays a shared, deterministic stacking contract. A
`z-index` declaration must survive the native CSS cascade and control the
relative order used by the layout snapshot, display list, software raster, and
point hit testing.

## Contract

- `z-index` accepts `auto` and bounded signed integer values from
  `-1_000_000` through `1_000_000`. CSS-wide reset keywords and `inherit` use
  the existing local cascade declaration path; stylesheet, inline, and
  `!important` declarations share the existing cascade owner.
- An explicit non-`auto` value establishes a local effective stacking level for
  positioned elements and direct children of flex/grid containers. Static-flow
  elements outside those flex/grid-item positions retain the current level.
- Effective levels are copied onto every emitted box and text run in the
  subtree. Nested positioned/flex/grid levels add to the active level with
  saturating arithmetic, so a subtree remains ordered with its owning overlay.
- Display-list entries are stably ordered by effective level from low to high;
  source order remains the tie-breaker. Existing opacity groups remain atomic
  at their outermost level while their begin/end boundaries are reordered with
  the group.
- Point hit testing compares the same effective level first, then layout depth,
  then emitted source order. Paint and interaction therefore select the same
  topmost overlay for equal geometry.
- No second renderer, display-list schema, process, dependency, or CDP path is
  introduced. Layout, paint, raster, capture, script geometry, and hit testing
  continue to consume one native snapshot.

## Implementation

`css.rs` adds the typed `NativeZIndexValue`, bounded parser, diagnostics
recognition, declaration storage, stylesheet/inline cascade application, and
computed-style accessor. `layout.rs` carries an active stacking level through
the existing recursive builder, recognizes positioned and direct flex/grid
items, and exposes the level on boxes/text while preserving normal flow.
`paint.rs` derives an optional stable index order only when a non-default level
exists; it keeps opacity groups contiguous and retains the previous source
order fast path for ordinary documents. `NativeLayoutSnapshot::hit_test`
uses the shared box level before depth/source order. Integration witnesses
exercise inline overlays, stylesheet values, and author `!important`.

## Tradeoffs and follow-up

The scalar effective level is deliberately cheap and deterministic for the
current single-root native profile. Full CSS stacking-context tuples,
`isolation`, transforms, filters, blend modes, top-layer dialogs, nested
scrolling contexts, and compositor layer promotion remain later rendering
work; they must extend this shared ordering owner rather than adding a parallel
paint or hit-test rule. Opacity groups are already treated as atomic outer
contexts so descendants cannot escape their group during this slice.

The integer bound prevents hostile or accidental values from creating
unbounded native state. The normal CSS `auto` path remains source/depth ordered
and does not allocate a sorting index.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_z_index_orders_positioned_overlays_for_paint_and_hit_testing -- --nocapture` (passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_ -- --nocapture` (19 passed, 0 failed)

Implementation checkpoint: `af9fb7b8`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
