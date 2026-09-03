---
id: native-engine-095
scope: glass-browser/native-engine/flex-direction-column-wrap
status: complete
depends-on: [native-engine-094]
---

# Native bounded column flex wrapping

## Objective

Extend the native Flexbox owner from bounded single-line columns to bounded
multi-line `column`/`column-reverse` containers using `flex-wrap:wrap`. Form
vertical main-axis lines from the existing order-sorted items, stack those
lines across the horizontal cross axis with `column-gap`, and reuse the
existing integer sizing, justification, alignment, descendant, paint,
overflow, scroll, hit-test, and capture owners.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex-wrap](https://www.w3.org/TR/css-flexbox-1/#flex-wrap)
- [CSS Flexible Box Layout Module Level 1: flex lines](https://www.w3.org/TR/css-flexbox-1/#flex-lines)
- [CSS Flexible Box Layout Module Level 1: axis mappings](https://www.w3.org/TR/css-flexbox-1/#axis-mapping)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The existing computed `flex-direction:column|column-reverse` and
`flex-flow:column wrap` values become layout-capable for one bounded context:

- the container is `display:flex`, has a finite explicit content height and a
  finite available content width, and uses `flex-wrap:wrap`;
- every visible direct element child has a bounded explicit height or a
  bounded pixel `flex-basis`; direct text, hidden/non-rendered children, and
  unsupported node shapes retain the established fallback boundary;
- visual items are sorted by `(order, source_index)` before line formation;
  DOM, semantic, keyboard, and source order remain unchanged;
- line formation walks the order-sorted items along the vertical main axis.
  Each line receives consecutive items until the next bounded outer height
  plus the existing `row-gap` would exceed the fixed content height; an item
  that cannot fit an empty line still forms that line so no item disappears;
- each formed line uses the fixed content height as its bounded main size.
  Existing integer flex-grow, base-height-weighted flex-shrink, min/max
  freezing, `flex-basis`, and `justify-content` policies run independently
  for each line before item placement;
- `column-gap` is the bounded horizontal gap between adjacent formed lines;
  `row-gap` remains the vertical main-axis gap inside each line. Existing
  `align-content:flex-start|center|flex-end|space-between|space-around|
  space-evenly|stretch|normal` policies distribute or stretch line widths
  using the same integer cumulative-offset rules already used for wrapped
  rows;
- `column` places each line from physical top to bottom and
  `column-reverse` places each line from physical bottom to top. The current
  bounded `align-items`/`align-self` values align item widths inside each
  line; explicit widths remain authoritative and auto-width stretch uses the
  resolved line width;
- line and item coordinates are finalized before layout artifacts are emitted.
  Complete descendant ranges and the shared document-space geometry feed
  display-list generation, software rasterization, viewport projection,
  root overflow, scrolling, point hit testing, capture, and semantic/source
  order consumers;
- `flex-flow:column wrap` routes through this owner, while the already parsed
  `column-reverse wrap` and `flex-wrap:wrap-reverse` combinations remain
  explicitly outside this slice and retain the established normal-flow
  fallback until a separate cross-axis reversal contract lands.

`flex-wrap:nowrap` continues to use native-engine-094. Auto-height columns,
wrap-reverse, percentage/fractional/intrinsic main sizes, automatic margins,
logical direction or writing modes, baseline alignment, nested scrolling,
grid/block/absolute layout, and browser-wide Flexbox conformance remain
outside this bounded contract. No new crate, dependency, runtime, network,
JavaScript, storage, or artifact pipeline is introduced.

## Tradeoffs

- Forming lines from bounded outer heights makes `column-gap`, reverse main
  placement, and per-line flex sizing deterministic without importing a full
  intrinsic-sizing engine; an item larger than the line remains visible and
  may contribute bounded overflow.
- Resolving line widths before emitting children avoids a second coordinate
  shift pass and keeps descendant artifacts attached to one final geometry
  owner, at the cost of limiting cross-size contributions to the existing
  fixed/intrinsic pixel measurement policy.
- Reusing the row line-distribution formulas keeps integer rounding and
  `align-content` semantics consistent across axes, while leaving
  `wrap-reverse` separate prevents an untested cross-axis reflection from
  being mistaken for ordinary wrapping.
- `column-gap` is consumed only between formed lines in this multi-line
  column context. It is not silently applied to single-line columns, and
  earlier row-gap/column-gap contracts remain valid for their own contexts.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation checkpoint: `96ea62a3` (design checkpoint: `598228a7`).

Passed locally:

- `cargo fmt --all` and `git diff --check`;
- focused `native_flex_column_wrap*` integration: 2/2 passed;
- full `cargo test -p glass-browser --features native-engine --test
  native_engine`: 123/123 passed;
- `cargo clippy -p glass-browser --features native-engine --all-targets --
  -D warnings`: passed;
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --features
  native-engine --lib --no-fail-fast`: 887 passed, 1 ignored, 0 failed;
- the `wrap-reverse` fallback comparison, order-preserving line formation,
  row/column gap mapping, per-line grow sizing, reverse placement, complete
  descendants, display-list paint, and hit testing are covered by the new
  integration tests.

The default 2 MiB Rust test-thread stack still overflows in the pre-existing
`cli::args::tests::agent_readiness_commands_are_explicit` test. It reproduces
with and without `native-engine`, and the same feature test passes with the
explicit 32 MiB stack above; this is recorded as a test-harness/environment
follow-up rather than attributed to this layout slice.

Repository-wide certification passed locally: lockfile-pinned all-feature and
no-default-feature Clippy, warning-denied workspace rustdoc, locked
`glass-dev` binary compilation, locked package assembly and dependency-boundary
checking, fuzz-target compilation, static documentation/release validators,
live documentation coverage, and the exact temporary-target cleanup recorded
below. Remote CI remains pending because `main` is local-only and has not been
pushed.

## Completion evidence

- design checkpoint: `598228a7`;
- implementation checkpoint: `96ea62a3`;
- focused column-wrap integration: 2/2 passed in 0.37s after incremental
  compilation;
- full `native_engine` integration: 123/123 passed in 2.71s;
- feature-enabled `glass-browser` library: 887 passed, 1 ignored, 0 failed in
  6.55s with `RUST_MIN_STACK=33554432`;
- strict package Clippy with warnings denied: `glass-browser` all-features
  passed in 5m09s, `glass-dev` all-features passed in 5m05s, and the
  no-default-feature `glass-browser` pass completed in 4m40s;
- warning-denied locked workspace rustdoc passed in 5m17s;
- locked `glass-dev --bins` compilation passed in 12m44s;
- locked `glass-browser` and patched locked `glass-dev` package assembly
  passed; the packaged dependency check confirmed `glass-dev` resolves
  `glass-browser` exactly at 0.3.14. Cargo reported only the pre-existing
  yanked `chacha20 0.10.1` lockfile warning;
- locked offline fuzz-target compilation passed in 8m25s;
- version sync, feature parity, release documentation, TUI, depth, reliability,
  adapter, and Web IR validators passed. The release-doc audit reported 509
  Markdown documents, 83 current documents, 57 previous-version hits, 569
  semantic hits, and 0 current-claim failures; live coverage reported
  509/345/17/22 and Web IR reported 8/8/11;
- the new tests cover vertical line formation, order, row/column gaps,
  per-line flex sizing, align-content cross-axis placement,
  column-reverse, descendants, paint, and hit testing, plus the explicit
  wrap-reverse fallback boundary.

The default 2 MiB Rust test-thread stack still overflows in the pre-existing
`cli::args::tests::agent_readiness_commands_are_explicit` test. The failure
reproduces with and without `native-engine`; running the feature library with
`RUST_MIN_STACK=33554432` passes all 887 active tests. No native-engine code
path is exercised by the failing parser test.

Cleanup removed the exact regenerable `/tmp/glass-095-target` tree and
`/tmp/glass-095-release-documentation.json` only after process/open-file
checks. The repository `target` remains untouched at 4.0K, `fuzz/target` is
absent, shared registries/toolchains and long-lived Glass processes were
retained, and remote CI remains pending because `main` is local-only. No push,
tag, release, registry publication, or browser-parity certification is
claimed.
