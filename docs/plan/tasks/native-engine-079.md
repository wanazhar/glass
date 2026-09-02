---
id: native-engine-079
scope: glass-browser/native-engine/flex-grow
status: complete
depends-on: [native-engine-078]
---

# Native bounded flex-grow allocation

## Objective

Extend the existing bounded row-flex geometry owner with deterministic positive
free-space allocation for eligible flex items. The slice makes common fixed
width rows adapt their item widths without introducing a second layout engine,
fractional metrics, or a new workspace crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-grow` as a non-negative decimal integer
from `0` through `1024`. The property is non-inherited, defaults to `0`, and
uses the existing selector specificity, source order, and inline precedence.
Invalid, negative, fractional, unit-bearing, CSS-wide, and out-of-range values
remain unsupported and are reported through the existing bounded diagnostic
path without replacing a valid value.

For an eligible block-level `display:flex` row or `row-reverse` container,
the existing computed outer width after padding, borders, min/max constraints,
and margins is each item's flex base width. Wrapping and line formation still
use those base widths and the resolved column gap. After a line is formed, if
its base items and gaps leave positive free space and at least one item has a
positive grow factor, that free space is allocated by grow-weighted integer
shares. The prefix-floor remainder policy assigns every pixel deterministically
in visual/source order; arithmetic uses bounded `u64` intermediates.

Existing bounded `max-width` constraints cap grown outer widths. A capped item
is frozen and any unallocated remainder is redistributed among the remaining
positive grow factors until no eligible item can accept more space. Existing
minimum constraints remain part of each base width. If free space is zero or
negative, or all grow factors are zero, the prior fixed-width behavior remains;
there is no shrink pass.

Growth occurs before `justify-content`, so a line whose positive free space was
fully consumed by grow factors presents zero positive free space to the
existing justification owner. Gap, margins, row/cross-line alignment,
row-reverse placement, wrapping, overflow, paint, raster, viewport projection,
hit testing, scrolling, capture, and semantic/source order all consume the
same final item widths and coordinates. A grown width is passed into child
layout as the item's outer width so descendants and content rectangles observe
the same geometry.

This slice explicitly excludes `flex-shrink`, `flex-basis`, the `flex`
shorthand, fractional grow factors, auto margins, column directions,
percentage/intrinsic sizing changes, multiple independent flex formatting
contexts, and browser Flexbox conformance. It does not alter semantic order or
make native-engine selection implicit.

## Tradeoffs

- Integer grow factors and prefix-floor shares keep the current bounded
  coordinate model deterministic and cheap, but decimal CSS grow values remain
  visibly unsupported.
- Applying growth after line formation makes wrapped rows predictable and
  preserves the current line owner, but it does not reflow items based on
  grown widths; wrapping remains a base-size decision for this slice.
- Freezing max-constrained items and redistributing their remainder preserves
  the existing min/max contract, but adds a bounded iterative sizing pass.
- Forcing the final outer width into child layout keeps descendants, paint,
  hit testing, and overflow coherent, but it intentionally treats the current
  fixed-width box model as the flex base-size owner rather than implementing
  full CSS used-value resolution.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Local evidence captured on 2026-09-02 UTC:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused CSS coverage passed 2/2 tests, including bounded parsing, invalid
  fallback, specificity, declaration order, inline precedence, and
  non-inheritance;
- focused flex-grow integration coverage passed 2/2 tests in 19 seconds,
  including weighted growth before justification, descendant geometry, paint,
  hit testing, wrap-reverse, overflow, and max-width redistribution;
- the full native integration suite passed 103/103 tests in 3 seconds;
- the full native library suite passed 875 tests with 1 existing ignored test
  under `RUST_MIN_STACK=8388608`;
- strict `cargo clippy --all-targets --all-features --locked -- -D warnings`
  passed in 13m40s, and the no-default-feature variant passed in 7m02s;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`
  passed in 3m04s, and `cargo build -p glass-dev --bin glass --locked` passed
  in 11m07s;
- repository validators passed: version sync at 0.3.14; feature parity at 14
  capabilities across 4 targets; release documentation at 493 Markdown files
  with 0 current-claim failures; TUI at 15 implementation keys and 63
  documentation markers; depth at 93 guides and 19 contracts; coverage at 493
  Markdown files, 345 full-product MCP tools, 17 examples, and 22 public
  modules; reliability at 6 scenarios across 4 targets; 5 read-only adapters;
  and Web IR at 8 fixtures, 8 scenarios, and 11 categories;
- after validation, the exact regenerable Glass target paths were checked for
  active users and open files, then removed: `target` fell from 5.6 GB to 4.0
  KB and filesystem headroom increased from 60 GB (70%) to 66 GB (67%);
- implementation checkpoint `1a930a3` and this documentation checkpoint are
  committed locally before the next slice; remote CI remains unclaimed because
  the branch is local-only.
