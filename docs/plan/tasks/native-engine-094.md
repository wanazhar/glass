---
id: native-engine-094
scope: glass-browser/native-engine/flex-direction-column
status: complete
depends-on: [native-engine-093]
---

# Native bounded flex-direction column

## Objective

Extend the native Flexbox boundary from horizontal rows to a bounded vertical
`column`/`column-reverse` main axis without creating a second document or
geometry owner. Reuse the existing item sorting, gap, flexible-length,
justification, cross-axis alignment, descendant, paint, hit-test, scroll, and
capture contracts where their axis is explicitly mapped.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Flexible Box Layout Module Level 1: flex flow direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction)
- [CSS Flexible Box Layout Module Level 1: axis mappings](https://www.w3.org/TR/css-flexbox-1/#axis-mapping)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar and cascade accept `flex-direction:column` and
`flex-direction:column-reverse` as distinct non-inherited computed keywords.
The existing `flex-flow` parser accepts the same direction values alongside
the already bounded wrap values. Omitted direction remains `row`.

Layout uses the new direction only for an eligible bounded vertical flex
container:

- the container is `display:flex`, has only direct eligible element children,
  has a finite explicit content height, and uses `flex-wrap:nowrap`;
- each visible child has an explicit bounded height or a bounded pixel
  `flex-basis`; its width remains the existing bounded fixed/intrinsic cross
  size, and physical margins remain authoritative;
- `row-gap` is the vertical main-axis gap between items; the existing
  `column-gap` field is not consumed for a single no-wrap column line;
- existing visual `(order, source_index)` sorting occurs before placement while
  DOM and semantic/source order remain unchanged;
- `justify-content:normal|flex-start|center|flex-end|space-between|
  space-around|space-evenly|stretch` distributes the formed items along the
  vertical main axis using the existing bounded integer policies. Explicit
  `stretch` and `normal` retain their distinct computed values and use the
  existing flex-start fallback owner;
- `column` walks from physical top to bottom and `column-reverse` walks from
  physical bottom to top. Reverse placement preserves margins, gaps, item
  identity, complete descendant ranges, non-negative bounded coordinates, and
  the existing overflow/scroll behavior;
- `align-items` and per-item `align-self` reuse the existing bounded
  `flex-start|center|flex-end|stretch|normal` cross-axis semantics. Explicit
  child widths remain authoritative; an auto-width child may use the existing
  stretch width path within the finite container content width;
- existing integer `flex-grow`, base-width-weighted `flex-shrink`, and
  `flex-basis:auto|Npx` policies are applied on the vertical main axis after
  line formation and before `justify-content`. Flexible resizing changes the
  item’s outer height through the existing box owner while preserving
  descendant artifact ranges;
- the final box coordinates and complete descendant artifact ranges feed the
  existing layout, display-list, software rasterization, viewport projection,
  hit testing, scrolling, capture, and semantic/source-order consumers.

`flex-wrap:wrap` and `wrap-reverse`, an auto-height column container, multiple
vertical lines, cross-axis line packing, column-gap distribution, auto
margins, percentage/fractional/intrinsic main sizes, logical direction or
writing modes, baseline alignment, grid/block/absolute layout, and
browser-wide Flexbox conformance remain outside this slice. Such contexts
retain the established normal-flow fallback or bounded unsupported behavior;
the parser accepting a direction does not claim those layout modes.

Cascade specificity, source order, inline precedence, non-inheritance,
failure-atomic diagnostics, and the two-crate/default-off native-engine
boundary remain unchanged. No network, JavaScript, storage, new dependency,
third crate, runtime, or artifact pipeline is introduced.

## Tradeoffs

- Mapping the vertical axis into the existing flex item and artifact owners
  advances real column layouts without duplicating document state or paint
  consumers, at the cost of deliberately excluding multi-line columns until
  a separate cross-axis line contract exists.
- Requiring explicit container height makes free-space and reverse placement
  deterministic and prevents an incomplete intrinsic-sizing algorithm from
  masquerading as browser behavior; auto-height columns retain the established
  fallback.
- Reusing the current integer grow/shrink policies keeps row and column
  rounding consistent, while the bounded fixed-pixel scope does not model
  percentage, fractional, auto-margin, or font-metric contributions.
- Keeping the computed `column` keywords distinct preserves CSS provenance even
  when unsupported combinations fall back, and requires parser, cascade,
  `flex-flow`, diagnostics, layout, and complete-artifact regression coverage.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The certification gate will include focused parser/cascade tests, focused
column/column-reverse layout and artifact tests, the full native integration
and feature-enabled library suites, strict all-feature and no-default-feature
Clippy, warning-denied workspace rustdoc, locked `glass-dev` binary
compilation, static documentation/release validators, live documentation
coverage with explicit temporary binaries, and exact isolated-target cleanup.
Remote CI remains pending because `main` is local-only and has not been
pushed.

## Completion evidence

Design checkpoint: `aea47b17`.

Implementation checkpoints: `7dc92517` added the parser, cascade, vertical
flex sizing/placement owner, and integration coverage; `4e212151` grouped the
forced outer dimensions into a bounded value so the strict lint contract stays
clean.

Focused and full behavior evidence:

- focused CSS parser/cascade: 73/73 passed in 14m13.20s; peak RSS
  2,521,608 KiB;
- focused column justification/cross-axis/descendant/artifact integration:
  1/1 passed in 6m13.74s; peak RSS 2,105,384 KiB;
- focused vertical grow/shrink integration: 1/1 passed in 1.33s; peak RSS
  82,500 KiB;
- full native integration: 121/121 passed in 3.95s; peak RSS 82,496 KiB;
- feature-enabled `glass-browser` library: 887 passed, 1 ignored, 0 failed
  in 6.21s under `RUST_MIN_STACK=8388608`; peak RSS 82,232 KiB.

Repository certification evidence:

- strict all-feature Clippy passed with warnings denied in 5m29.37s; peak RSS
  1,886,036 KiB;
- strict no-default-feature Clippy passed with warnings denied in 4m52.17s;
  peak RSS 1,805,448 KiB;
- warning-denied workspace rustdoc passed in 7m18.43s; peak RSS
  1,638,396 KiB;
- locked `glass-dev --bins` build passed in 11m07.77s; peak RSS
  1,999,576 KiB;
- generated binaries are AArch64 ELF debug outputs: `glass` 140,088,688
  bytes and `glass-browser` 90,867,448 bytes;
- version sync, feature parity, release documentation, TUI, documentation
  depth/coverage, reliability, adapter, Web IR, formatting, and diff audits
  passed. The release-doc audit reported 508 Markdown documents, 83 current
documents, 57 previous-version hits, 568 semantic hits, and 0 current-claim
  failures; coverage reported 508/345/17/22 and Web IR reported 8/8/11.

The first all-feature Clippy attempt correctly blocked on the new helper's
eight arguments; `4e212151` repaired that finding and the rerun passed. This
is recorded so the implementation checkpoint reflects the actual gate path,
not only the final green result.

Cleanup evidence: the exact regenerable `/tmp/glass-094-target` tree removed
5.0G after process and open-file checks; its temporary release-documentation
report was removed; no `/tmp/glass-*-target` directories remain;
`/home/ubuntu/work/glass/target` remains 4.0K and `fuzz/target` is absent.
The final filesystem check reported 65G available at 67% use. Shared
registries/toolchains, source, durable data, and long-lived Glass processes
were retained. Remote CI remains pending because `main` is local-only; no
push, tag, release, registry publication, or browser-parity certification is
claimed.
