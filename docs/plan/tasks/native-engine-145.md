---
id: native-engine-145
scope: glass-browser/native-engine/cascade-layers-paint-color-revert-layer
status: planned
depends-on: [native-engine-144]
---

# Native bounded paint-color `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-144 to
add standalone, case-insensitive `revert-layer` to the existing
`background-color` and inherited `color` owners. The two properties must keep
independent private candidate sequences while preserving the existing
display-list, software-surface, capture, text, overflow, hit-test, and
semantic/source-order contracts.

## Context

The native engine already accepts a bounded `NativeColor` grammar for
`background-color` and inherited `color`, including named colors, short/full
hex, bounded `rgb(...)`, bounded `rgba(...)`, and `transparent`. Their current
stylesheet and inline paths retain only one concrete winner, so a higher
priority rollback cannot expose a lower candidate or the established omitted
fallback. This slice adds only private declaration/candidate state and reuses
the bounded 15-layer registry and optional local resolver.

`background-color` remains a local paint property whose omitted fallback is
`None`, meaning no background fill command is emitted. `color` remains an
inherited optional value: when no local concrete candidate survives rollback,
the existing parent/root inherited color is used, and the paint path retains
its existing black fallback for an absent root color. The already-completed
`text-decoration-color` rollback remains a separate local paint owner and is
not merged into this slice.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-color-4/#the-color-property>
- <https://www.w3.org/TR/css-backgrounds-3/#background-color>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-144.md`
- `docs/plan/tasks/native-engine-127.md`

## Contract

### Declaration and cascade state

- `background-color` and `color` accept one standalone, case-insensitive
  `revert-layer` token in addition to the existing bounded color grammar. A
  rollback is private and cannot reach `NativeComputedStyle`, display-list
  commands, capture bytes, or software raster replay as an unresolved keyword.
- Each property receives an independent bounded candidate sequence. A winning
  rollback blocks only its current bounded layer and resolves through the
  highest-priority remaining concrete candidate. Repeated rollback continues
  through lower candidates; unlayered and inline declarations remain above
  named layers.
- `background-color` resolves to an optional concrete color with the existing
  `None` local fallback. An absent result produces no background fill, as it did
  before this slice.
- `color` resolves to an optional concrete color with the existing inherited
  parent/root fallback. A local rollback cannot erase an inherited color, and a
  local concrete `transparent` value remains distinct from an absent result.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; inline declarations
  use the existing unlayered bucket. A valid declaration followed by an invalid
  declaration in the same source block preserves the earlier valid candidate.
- Existing `!important` stripping remains in force and is not promoted to a
  separate origin. Other CSS-wide keywords, `currentColor`, gradients, system
  colors, color spaces, percentages, and malformed or mixed forms remain typed
  unsupported values without raw stylesheet echo.

### Existing owners preserved

- Resolved background colors continue through the existing fill display command,
  opacity grouping, clipping, scrolling, capture, and integer source-over
  raster replay.
- Resolved inherited text colors continue through the existing text-run
  display command, descendant inheritance walk, clipping, scrolling, capture,
  hit testing, and fixed-cell glyph raster replay.
- Text-decoration color remains local and independent; an absent decoration
  color still falls back to the text-run color through its existing owner.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not
add `border-color`, `currentColor` substitution, gradients, system colors,
wide-gamut or color-space conversion, animations, multiple origins, or
browser-wide CSS color conformance.

## Tradeoffs

- Reusing `LocalCascadeDeclaration<NativeColor>` for both properties keeps the
  rollback algorithm small and makes optional local fallback explicit, while
  the two candidate arrays preserve independent inherited and local behavior.
- Grouping fill and glyph colors makes one focused regression cover both
  display-list color ownership and decoded raster evidence, at the cost of a
  slightly broader paint test than a single-property change.
- Keeping the public optional color fields unchanged preserves downstream
  layout and artifact stability, but intentionally leaves `currentColor`,
  border-color, gradients, and the general CSS color grammar unsupported.
- Two fixed candidate arrays add bounded per-style cascade state and source
  parsing work, but require no dependency or build-configuration change.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, `currentColor`, gradient, percentage, and
  color-space forms for both properties;
- independent named-layer priority, repeated rollback, unlayered/inline
  precedence, inherited descendant fallback, local absent fallback,
  transparent override, and invalid-later preservation;
- fill and glyph display-list colors plus decoded raster pixels after concrete
  selection, rollback, and absent/inherited fallback; unchanged clipping,
  opacity, scrolling, text-decoration fallback, hit testing, and
  semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.
