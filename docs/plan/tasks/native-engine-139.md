---
id: native-engine-139
scope: glass-browser/native-engine/cascade-layers-inherited-text-presentation-revert-layer
status: complete
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
inherited properties and carries them through the DOM style walk. This slice
replaces the single-winner declaration state for these owners with private
per-property candidates so a higher-priority rollback declaration can expose a
lower candidate or the inherited parent value. It reuses the existing bounded
first-appearance 15-layer registry.

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

## Implementation

Implemented in `0114659d56fadb87e41fb31b5dff5842a4e72537` with the design
checkpoint `16589ae4e1fb0febde17879de54519d583aeeb6f`. The implementation adds
one private generic declaration wrapper and four fixed candidate arrays, keeps
the public computed values unchanged, and routes the existing transform,
wrapping, display-list, raster, overflow, capture, hit-test, and
semantic/source-order owners through the resolved finite values. No dependency,
feature, public schema, or crate-boundary changes were made.

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

## Verification evidence

The completed gate covered:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported concrete forms for all four properties;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  inherited descendants, root fallback, same-block order, and invalid-later
  declarations for each owner;
- transformed and wrapped text layout, bold/italic display/raster output,
  overflow/capture, point hit testing, and unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback keyword
  leakage; and
- focused `glass-browser` check passed;
- parser and cascade unit tests passed (1/1 each), and the integrated consumer
  regression passed (1/1, 175 filtered);
- full native integration passed (176/176);
- the native-feature `glass-browser` library passed (942 passed, 1 ignored);
- strict affected-package Clippy passed with `-D warnings`;
- formatting and final static documentation gates passed: 553 Markdown
  documents, 83 current documents, 57 previous-version hits, 649 semantic
  audit hits, and zero current-claim failures; coverage reported 345
  full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules; depth reported 93 current guides and 19 substantive contracts;
  parity reported 14 capabilities across 4 targets; TUI reported 15
  implementation keys and 63 documentation markers; adapters reported 5;
  reliability reported 6 scenarios across 4 targets; and Web IR reported
  8 fixtures, 8 scenarios, and 11 categories;
- the temporary `/tmp/glass-139-focused` target measured 5,850,572,751 logical
  bytes across 6,490 files and 887 directories, with no open handles or
  active Cargo/rustc/Clippy consumers; the exact target and 4,742-byte audit
  directory were removed with bounded `find -P ... -xdev -depth -delete`,
  reclaiming 5,870,452,736 bytes of measured `/tmp` free space; and
- remote CI remains unclaimed because the checkout is local-only and no push
  was authorized.
