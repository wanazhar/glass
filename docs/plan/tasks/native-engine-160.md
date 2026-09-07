---
id: native-engine-160
scope: glass-browser/native-engine/cascade-border-none-complete
status: planned
depends-on: [native-engine-159]
---

# Native bounded complete `border: Npx none color`

## Objective

Accept the bounded complete-value `Npx none color` form for `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left`, with
case-insensitive `none`. Preserve the exact omitted-component `none` and
`hidden` forms, the complete painted `Npx style color` grammar, the complete
`Npx hidden color` form, and standalone `revert-layer`.

The complete none form must carry its declared width and color through the
existing private component streams while mapping only its style to the private
`NativeBorderStyleValue::None` sentinel. A winning none style must suppress
current non-table paint and retain the existing no-side/zero-width public
result. No public `None` paint variant or display-list schema is permitted.

## Context

The native engine now accepts exact omitted-component `border:none` and
`border:hidden`, complete painted values, and complete hidden values. Its
complete parser still rejects `2px none red` because the public
`NativeBorderSide` stores painted styles only. The existing private none style
sentinel already owns no-paint cascade behavior. This slice adds a private
complete-none declaration rather than leaking a no-style value into public
computed values or raster commands.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-159.md`
- `docs/plan/tasks/native-engine-158.md`
- `docs/plan/tasks/native-engine-157.md`

## Contract

### Declaration and cascade state

- The five physical border shorthand properties accept the existing complete
  bounded `Npx style color` form for public painted styles, the exact
  case-insensitive omitted-component `none` or `hidden` token, the complete
  bounded `Npx none color` or `Npx hidden color` form, or standalone
  case-insensitive `revert-layer`.
- A complete none declaration is represented by a private declaration-only
  value carrying its bounded width and parsed color. Its style projection is
  `NativeBorderStyleValue::None`; its width and color projections are the
  declared values at the same declaration order. It does not add a public
  `NativeBorderStyle::None` variant.
- A winning complete none style blocks current non-table paint and resolves to
  the existing no-side/zero-width result. Width and color remain typed private
  candidates for future table/conflict work, but cannot resurrect a painted
  side in current computed-style composition. A later bounded `revert-layer`
  can expose an existing lower painted component.
- Complete none values with missing width/color, extra tokens, unsupported
  styles, other CSS-wide keywords, empty values, malformed dimensions/colors,
  style-only values other than the exact omitted-component tokens, and
  unsupported CSS syntax remain typed unsupported-value diagnostics. Raw CSS
  text is not added to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered/inline
  precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative for all three
  component streams.

### Existing owners preserved

- Exact omitted-component `none` continues to use its existing private
  declaration and does not invent width/color candidates. Exact omitted
  `hidden`, complete hidden, and complete painted borders remain unchanged.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add collapsed-table conflict resolution, logical
  sides, arbitrary omitted defaults, CSS-wide reset machinery, `currentColor`,
  gradients, border-image, animation, multiple origins, `!important`
  inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A private complete-none value preserves the declared width/color provenance
  needed by the existing component-cascade model and future table work without
  expanding public enums or claiming that current non-table rendering paints
  a none border.
- The parser remains exact and bounded: it adds `Npx none color`, not
  arbitrary omitted combinations, default-width/current-color inference,
  CSS-wide keyword semantics, or a general border conflict model.
- Retaining width/color privately while suppressing them at current computed
  style is deliberate. The private/public separation must be proven by tests;
  the current renderer must never emit a border command for a winning none
  style.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- case-insensitive complete `Npx none color` parsing for the shorthand and
  all four physical properties, preservation of complete painted/omitted and
  complete hidden values, standalone `revert-layer`, and typed rejection of
  incomplete, mixed, malformed, CSS-wide, and unsupported inputs;
- private complete-none width/style/color separation, declaration order,
  named-layer/specificity/source-order/inline precedence, same-block order,
  repeated rollback, and valid-before-invalid preservation;
- current no-side/zero-width geometry and absent border commands/raster while
  declared width/color remain private, plus clipping, point-hit, capture,
  semantic/source order, and unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

Not implemented. This task is the docs-first design and acceptance contract;
implementation must remain a separate coherent batch after this record is
committed and issue #40 is updated with the design checkpoint.

## Evidence

To be filled after implementation. The evidence must record exact commands,
affected-package results, test counts, static documentation truth, and exact
regenerable-target cleanup. It must not imply remote CI, release, publication,
browser parity, or security-boundary certification.
