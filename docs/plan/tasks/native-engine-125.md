---
id: native-engine-125
scope: glass-browser/native-engine/cascade-layers-decoration-thickness-revert-layer
status: planned
depends-on: [native-engine-124]
---

# Native bounded `text-decoration-thickness` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery and private rollback declaration
boundary proven by native-engine-124 to support the explicit CSS-wide
`revert-layer` keyword for the existing inherited
`text-decoration-thickness` property. Keep the existing finite pixel value,
immutable text command, decoration geometry, and software raster owners
unchanged.

## Context

The native engine already supports inherited integer decoration thicknesses
from `1px` through `4px`. The resolved value is carried by the existing text
command and controls the current fixed-cell decoration bands; there is no
separate layout or geometry representation to introduce. Slices 122-124
established bounded top-level named-layer order and private rollback for three
inherited decoration properties. Thickness is the next adjacent consumer and
must exercise the same cascade boundary without widening the paint contract.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-thickness-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-124.md`

## Contract

### Declaration and cascade state

- `text-decoration-thickness: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `1px`, `2px`, `3px`, and `4px` values remain the only resolved
  public values. The private declaration state may contain
  `Value(1..=4)` or `RevertLayer`; no unresolved keyword may reach
  `NativeDisplayCommand` or the software rasterizer.
- Stylesheet candidates use the existing 15 named-layer registry and the
  unlayered bucket: first-appearance layer rank precedes selector specificity,
  source order remains the tie-breaker within one layer, and inline
  declarations stay in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants receive their parent thickness; root fallback remains
  `1`, the property's current native initial value.
- `auto`, `from-font`, `0px`, `5px`, fractional values, negative values,
  `revert`, `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates,
  and unknown values remain unsupported typed diagnostics without raw
  stylesheet echo.

### Existing owners preserved

The resolved thickness continues through the existing inherited computed style,
immutable text command, display-list, capture, and decoded raster path. The
current one-to-four-cell decoration geometry and style-specific replay remain
the owners for underline, overline, and line-through. No layout dimensions,
line metrics, wrapping, offsets, skip-ink, skip-spaces, style, color, or paint
pattern semantics change.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, the 124 parser and layer owners, and all unrelated
style diagnostics retain their existing contracts. This remains a horizontal-tb,
fixed-cell, local software-raster contract and does not claim browser-wide CSS
conformance or browser parity.

## Tradeoffs

- A fourth explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine. The
  type keeps the finite `u32` thickness artifact separate from rollback-only
  keyword state.
- Reusing the existing candidate-slot model proves the same layer semantics for
  a property whose concrete value already has dedicated geometry tests. Other
  properties retain their current unsupported-keyword diagnostics.
- Root fallback remains `1` rather than changing omitted-property behavior.
  This preserves the current default thickness and distinguishes inherited
  fallback from a concrete declaration.
- The implementation does not add `auto`, `from-font`, percentages, lengths
  outside `1px`-`4px`, `all`, multiple origins, `!important` inversion, layer
  statements, nested/anonymous layers, or a general CSS-wide keyword parser.
  Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private thickness declaration storage,
layer-indexed candidate resolution, case-insensitive `revert-layer` parsing,
and parser/cascade plus display-list/raster regressions. It must not change
package dependencies, feature defaults, crate boundaries, public thickness
values, layout owners, or unrelated style behavior.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for thickness and typed rejection of
  unsupported CSS-wide, dimension, and unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, rollback from named and unlayered buckets,
  repeated rollback, inherited descendant fallback, and root `1px` fallback;
- immutable one-to-four-cell display-list values and decoded-raster evidence
  for underline, overline, and line-through, proving the keyword cannot reach
  paint replay;
- the existing thickness geometry and style-pattern regressions, plus the
  inherited skip-ink, skip-spaces, and style owners remaining unchanged.

Then run the established native feature library/integration suites, locked
two-crate tests, strict Clippy, no-default-feature Clippy, warnings-denied
rustdoc, locked package/fuzz/deny/audit checks, and the repository's static
documentation/reliability/adapter/Web IR validators. Use isolated targets and
record exact commands, counts, durations, warnings, commits, issue
synchronization, and cleanup evidence here. Remote CI, browser parity,
release, registry publication, and a third crate remain outside local task
evidence unless separately executed and verified.

## Cleanup

All expensive gates must use isolated task targets where practical. Before
deleting generated output, verify no Cargo/rustc/rustdoc/Clippy/fuzz/test
writer owns it and no open file handle remains. Remove only exact task targets,
reports, scratch entries, and generated evidence created by this task; preserve
source, durable fixtures, active processes, and unrelated workloads. Final
evidence must show no repository `target/`, no current task target/report
candidates, no open handles, and the available-byte delta.

## Certification

Pending local certification after implementation, full validation, issue #40
synchronization, and exact regenerable-output cleanup.
