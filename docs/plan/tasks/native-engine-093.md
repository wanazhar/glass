---
id: native-engine-093
scope: glass-browser/native-engine/justify-content-stretch
status: complete
depends-on: [native-engine-092]
---

# Native bounded justify-content stretch

## Objective

Accept explicit non-inherited `justify-content:stretch` in the bounded native
flex-row grammar while retaining a distinct computed keyword and routing its
used geometry through the already-proven `flex-start` owner. This closes the
remaining standard content-distribution alias after the completed explicit
`normal` boundary without changing omitted-value behavior or adding another
layout owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex-container stretch fallback](https://drafts.csswg.org/css-align/#content-distribution-flex)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts explicit non-inherited
`justify-content:stretch` alongside the completed bounded positional,
distributed, and `normal` values. Parsing remains case-insensitive and
accepts one token only. The computed value remains a distinct `Stretch`
keyword. Omitted `justify-content` continues to use Glass native's
established `FlexStart` fallback and does not start inheriting.

For an eligible fixed-width `row` or `row-reverse` flex line, after existing
visibility, source/order sorting, explicit gap, flex grow/shrink, bounded
width, and margin calculations have formed the item list, explicit `stretch`
uses the same bounded used placement as `flex-start`:

- no positive main-axis free-space offset is inserted before or between items;
- explicit gaps and item margins remain authoritative;
- row-reverse retains its existing physical reverse walk and mirrors no new
  distribution, without changing source/order identity;
- wrapped lines remain independently formed and use the same per-line path;
- empty and overflowing lines retain the established bounded behavior.

The existing shared one-token `place-content` parser consequently accepts
`place-content:stretch`, producing the distinct
`AlignContentValue::Stretch` cross-axis component and
`JustifyContentValue::Stretch` main-axis component. A two-token form such as
`place-content:center stretch` is valid through the same axis parser
composition; the cross-axis component continues to use its existing owner.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid explicit `stretch`
declaration wins at its existing cascade position; an invalid later
declaration does not erase an earlier valid winner. Unsupported CSS-wide,
logical, safe/unsafe, percentage, fractional, auto-margin, and other
unsupported forms remain typed diagnostics or out of scope.

Outside the bounded fixed-width row/row-reverse flex line, this slice does not
claim block, grid, absolute-positioned, column-direction, writing-mode,
negative-free-space, intrinsic-sizing, or browser-wide conformance semantics.
The resulting box and complete descendant artifact range must remain
consistent across layout, display-list paint, software rasterization,
viewport projection, hit testing, scrolling, capture, and semantic/source
order.

## Tradeoffs

- Keeping `Stretch` distinct in computed style preserves specified-value
  observability while using the existing `FlexStart` used geometry, avoiding a
  second main-axis algorithm.
- Mapping `stretch` to flex-start only in the supported flex context captures
  the standards-defined flex fallback without pretending that the bounded
  native backend implements every layout mode. Other layout-mode semantics
  remain explicitly outside the contract.
- Reusing the reverse walk, margins, gaps, sizing, wrapping, and artifact
  consumers keeps the change small and deterministic, at the cost of not
  modeling the complete CSS alignment matrix.
- Expanding the shared one-token and two-token `place-content` forms keeps
  shorthand components consistent, but makes previously unsupported
  `place-content:stretch` declarations valid and therefore requires explicit
  cascade-regression coverage.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation checkpoint: `653f025e` (`feat(native-engine): support
justify-content stretch`). Documentation closeout follows the complete local
certification and cleanup record below.

- focused CSS parser/cascade: 73/73 passed in 13m16.38s; peak RSS
  2,467,176 KiB;
- focused shared-layout integration:
  `native_flex_row_justify_content_aliases_reuse_flex_start_geometry_and_artifacts`
  passed 1/1 in 5m48.13s; peak RSS 2,105,432 KiB;
- full native integration: 119/119 passed in 2.90s; peak RSS 82,236 KiB;
- feature-enabled `glass-browser` library: 887 passed, 1 ignored, 0 failed
  in 8.15s with the established `RUST_MIN_STACK=8388608`; peak RSS
  82,164 KiB;
- strict all-feature Clippy: warnings denied, passed in 8m48.23s; peak RSS
  1,886,268 KiB;
- strict no-default-feature Clippy: warnings denied, passed in 5m04.05s;
  peak RSS 1,797,240 KiB;
- warning-denied workspace rustdoc: passed in 6m38.76s; peak RSS
  1,634,400 KiB;
- locked `glass-dev --bins` build: passed in 10m44.80s; peak RSS
  1,998,384 KiB. The generated AArch64 ELF debug binaries were
  `glass` (140,088,688 bytes) and `glass-browser` (90,867,448 bytes);
- repository audits passed at version `0.3.14`: version sync; feature parity
  14 capabilities across 4 targets; release documentation 507 Markdown
  files, 83 current documents, 57 previous-version hits, 566 semantic audit
  hits, and 0 current-claim failures; TUI 15/63; documentation depth 93/19;
  live documentation coverage 507/345/17/22; reliability 6/4; public
  read-only adapters 5; Web IR 8/8/11; formatting and diff checks;
- the exact isolated `/tmp/glass-093-target` tree reached 5.0G during the
  gate set and was removed only after the build/coverage processes exited and
  `lsof` reported no open files beneath it. The generated release report
  `/tmp/glass-093-release-documentation.json` was also removed. The project
  `target/` remains 4.0K and `fuzz/target` remains absent. Shared registries,
  toolchains, source, and long-lived Glass processes were retained;
- remote CI remains pending because `main` is local-only and has not been
  pushed. No release, tag, registry publication, or browser-parity
  certification is claimed.
