---
id: native-engine-123
scope: glass-browser/native-engine/cascade-layers-skip-ink-revert-layer
status: planned
depends-on: [native-engine-122]
---

# Native bounded `text-decoration-skip-ink` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery from native-engine-122 to support the
explicit CSS-wide `revert-layer` keyword for the existing inherited
`text-decoration-skip-ink` property. Keep the resolved `Auto|None` paint value,
immutable text command, and software raster owner unchanged.

## Context

Native-engine-122 introduced first-appearance ordering for at most 15 named
top-level layers, an implicit unlayered bucket above them, and private
`revert-layer` rollback for `text-decoration-skip-spaces`. The native engine
already resolves `text-decoration-skip-ink` as an inherited finite
`Auto|None` value, but its declaration storage still assumes that every valid
candidate is immediately resolved. This slice exercises the same layer
boundary with a second inherited decoration property while preserving the
private/public separation.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-ink-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-122.md`

## Contract

### Declaration and cascade state

- `text-decoration-skip-ink: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `auto` and `none` values remain the only resolved public values.
  The private declaration state may contain `Value(Auto)`, `Value(None)`, or
  `RevertLayer`; no unresolved keyword may reach `NativeDisplayCommand` or
  the software rasterizer.
- Stylesheet candidates use the 122 layer registry and candidate slots:
  first-appearance layer rank precedes selector specificity, existing source
  order remains the tie-breaker within one layer, and inline declarations stay
  in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants therefore receive their parent `Auto|None` value; root
  fallback remains `Auto`, the property's current native initial value.
- `revert` remains unsupported for this property in this slice, as do
  `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates, and unknown
  values. Each remains a bounded typed diagnostic without raw stylesheet echo.

### Existing owners preserved

The resolved value continues through the existing inherited computed style,
immutable text command, glyph-intersection check, line-edge provenance,
software decoration replay, display-list, capture, and decoded raster tests.
`Auto` continues to suppress decoration pixels intersecting eligible glyphs;
`None` continues to paint through them. No new skip-ink behavior is invented.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, and the 122 layer parser owners retain their existing
contracts. This remains a horizontal-tb, fixed-cell, local software-raster
contract and does not claim browser-wide CSS conformance or browser parity.

## Tradeoffs

- A second private declaration state duplicates a small amount of resolver
  shape instead of prematurely designing a generic CSS-wide keyword engine;
  the type remains explicit about the distinct inherited property and its
  finite public value.
- Reusing the 122 candidate-slot model proves layer ordering across two
  properties, but only `text-decoration-skip-ink` and `text-decoration-skip-
  spaces` have meaningful rollback semantics. Other properties retain their
  existing unsupported-keyword diagnostics.
- Root fallback remains `Auto` rather than changing the omitted-value
  behavior. This keeps the slice compatible with existing raster expectations
  while distinguishing inherited fallback from a concrete declaration.
- The implementation does not add `all`, multiple origins, `!important`
  inversion, layer statements, nested/anonymous layers, `@import`, or a
  general CSS-wide keyword parser. Those boundaries remain visible and typed.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private skip-ink declaration storage,
layer-indexed candidate resolution, case-insensitive `revert-layer` parsing,
and parser/cascade plus display-list/raster regressions. It must not change
package dependencies, feature defaults, crate boundaries, public paint values,
layout owners, or unrelated style behavior.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for skip-ink and typed rejection of
  unsupported CSS-wide/unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, and the existing 15-layer/invalid-form
  diagnostics inherited from 122;
- rollback from a named layer to a lower named layer, unlayered rollback to the
  highest named layer, repeated rollback, inherited descendant fallback, and
  root `Auto` fallback;
- display-list values and decoded raster evidence for both `Auto` and `None`,
  proving the keyword cannot reach paint replay.

Then run the established native feature library/integration suites, locked
`glass-dev` tests, strict Clippy, no-default-feature Clippy, warnings-denied
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
