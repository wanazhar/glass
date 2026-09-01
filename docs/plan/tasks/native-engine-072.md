---
id: native-engine-072
scope: glass-browser/native-engine/flex-align-content-space-around
status: complete
depends-on: [native-engine-071]
---

# Native engine 072: bounded flex cross-line space-around

## Objective

Extend the completed 071 wrapped-row `align-content` owner with one additional
bounded value, `space-around`, while preserving the existing 064 through 071
line formation, visual order, fixed widths, margins, gap, justification,
cross-axis item alignment, direction, wrapping, shared artifact geometry, root
scrolling, and two-crate package boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-071` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, and bounded cross-line
alignment slices.

## Contract

The native CSS grammar adds the non-inherited keyword `space-around` to
`align-content`. The existing accepted values remain
`flex-start|center|flex-end|space-between`; the default remains `flex-start`.
`stretch`, `space-evenly`, `normal`, logical values, CSS-wide keywords,
safe/unsafe modifiers, and malformed values continue to produce the existing
bounded unsupported-value diagnostic and use the `flex-start` fallback.

For a block-level `display:flex` container that passes the existing eligible
direct-element gate and uses `flex-wrap:wrap`:

- line formation, line sizes, item sizing, per-line `align-items`, main-axis
  placement, and complete line artifact ownership remain unchanged;
- when explicit resolved content-box height has positive remainder after the
  formed line heights, `space-around` places each line at the center of one
  equal integer slot. The line-start offset for line `i` is
  `floor(remainder * (2*i + 1) / (2*line_count))`, with saturating bounded
  arithmetic; this gives half a slot before the first line, one slot between
  neighboring lines, and half a slot after the last line under deterministic
  integer rounding;
- the offset formula distributes only positive free space, never creates a
  negative line coordinate, and leaves auto-height and undersized content boxes
  stacked as before;
- `space-around` applies to the complete line artifact ranges after
  `align-items`. Direct item boxes, descendants, text runs, display-list
  entries, viewport projection, point hit testing, root overflow, and capture
  therefore consume the same shifted coordinates;
- a single formed line uses half of the available positive remainder before
  the line, while `nowrap` ignores `align-content` and remains coordinate- and
  paint-equivalent to 071;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order. The value changes visual geometry
  only.

The property remains bounded cross-line placement, not general Flexbox or
browser conformance.

## Explicit exclusions

This slice does not add `space-evenly`, `stretch`, `place-content`,
cross-axis gaps, `row-gap`, `column-gap`, flex growth/shrink/basis, auto
margins, column directions, `wrap-reverse`, logical direction/RTL, intrinsic
or percentage sizing, nested scrolling, positioned or stacking layout,
keyboard or accessibility reordering, or browser Flexbox parity. It does not
change non-flex normal flow, source/semantic traversal, or the stable two-crate
package topology.

## Tradeoffs and risks

The formula uses integer document pixels and floors each line's ideal offset.
That makes the result reproducible and bounded, but it cannot represent
fractional half-slots or subpixel Flexbox distribution. Saturating `u64`
intermediate arithmetic prevents a large free-space/line-index product from
wrapping before it is converted back to the engine's bounded `u32` geometry.
The default and all previously supported values retain their existing paths;
only the new parser value selects the additional offset formula.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `SpaceAround` computed value, cascade, inline declaration, and diagnostics;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: add the checked
  integer line-offset formula to the existing complete-artifact second pass;
- `crates/glass-browser/tests/native_engine.rs`: parser/cascade rejection and
  acceptance, single- and multi-line explicit-height geometry, auto/smaller
  heights, `nowrap` equivalence, descendants, paint, hit testing, overflow,
  and semantic-order coverage;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [x] CSS unit tests cover accepted `space-around`, defaulting, rejected
  values, non-inheritance, selector cascade, and inline precedence;
- [x] integration tests cover the saturating offset formula for one and multiple
  lines, explicit/auto/smaller heights, `nowrap` equivalence, descendants,
  root overflow, hit testing, paint, and semantic/source-order fallback;
- [x] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, and documentation validators pass;
- [x] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [x] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

The design checkpoint is `c8e5170` and the implementation checkpoint is
`a1c8b56`. Local verification completed on 2026-09-01 UTC:

- `RUST_MIN_STACK=4194304 cargo test -p glass-browser --features native-engine --test native_engine native_flex_align_content --locked -- --nocapture`: 2 passed (6m00s compile; 0.20s test runtime);
- `RUST_MIN_STACK=4194304 cargo test -p glass-browser --features native-engine --test native_engine --locked -- --nocapture`: 94 passed (2.20s warm run);
- `RUST_MIN_STACK=4194304 cargo test -p glass-browser --features native-engine --lib --locked`: 869 passed, 1 ignored (6m58s compile; 4.50s test runtime);
- `RUST_MIN_STACK=4194304 cargo clippy -p glass-browser --all-targets --all-features --locked -- -D warnings`: passed in 4m57s;
- `RUST_MIN_STACK=4194304 cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings`: passed in 4m52s;
- `cargo fmt --all -- --check` and `git diff --check`: passed;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`: passed in 6m57s;
- `cargo build -p glass-dev --locked`: passed in 11m24s;
- repository validators passed: version sync, feature parity, release documentation
  (486 Markdown documents; 83 current documents; 0 current-claim failures), TUI
  shortcuts, documentation depth, documentation coverage (486 Markdown files,
  345 full-product MCP tools, 17 examples, 22 public modules), reliability
  matrix, public read-only adapters, and the web-IR corpus (8 fixtures, 8
  scenarios, 11 categories).

The exact regenerable Glass Cargo target was reclaimed after validation from
4.1 GB to 4.0 KB. The separate ForgeBuild target remains at 4.0 KB. No
long-lived Glass process was terminated. The branch remains local-only until a
separately authorized push, so remote CI and release status are not inferred.
