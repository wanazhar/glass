---
id: native-engine-144
scope: glass-browser/native-engine/cascade-layers-local-box-model-revert-layer
status: complete
depends-on: [native-engine-143]
---

# Native bounded local box-model `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-143 to
add standalone, case-insensitive `revert-layer` to the existing local
`box-sizing`, physical `padding` edges, and physical `margin` edges. The nine
physical edge owners and the box-sizing owner must resolve independently while
preserving the existing box-model, normal-flow, flex auto-margin, overflow,
capture, hit-test, display-list, raster, and semantic/source-order contracts.

## Context

The native engine already accepts finite non-negative integer-pixel physical
padding and margin values, `margin:auto`, and `box-sizing:content-box|border-box`.
Their stylesheet and inline paths currently keep one winning concrete candidate
per edge or property, so a higher-priority rollback cannot expose a lower
candidate or the absent local fallback. This slice adds only private
declaration/candidate state and reuses the bounded 15-layer registry and local
resolver; it does not add logical edges, percentages, negative values, or
margin-collapsing behavior.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/CSS2/box.html>
- <https://www.w3.org/TR/css-sizing-3/#box-sizing>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-143.md`

## Contract

### Declaration and cascade state

- `box-sizing` accepts one standalone, case-insensitive `revert-layer` token in
  addition to `content-box|border-box`. The rollback representation is
  private; the public computed value remains `NativeBoxSizing` and the local
  fallback remains `content-box`.
- `padding`, each physical `padding-top|right|bottom|left` edge, `margin`, and
  each physical `margin-top|right|bottom|left` edge accept standalone,
  case-insensitive `revert-layer` in addition to their existing bounded
  grammar. A shorthand rollback supplies the same rollback candidate to its
  four physical edge owners; a longhand remains edge-local.
- Padding values remain finite non-negative integer pixels. Margin values remain
  finite non-negative integer pixels or `auto`. The public computed edge values
  remain zero-based `NativeBoxEdges`; auto-margin provenance remains a private
  `NativeAutoEdges` view.
- Each physical edge and `box-sizing` gets an independent bounded candidate
  sequence. Rollback blocks only the current bounded layer for that owner and
  resolves through its lower concrete candidate; if no concrete edge remains,
  padding and margin resolve to zero and box sizing resolves to `content-box`.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, same-block valid
  shorthand/longhand order, and invalid-later preservation remain explicit
  behavior.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- Resolved padding and margins continue to derive outer/content rectangles,
  content-origin child/text placement, normal-flow offsets, and the existing
  box-sizing conversion.
- Resolved `margin:auto` edges continue to feed the existing eligible flex-row
  and fixed-height flex-column auto-space distribution; no new flex owner or
  auto-margin semantics are introduced.
- The values continue through the existing overflow, root scrolling, capture,
  display-list, raster, point-hit-test, and semantic/source-order consumers
  without a new geometry or artifact schema.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not add
logical or percentage edges, negative values, margin collapsing, positioned or
replaced-element sizing, multiple origins, animation, script, grid, or
browser-wide box-model conformance.

## Tradeoffs

- One generic local declaration/resolver path avoids nine property-specific
  rollback loops, while one candidate sequence per physical edge preserves
  independent shorthand/longhand and absent-fallback behavior.
- Grouping `box-sizing` with the eight padding/margin longhands reduces design,
  build, and static-audit overhead, but the focused regression must cover both
  content-box/border-box conversion and flex auto-margin placement so a
  cascade-only assertion cannot hide a geometry regression.
- Keeping public edge and box-sizing representations unchanged preserves
  downstream build stability at the cost of intentionally excluding broader
  CSS-wide keyword and box-model grammar.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Implementation

- Design checkpoint: `255a1ac8090250e7a73b46f41c83223dc7b56109`.
- Source checkpoint: `7ce9c52cd33b3270f6412002d31643f3250d51a6`.
- The source checkpoint updates `css.rs` and `native_engine.rs` only; the
  public computed-style and artifact schemas, feature defaults, dependencies,
  and two-crate boundary remain unchanged.
- Product and authoritative planning documentation is synchronized in the
  closeout checkpoint for this task.

## Verification

The completed gate covered:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, percentage, negative, and intrinsic forms for
  `box-sizing`, every padding edge, and every margin edge;
- independent named-layer priority, repeated rollback, unlayered/inline
  precedence, shorthand/longhand order, absent fallback, auto-margin
  provenance, and invalid-later preservation for every owner;
- content-box/border-box outer/content geometry, physical edge origins,
  normal-flow margin offsets, flex auto-margin distribution, overflow/capture,
  display-list/raster output, point hit testing, and unchanged semantic/source
  order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed because this
  checkout is local-only.

## Evidence

- Focused locked `glass-browser` check passed.
- Local box-model parser/cascade tests passed 2/2.
- Targeted native integration tests passed 2/2 with 180 filtered.
- Full native integration passed 182/182.
- Feature-enabled `glass-browser` library tests passed 953, with 1 ignored.
- Strict affected-package Clippy passed with warnings denied.
- The required default-target inventory binaries passed locked package checks
  and builds; documentation coverage then passed with 345 full-product MCP
  tools (100 browser-only), 17 examples, and 22 public modules.
- Static truth gates passed: version sync at 0.3.14; release documentation with
  558 Markdown files, 83 current-document records, 57 previous-version hits,
  653 semantic-audit hits, and zero current-claim failures; documentation depth
  with 93 current guides and 19 substantive contracts; feature parity for 14
  capabilities across 4 targets (baseline 0.3.0, next 0.3.14, checkout 0.3.14);
  TUI shortcut parity with 15 implementation help keys and 63 documentation
  markers; 5 public read-only adapters; reliability with 6 scenarios across 4
  targets; and Web IR with 8 fixtures, 8 scenarios, and 11 categories.
- Formatting and diff checks passed.
- After all checks completed, no Cargo/Rust process or open handle referenced
  the exact regenerable targets. `/home/ubuntu/work/glass/target` (3,571,359,744
  bytes, 5,751 files, 882 directories) and `/tmp/glass-144-focused`
  (3,316,219,904 bytes) were removed with bounded same-filesystem deletion;
  both exact paths are absent. `/tmp` availability increased from
  77,005,815,808 to 83,893,362,688 bytes, reclaiming 6,887,546,880 bytes.
