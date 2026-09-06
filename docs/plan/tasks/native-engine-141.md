---
id: native-engine-141
scope: glass-browser/native-engine/cascade-layers-inherited-vertical-align-revert-layer
status: planned
depends-on: [native-engine-140]
---

# Native bounded inherited `vertical-align` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-140 to
add standalone, case-insensitive `revert-layer` to the existing inherited
`vertical-align` declaration. The owner must preserve the fixed-cell inline
and inline-block line-item placement, text fragments, display-list, raster,
overflow, capture, hit-test, and semantic/source-order contracts.

## Context

The native engine already accepts the finite `baseline|top|middle|bottom`
`vertical-align` values and carries them through the inherited style walk to
the shared line flush owner. Its stylesheet and inline declarations currently
keep only one winning concrete candidate, so a higher-priority rollback cannot
expose a lower candidate or the inherited parent value. This slice adds only
private declaration/candidate state and reuses the bounded first-appearance
15-layer registry and generic inherited-text resolver.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/CSS2/visudet.html#propdef-vertical-align>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-140.md`

## Contract

### Declaration and cascade state

- `vertical-align` accepts one standalone, case-insensitive `revert-layer`
  token in addition to its existing finite keyword grammar. The rollback
  representation is private; the public computed value remains the finite
  `VerticalAlignValue` enum.
- A bounded candidate sequence resolves the winning declaration. Rollback
  blocks only the current bounded layer and resolves through its lower concrete
  candidate; if no concrete candidate remains, it resolves through the
  inherited parent value. The root fallback remains `baseline`.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, descendants,
  same-block valid declarations, and invalid-later-declaration preservation
  remain explicit behavior.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- `baseline` and `top` continue to retain the line origin; `middle` and
  `bottom` continue to use the existing finite remaining-line-height offsets.
- The resolved value continues to travel through the existing inline-item
  range and line flush owner, including complete inline-box and text artifact
  ranges, without a new geometry representation.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, fixed-cell, and bounded. It
does not add baseline/font metrics, `text-top`/`text-bottom`, `sub`/`super`,
lengths, percentages, bidi, writing modes, ruby/table-cell/replaced-element
alignment, multiple origins, animation, script, or browser-wide CSS parity.

## Tradeoffs

- Reusing the generic private inherited-text wrapper and resolver avoids a
  property-specific cascade implementation, at the cost of grouping a line
  placement owner with the text-presentation declaration type.
- One fixed candidate array adds bounded style-walk state, but keeps rollback
  local and avoids changing public computed style or layout/artifact schemas.
- The focused regression must assert physical line-item geometry and inherited
  fallback together because a cascade-only assertion would not prove that
  middle/bottom offsets still reach the shared line flush owner.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, length, and percentage forms;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  inherited descendants, root fallback, same-block order, and invalid-later
  declarations;
- baseline/top/middle/bottom line-item placement, inline text fragments,
  display-list/raster geometry, overflow/capture, point hit testing, and
  unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.
