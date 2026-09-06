---
id: native-engine-142
scope: glass-browser/native-engine/cascade-layers-local-text-indent-text-overflow-revert-layer
status: planned
depends-on: [native-engine-141]
---

# Native bounded local `text-indent` and `text-overflow` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-141 to
add standalone, case-insensitive `revert-layer` to the existing local
`text-indent` and `text-overflow` declarations. The two owners must resolve
independently while preserving first-line fixed-cell flow, eligible clipped
single-line truncation, text fragments, display-list, raster, overflow,
capture, hit-test, and semantic/source-order contracts.

## Context

The native engine already accepts bounded non-negative integer-pixel
`text-indent` and finite `text-overflow:clip|ellipsis` values. Their stylesheet
and inline paths currently keep one winning concrete candidate, so a
higher-priority rollback cannot expose a lower candidate or the local fallback.
This slice adds only private declaration/candidate state and reuses the bounded
15-layer registry and generic candidate resolver; it does not make either
property inherited.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-3/#text-indent-property>
- <https://www.w3.org/TR/css-overflow-4/#text-overflow-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-141.md`

## Contract

### Declaration and cascade state

- `text-indent` accepts one standalone, case-insensitive `revert-layer` token
  in addition to its existing finite non-negative integer-pixel grammar. The
  rollback representation is private; the public computed value remains a
  finite `u32` pixel value and the local fallback remains `0px`.
- `text-overflow` accepts one standalone, case-insensitive `revert-layer`
  token in addition to its existing finite `clip|ellipsis` grammar. The
  rollback representation is private; the public computed value remains the
  finite enum and the local fallback remains `clip`.
- Each property gets an independent bounded candidate sequence. Rollback
  blocks only the current bounded layer for that property and resolves through
  its lower concrete candidate; if no concrete candidate remains, it resolves
  through the local fallback rather than an inherited parent value.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, same-block valid
  declarations, and invalid-later-declaration preservation remain explicit
  behavior.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- `text-indent` continues to shift only the first line of eligible block flow,
  with its existing one-cell clamp and shared flow cursor; it remains local and
  does not inherit to descendants or inline children.
- `text-overflow` continues to affect only the existing eligible clipped,
  single-line, direct-text `nowrap` path; `clip` keeps the original run and
  `ellipsis` keeps the existing fixed `...` marker and truncation metadata.
- The resolved values continue through the existing text-fragment,
  display-list, raster, root-overflow, capture, point-hit-test, and
  semantic/source-order owners without a new geometry or artifact schema.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, fixed-cell, and bounded. It
does not add negative or hanging indentation, `each-line`, percentages,
relative/font-derived units, inherited text-overflow, custom markers, multiline
ellipsis, nested inline formatting, multiple origins, animation, script, or
browser-wide CSS parity.

## Tradeoffs

- Reusing one generic private local resolver avoids duplicate rollback logic,
  while two property-local candidate arrays keep indentation and truncation
  fallbacks independent.
- Grouping the two local text owners reduces checkpoint and static-audit
  overhead, but the focused regression must cover both first-line coordinates
  and clipped-nowrap ellipsis so a cascade-only test cannot hide a consumer
  regression.
- Keeping the public `u32` and `TextOverflowValue` fields unchanged preserves
  downstream build stability at the cost of intentionally excluding the wider
  CSS grammar and marker model.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, negative, percentage, relative, and
  font-derived forms for both properties;
- independent named-layer priority, repeated rollback, unlayered/inline
  precedence, same-block order, local fallback, and invalid-later preservation
  for `text-indent` and `text-overflow`;
- first-line coordinates and one-cell indentation clamp, clipped-nowrap
  ellipsis eligibility and marker/truncated metadata, text fragments,
  display-list/raster geometry, overflow/capture, point hit testing, and
  unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.
