---
id: native-engine-090
scope: glass-browser/native-engine/justify-content-space-around
status: complete
depends-on: [native-engine-089]
---

# Native bounded justify-content space-around

## Objective

Extend the bounded non-inherited `justify-content` grammar with the explicit
`space-around` value for eligible fixed-width flex lines. Distribute positive
main-axis free space around the existing ordered item placements with bounded
integer rounding, while preserving explicit gaps, margins, flex sizing,
row-reverse mirroring, wrapping, and all shared downstream artifacts.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: space-around distribution](https://drafts.csswg.org/css-align/)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited
`justify-content:space-around` value alongside the completed
`flex-start`, `center`, `flex-end`, and `space-between` values. Parsing remains
case-insensitive and accepts one token only. The computed value remains
distinct; the omitted/default value remains `flex-start`. Because the existing
`place-content` parser delegates shared values to both axis parsers, a
one-token `place-content:space-around` declaration becomes valid through the
same bounded component expansion; no separate geometry path is introduced.

For each eligible fixed-width row or row-reverse flex line, after the
existing item visibility, source/order sorting, explicit gap, flex grow/shrink,
bounded width, and margin calculations have formed the item list:

- only positive bounded main-axis free space is distributed;
- for item index `i` in a line containing `n` items, its cumulative extra
  leading offset is the deterministic integer
  `floor(free_space * (2*i + 1) / (2*n))`;
- the first item therefore receives a half-size leading slot, adjacent items
  receive the difference between successive cumulative offsets in addition to
  the explicit gap, and the trailing remainder is retained at the line end;
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
omitted-value defaults do not change except for the existing shared
one-token `place-content` parser now accepting this value.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid `space-around`
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
- Accepting the shared one-token `place-content:space-around` form is a
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

- Design checkpoint: `97c69d9d`.
- Implementation checkpoint: `d814784f`. Focused CSS parser/cascade
  coverage passed 2/2 in 13m43s with 2,502,236 KiB peak RSS. The focused
  shared-layout integration passed 1/1 in 6m08s with 2,100,188 KiB peak RSS.
  The fixture covers deterministic remainder rounding, explicit gap-aware
  three-item placement, row-reverse mirroring, nested descendants, display
  list paint, hit testing, wrapped-line independence, one-item centering, and
  the zero-free-space overflow fallback.
- The full native integration suite passed 117/117 in 3.76s with 82,664 KiB
  peak RSS. The complete feature-enabled library suite passed 887 tests with
  1 ignored and 0 failures in 8.04s with 82,752 KiB peak RSS.
- Strict all-feature Clippy passed with warnings denied in 13m38s with
  1,885,896 KiB peak RSS. Strict no-default-feature Clippy passed in 6m49s
  with 1,795,076 KiB peak RSS. Workspace rustdoc with
  `RUSTDOCFLAGS="-D warnings"` passed in 3m16s with 1,630,028 KiB peak RSS.
  The locked `glass-dev` build of both `glass` and `glass-browser` binaries
  passed in 11m23s with 1,995,944 KiB peak RSS.
- Formatting, `git diff --check`, version synchronization at `0.3.14`,
  feature parity (14 capabilities across 4 targets), release documentation
  (504 Markdown documents, 83 current documents, 57 previous-version hits,
  564 semantic audit hits, and 0 current-claim failures), TUI shortcut
  inventory (15 implementation keys and 63 documentation markers),
  documentation depth (93 current guides and 19 substantive contracts),
  reliability (6 scenarios across 4 targets), public read-only adapters (5),
  and Web IR (8 fixtures, 8 scenarios, and 11 categories with runtime
  goldens) all passed. Live documentation coverage passed with 504 Markdown
  files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
  public modules.
- The exact temporary `/tmp/glass-090-target` tree reached 5.3G during
  validation. After all build processes exited, it had no open files and was
  removed in full. The project `/home/ubuntu/work/glass/target` remains only
  its 4.0K root directory; `/dev/sda1` is 129G used, 65G available, and 67%
  full. The 652M `/home/ubuntu/work/glass/fuzz/target` tree was also removed
  as regenerable output after an exact no-process/no-open-file check. Shared
  Cargo registries/toolchains and long-lived Glass processes were retained.
- Issue [#40](https://github.com/wanazhar/glass/issues/40) is updated with
  the design, implementation, evidence, and cleanup state. Remote CI remains
  pending because the branch is local-only; no push, release, tag, registry
  publication, or browser-parity certification is claimed.
