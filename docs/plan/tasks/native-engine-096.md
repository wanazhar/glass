---
id: native-engine-096
scope: glass-browser/native-engine/flex-direction-column-wrap-reverse
status: complete
depends-on: [native-engine-095]
---

# Native bounded column wrap-reverse

## Objective

Extend the bounded column Flexbox owner from `flex-wrap:wrap` to
`flex-wrap:wrap-reverse` for eligible fixed-height `column` and
`column-reverse` containers. Reuse 095 line formation, per-line sizing,
justification, cross-line distribution, and shared artifact ownership while
reflecting the horizontal cross axis at the physical cross-end.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex-wrap](https://www.w3.org/TR/css-flexbox-1/#flex-wrap)
- [CSS Flexible Box Layout Module Level 1: multi-line flex containers](https://www.w3.org/TR/css-flexbox-1/#multi-line-flex-container)
- [CSS Flexible Box Layout Module Level 1: align-content](https://www.w3.org/TR/css-flexbox-1/#align-content-property)
- [CSS Flexible Box Layout Module Level 1: flex-direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The existing computed `flex-direction:column|column-reverse` and
`flex-flow:column wrap-reverse` values become layout-capable for one bounded
context:

- the container is `display:flex`, has a finite explicit content height and a
  finite available content width, and uses `flex-wrap:wrap-reverse`;
- every visible direct element child has a bounded explicit height or a
  bounded pixel `flex-basis`; direct text, hidden/non-rendered children, and
  unsupported node shapes retain the established fallback boundary;
- visual items are sorted by `(order, source_index)` before line formation;
  DOM, semantic, keyboard, and source order remain unchanged;
- line formation is identical to 095: order-sorted items form vertical lines
  against the fixed content height, with `row-gap` inside a line and an item
  that cannot fit an empty line still retained;
- each line keeps the 095 fixed content height, per-line integer flex
  grow/shrink/basis, `justify-content`, and complete descendant/artifact
  ownership. `column` places the main-axis items top-to-bottom and
  `column-reverse` places them bottom-to-top;
- the formed line sequence is reflected across the horizontal content box:
  the first source-order line is placed from the physical cross-end and later
  lines proceed toward the physical cross-start. `column-gap` remains the
  bounded gap between adjacent horizontal line boxes;
- every existing `align-content` value supported by 095
  (`flex-start|center|flex-end|space-between|space-around|space-evenly|
  stretch|normal`) is reflected with the line boxes, including its leading,
  inter-line, trailing, and deterministic integer remainder behavior;
- `align-items` and `align-self` cross-axis placement is reflected with the
  line: `flex-start`/stretch/normal use the line's cross-start, `flex-end`
  uses its cross-end, and center remains centered. Explicit item widths stay
  authoritative; auto-width stretch uses the resolved line width;
- line and item coordinates are finalized before layout artifacts are emitted.
  Complete descendant ranges and shared document-space geometry feed display
  lists, software rasterization, viewport projection, root overflow, scrolling,
  point hit testing, capture, and semantic/source-order consumers;
- unsupported or ineligible column layouts retain the established normal-flow
  fallback. `flex-wrap:wrap` continues to use native-engine-095.

## Tradeoffs

- Reflecting the completed 095 line geometry at the cross-axis owner keeps
  `align-content`, gaps, and line widths consistent without a second artifact
  translation pass. It makes the cross-axis reversal explicit in the layout
  contract rather than treating it as a row-only behavior.
- Reversing only physical line placement and cross-axis item alignment preserves
  source and semantic order while matching the intended visual meaning of
  `wrap-reverse`; `column-reverse` remains an independent main-axis concern.
- The slice continues to use bounded integer line widths and fixed content
  dimensions. It does not import intrinsic sizing, logical writing modes, or
  an unbounded CSS alignment model just to cover a wider browser surface.
- Keeping 095's eligible-child gate avoids mixing unsupported anonymous text,
  auto-height columns, and intrinsic sizing into the cross-axis reflection;
  those cases remain diagnosable through the established fallback behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation and documentation closeout are complete. The design checkpoint
is `f5026f3c` and the implementation checkpoint is `6862aff6`.

Passed locally:

- `cargo fmt --all -- --check` and `git diff --check`;
- focused column `wrap-reverse` integration: 4/4 passed;
- full `cargo test -p glass-browser --features native-engine --test
  native_engine`: 125/125 passed;
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --features
  native-engine --lib --no-fail-fast`: 887 passed, 1 ignored, 0 failed;
- lockfile-pinned all-feature workspace Clippy passed for both crates with
  warnings denied; no-default-feature `glass-browser` Clippy also passed;
- warning-denied locked workspace rustdoc passed;
- locked `glass-dev --bins` compilation passed;
- locked `glass-browser` and patched locked `glass-dev` package assembly
  passed, and the packaged dependency check confirmed exact `glass-browser`
  0.3.14 resolution;
- locked offline fuzz-target compilation passed;
- version sync, feature parity, release documentation, TUI, documentation
  depth, live coverage, reliability, public-adapter, and Web IR validators
  passed. The pre-closeout release-doc audit reported 510 Markdown documents,
  83 current documents, 57 previous-version hits, 570 semantic hits, and 0
  current-claim failures; live coverage reported 510/345/17/22 and Web IR
  reported 8/8/11;
- the reflected line boxes, `align-content`, cross-axis item alignment,
  `column`/`column-reverse` main placement, descendant artifacts, display-list
  paint, raster output, hit testing, and explicit ineligible fallback are
  covered by the native integration tests.

The default 2 MiB Rust test-thread stack still overflows in the pre-existing
`cli::args::tests::agent_readiness_commands_are_explicit` test. It reproduces
with and without `native-engine`; the feature library passes with the explicit
32 MiB stack. No native-engine path is exercised by that parser test.

Cleanup removed the exact regenerable `/tmp/glass-096-target` tree and
`/tmp/glass-096-release-documentation.json` only after process/open-file
checks. No `/tmp/glass-*-target` directories remain, the repository
`target/` remains 4.0K, `fuzz/target` is absent, and the final filesystem check
reports 65G available at 67% use. Shared registries/toolchains, source,
durable data, and long-lived Glass processes were retained.

Remote CI remains pending while `main` is local-only. No push, tag, release,
registry publication, or browser-parity certification is part of this slice.
