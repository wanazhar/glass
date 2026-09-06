---
id: native-engine-124
scope: glass-browser/native-engine/cascade-layers-decoration-style-revert-layer
status: planned
depends-on: [native-engine-123]
---

# Native bounded `text-decoration-style` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery from native-engine-122 and the
private declaration boundary proven by native-engine-123 to support the
explicit CSS-wide `revert-layer` keyword for the existing inherited
`text-decoration-style` property. Keep the finite public style enum, immutable
text command, and software raster owner unchanged.

## Context

The native engine already supports the five finite decoration patterns
`solid`, `dashed`, `dotted`, `double`, and `wavy` through one inherited computed
style and one immutable text command. Slices 122 and 123 established bounded
top-level named-layer ordering and private rollback for two inherited
decoration properties. The style property is the next adjacent consumer: its
resolved value already selects the existing pattern helper and has no separate
layout or geometry owner.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-style-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-123.md`

## Contract

### Declaration and cascade state

- `text-decoration-style: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `solid`, `dashed`, `dotted`, `double`, and `wavy` values remain the
  only resolved public values. The private declaration state may contain
  `Value(Solid|Dashed|Dotted|Double|Wavy)` or `RevertLayer`; no unresolved
  keyword may reach `NativeDisplayCommand` or the software rasterizer.
- Stylesheet candidates use the 122 layer registry and candidate slots:
  first-appearance layer rank precedes selector specificity, source order
  remains the tie-breaker within one layer, and inline declarations stay in
  the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants receive their parent style; root fallback remains `Solid`,
  the property's current native initial value.
- `revert`, `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates, and
  unknown values remain unsupported typed diagnostics without raw stylesheet
  echo.

### Existing owners preserved

The resolved style continues through the existing inherited computed style,
immutable text command, fixed-cell pattern helper, software decoration replay,
display-list, capture, and decoded raster tests. `double` continues to paint
two solid bands separated by one cell; `wavy` continues to use its existing
bounded phase; `solid`, `dashed`, and `dotted` retain their current patterns.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, skip-ink, skip-spaces, and the 123 layer/parser
owners retain their existing contracts. This remains a horizontal-tb,
fixed-cell, local software-raster contract and does not claim browser-wide CSS
conformance or browser parity.

## Tradeoffs

- A third explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine; the
  type keeps the finite public style values and rollback-only keyword state
  visibly separate.
- Reusing the existing candidate-slot model exercises layer ordering across
  three inherited decoration properties, while style's five public values
  continue to be owned by the existing raster helper. Other properties retain
  their current unsupported-keyword diagnostics.
- Root fallback remains `Solid` rather than changing omitted-property behavior.
  This preserves existing pattern tests and distinguishes inherited fallback
  from a concrete declaration.
- The implementation does not add `all`, multiple origins, `!important`
  inversion, layer statements, nested/anonymous layers, `@import`, or a
  general CSS-wide keyword parser. Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private style declaration storage,
layer-indexed candidate resolution, case-insensitive `revert-layer` parsing,
and parser/cascade plus display-list/raster regressions. It must not change
package dependencies, feature defaults, crate boundaries, public style values,
layout owners, or unrelated style behavior.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for style and typed rejection of
  unsupported CSS-wide/unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, and the existing 15-layer/invalid-form
  diagnostics inherited from 122;
- rollback from a named layer to a lower named layer, unlayered rollback to the
  highest named layer, repeated rollback, inherited descendant fallback, and
  root `Solid` fallback;
- immutable display-list style values and decoded raster evidence for all
  existing finite styles, proving the keyword cannot reach paint replay.

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
