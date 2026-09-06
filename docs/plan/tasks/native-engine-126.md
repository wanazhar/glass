---
id: native-engine-126
scope: glass-browser/native-engine/cascade-layers-underline-offset-revert-layer
status: planned
depends-on: [native-engine-125]
---

# Native bounded `text-underline-offset` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery and private rollback declaration
boundary proven by native-engine-125 to support the explicit CSS-wide
`revert-layer` keyword for the existing inherited `text-underline-offset`
property. Keep the finite signed pixel value, immutable text command, and
underline-only software raster owner unchanged.

## Context

The native engine already supports inherited signed underline offsets from
`-4px` through `4px`. The resolved value translates only the underline in the
existing immutable text command; overline and line-through retain their fixed
origins. Slices 122-125 established bounded top-level named-layer order and
private rollback for four inherited decoration properties. Underline offset is
the next adjacent consumer and must exercise the same cascade boundary without
widening the paint or geometry contract.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-underline-offset-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-125.md`

## Contract

### Declaration and cascade state

- `text-underline-offset: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `-4px` through `4px` integer values remain the only resolved public
  values. The private declaration state may contain `Value(-4..=4)` or
  `RevertLayer`; no unresolved keyword may reach `NativeDisplayCommand` or the
  software rasterizer.
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
  used. Descendants receive their parent offset; root fallback remains `0`,
  the property's current native initial value.
- `auto`, percentages, fractional values, positive-sign syntax, dimensions
  outside `-4px` through `4px`, `revert`, `inherit`, `unset`, `initial`,
  `all`, mixed tokens, duplicates, and unknown values remain unsupported typed
  diagnostics without raw stylesheet echo.

### Existing owners preserved

The resolved offset continues through the existing inherited computed style,
immutable text command, display-list, capture, and decoded raster path. Only
the underline origin is translated; overline and line-through continue to use
their existing fixed origins. Thickness, style, color, skip-ink, skip-spaces,
text layout, and all existing decoration geometry remain unchanged.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, the 125 parser and layer owners, and all unrelated
style diagnostics retain their existing contracts. This remains a
horizontal-tb, fixed-cell, local software-raster contract and does not claim
browser-wide CSS conformance or browser parity.

## Tradeoffs

- A fifth explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine. The
  type keeps the signed offset artifact separate from rollback-only keyword
  state.
- Reusing the existing candidate-slot model proves the same layer semantics for
  the existing underline translation owner. Other properties retain their
  current unsupported-keyword diagnostics.
- Root fallback remains `0` rather than changing omitted-property behavior.
  This preserves the current no-offset default and distinguishes inherited
  fallback from a concrete declaration.
- The implementation does not add `auto`, percentages, fractional or
  font-derived offsets, overline/line-through offsets, `all`, multiple origins,
  `!important` inversion, layer statements, nested/anonymous layers, or a
  general CSS-wide keyword parser. Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private underline-offset declaration
storage, layer-indexed candidate resolution, case-insensitive `revert-layer`
parsing, and parser/cascade plus display-list/raster regressions. It must not
change package dependencies, feature defaults, crate boundaries, public offset
values, underline-only ownership, or unrelated style behavior.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for underline offset and typed
  rejection of unsupported CSS-wide, dimension, sign, and unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, rollback from named and unlayered buckets,
  repeated rollback, inherited descendant fallback, and root `0px` fallback;
- immutable signed offset display-list values and decoded-raster evidence that
  moves underline only, proving the keyword cannot reach paint replay or change
  overline/line-through origins;
- the existing offset translation and thickness/style/color/skip-ink/
  skip-spaces owners remaining unchanged.

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
