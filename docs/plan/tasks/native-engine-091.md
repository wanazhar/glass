---
id: native-engine-091
scope: glass-browser/native-engine/justify-content-space-evenly
status: complete
depends-on: [native-engine-090]
---

# Native bounded justify-content space-evenly

## Objective

Extend the bounded non-inherited `justify-content` grammar with the explicit
`space-evenly` value for eligible fixed-width flex lines. Distribute positive
main-axis free space into equal integer slots around the existing ordered item
placements, while preserving explicit gaps, margins, flex sizing,
row-reverse mirroring, wrapping, and all shared downstream artifacts.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: space-evenly distribution](https://drafts.csswg.org/css-align/)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited
`justify-content:space-evenly` value alongside the completed
`flex-start`, `center`, `flex-end`, `space-between`, and `space-around`
values. Parsing remains case-insensitive and accepts one token only. The
computed value remains distinct; the omitted/default value remains
`flex-start`. Because the existing `place-content` parser delegates shared
values to both axis parsers, a one-token `place-content:space-evenly`
declaration becomes valid through the same bounded component expansion; no
separate geometry path is introduced.

For each eligible fixed-width row or row-reverse flex line, after the
existing item visibility, source/order sorting, explicit gap, flex grow/shrink,
bounded width, and margin calculations have formed the item list:

- only positive bounded main-axis free space is distributed;
- for item index `i` in a line containing `n` items, its cumulative extra
  leading offset is the deterministic integer
  `floor(free_space * (i + 1) / (n + 1))`;
- the first item receives one slot before it, each adjacent item receives the
  difference between successive cumulative offsets in addition to the
  explicit gap, and the trailing remainder is retained at the line end;
- the same offsets are mirrored for row-reverse without changing source/order
  identity or margin ownership;
- a one-item line receives the centered half-space offset, while zero-item
  lines remain empty;
- if the line has no positive free space after flex sizing and explicit gaps,
  the existing bounded zero-distribution/flex-start behavior remains in force.

The distribution applies independently to each already-formed wrapped line.
Explicit `gap`, `row-gap`, item margins, flex growth/shrink/basis, and
`justify-content`'s existing line-width owner remain authoritative; the new
value does not resize items or line boxes. `align-items`, `align-self`,
`align-content`, `place-content`'s cross-axis component, and the native
omitted-value defaults do not change except for the existing shared one-token
`place-content` parser now accepting this value.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid `space-evenly`
declaration wins at its existing cascade position; an invalid later
declaration does not erase an earlier valid winner. Unsupported CSS-wide,
logical, safe/unsafe, percentage, fractional, auto-margin, and other
unsupported forms remain typed diagnostics or out of scope.

Outside the bounded fixed-width row/row-reverse flex line, this slice does not
claim grid, block, absolute-positioned, column-direction, writing-mode,
negative-free-space safe fallback, intrinsic-sizing, or browser-wide
conformance semantics. The resulting box and complete descendant artifact
range must remain consistent across layout, display-list paint,
software rasterization, viewport projection, hit testing, scrolling, capture,
and semantic/source order.

## Tradeoffs

- The cumulative-offset formula reuses the existing bounded integer alignment
  style and gives every remainder a deterministic position, at the cost of
  floor-rounded rather than fractional CSS metrics.
- Applying offsets after flex grow/shrink and explicit gaps keeps one main-axis
  geometry owner and preserves current sizing behavior, but it intentionally
  does not model auto-margin absorption or negative free-space fallback.
- Row-reverse mirrors the resolved offsets around the same line container,
  preserving physical reachability and source/order identity without a second
  placement algorithm.
- Accepting the shared one-token `place-content:space-evenly` form is a
  deliberate parser consequence and keeps the shorthand components
  consistent; the cross-axis component continues to use its existing
  `align-content` owner.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation checkpoint is `118590f7`; the documentation closeout is
the follow-up checkpoint that records this evidence. Focused parser/cascade
coverage passed 2/2 in 13m27.54s with 2,505,236 KiB peak RSS. Focused shared
layout coverage passed 1/1 in 5m53.45s with 2,108,284 KiB peak RSS. The full
native integration suite passed 118/118 in 2.96s with 81,892 KiB peak RSS;
the complete feature-enabled library suite passed 887 with 1 ignored and 0
failures in 6.20s with 81,676 KiB peak RSS.

Strict all-feature Clippy passed with warnings denied in 13m25.67s with
1,888,112 KiB peak RSS. Strict no-default-feature Clippy passed with warnings
denied in 6m56.40s with 1,792,464 KiB peak RSS. Workspace rustdoc passed with
`RUSTDOCFLAGS="-D warnings"` in 3m11.04s with 1,634,344 KiB peak RSS. The
locked `glass-dev --bins` build passed in 11m08.68s with 1,996,664 KiB peak
RSS. The exact generated AArch64 ELF binaries were 140,088,688 bytes for
`glass` and 90,867,448 bytes for `glass-browser`.

Static validators passed at version `0.3.14`: feature parity 14 capabilities
across 4 targets; release documentation 505 Markdown documents, 83 current
documents, 57 previous-version hits, 566 semantic audit hits, and 0 current
claim failures; TUI 15 implementation keys and 63 documentation markers;
depth 93 guides and 19 contracts; live documentation coverage 505 Markdown
files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
public modules; reliability 6 scenarios across 4 targets; 5 public read-only
adapters; and Web IR 8 fixtures, 8 scenarios, and 11 categories. Formatting
and `git diff --check` passed.

The isolated `/tmp/glass-091-target` reached 5.3G. After all commands exited,
an exact no-process/no-open-file check passed and the tree was removed with a
bounded same-filesystem delete. Only the 4.0K project `target/` placeholder
remains; `/dev/sda1` is 129G used, 65G available, and 67% full. The separate
regenerable `fuzz/target` tree remains absent. Shared registries, toolchains,
source, and long-lived Glass processes were retained. Remote CI remains
pending because the branch is local-only.
