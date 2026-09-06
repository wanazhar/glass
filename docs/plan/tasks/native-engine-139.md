---
id: native-engine-139
scope: glass-browser/native-engine/cascade-layers-inherited-text-presentation-revert-layer
status: planned
depends-on: [native-engine-138]
---

# Native bounded inherited text-presentation `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-138 to
add standalone, case-insensitive `revert-layer` to the existing inherited
`text-transform`, `font-weight`, `font-style`, and `word-break` declarations.
The four owners must resolve independently while preserving their existing
fixed-cell layout, wrapping, immutable text-command, display-list, raster,
overflow, capture, hit-test, and semantic/source-order contracts.

## Context

The native engine already accepts finite concrete values for these four
inherited properties and carries them through the DOM style walk. Their
stylesheet and inline declarations currently keep only one winning concrete
candidate, so a higher-priority rollback declaration cannot expose a lower
candidate or the inherited parent value. This slice adds only private
declaration/candidate state and reuses the existing bounded first-appearance
15-layer registry.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-3/#text-transform-property>
- <https://www.w3.org/TR/css-fonts-4/#font-weight-prop>
- <https://www.w3.org/TR/css-fonts-4/#font-style-prop>
- <https://www.w3.org/TR/css-text-3/#word-break-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-138.md`

## Contract

### Declaration and cascade state

- `text-transform`, `font-weight`, `font-style`, and `word-break` accept one
  standalone, case-insensitive `revert-layer` token in addition to their
  existing finite concrete grammars. The rollback representation is private;
  the public computed values remain `None|Uppercase|Lowercase`,
  `Normal|Bold`, `Normal|Italic`, and `Normal|BreakAll`.
- Each property gets its own bounded candidate sequence. A rollback blocks only
  the current bounded layer for that property and resolves through its lower
  concrete candidate; if no concrete candidate remains, it resolves through
  the inherited parent value. The root defaults remain `None`, `Normal`,
  `Normal`, and `Normal` respectively.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, descendants,
  same-block valid declarations, and invalid-later-declaration preservation
  remain explicit behavior.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- `text-transform` continues to transform only supported ASCII letters during
  fixed-cell layout while semantic/source text remains unchanged.
- `font-weight` and `font-style` continue to preserve fixed-cell advances and
  feed the existing clipped bold-dilation and row-dependent italic-shear
  raster owners.
- `word-break` continues to select the existing normal or character-boundary
  wrapping path, with all emitted text fragments and artifact geometry shared.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary changes.

The slice remains fixture-relative, horizontal-tb, fixed-cell, and bounded. It
does not add Unicode case mapping, font selection/metrics, variable-font
interpolation, oblique angles, `keep-all`/`break-word`, percentages, multiple
origins, animation, script, grid, writing modes, or browser-wide CSS parity.

## Tradeoffs

- A generic private declaration wrapper can share the rollback resolver across
  four value types and avoids four copies of identical layer logic, at the
  cost of less property-specific type naming in the parser.
- Four fixed candidate arrays add a small style-walk cost and bounded stack
  state, but keep rollback local to each inherited owner and avoid changing
  public computed-style or artifact structures.
- Grouping layout- and raster-adjacent inherited text properties reduces
  checkpoint/build overhead; the focused tests must therefore cover each
  owner separately before the shared full native gate.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported concrete forms for all four properties;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  inherited descendants, root fallback, same-block order, and invalid-later
  declarations for each owner;
- transformed and wrapped text layout, bold/italic display/raster output,
  overflow/capture, point hit testing, and unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback keyword
  leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.
