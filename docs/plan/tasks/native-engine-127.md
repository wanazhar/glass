---
id: native-engine-127
scope: glass-browser/native-engine/cascade-layers-decoration-color-revert-layer
status: planned
depends-on: [native-engine-126]
---

# Native bounded `text-decoration-color` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery and private rollback declaration
boundary proven by native-engine-126 to support the explicit CSS-wide
`revert-layer` keyword for the existing local `text-decoration-color`
property. Keep the existing fixed palette/alpha grammar, separate glyph and
decoration paint colors, immutable text command, and software raster owners
unchanged.

## Context

The native engine already supports a local `text-decoration-color` value using
the bounded `NativeColor` parser. An explicit color is carried separately from
the run's glyph color and is used only by underline, overline, and line-through
pixels. An omitted decoration color remains represented as `None`; the paint
path then uses its existing resolved text-color fallback. This is a local
property, not an inherited one, so a descendant with no winning declaration
does not receive a parent decoration color.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-color-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-126.md`

## Contract

### Declaration and cascade state

- `text-decoration-color: revert-layer` is accepted as one case-insensitive
  token and stored in a private declaration-only state.
- Concrete values remain the existing bounded `NativeColor` grammar,
  including the explicit zero-alpha `transparent` value. The private
  declaration state may contain `Value(NativeColor)` or `RevertLayer`; no
  unresolved keyword may reach `NativeComputedStyle`, `NativeDisplayCommand`,
  capture, or software raster replay.
- Stylesheet candidates use the existing 15 named-layer registry and the
  unlayered bucket: first-appearance layer rank precedes selector specificity,
  source order remains the tie-breaker within one layer, and inline
  declarations stay in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  color is found or no candidate remains.
- When no candidate remains, the existing local fallback is used: computed
  decoration color is `None`, allowing the current paint path to use the run's
  text color. Parent decoration colors do not cross the local-property
  boundary, and root/descendant fallback remains `None`.
- Existing named colors, hex colors, bounded `rgb(...)`/`rgba(...)`, and
  `transparent` remain supported exactly as before. `currentColor`, gradients,
  system colors, percentages, CSS-wide keywords other than this explicit
  `revert-layer`, `all`, mixed tokens, duplicates, and unknown values remain
  unsupported typed diagnostics without raw stylesheet echo.

### Existing owners preserved

The resolved optional color continues through the existing immutable text
command, display-list, capture, and decoded raster path. Only decoration
pixels use the explicit color; glyph pixels retain the resolved text color.
Underline, overline, and line-through geometry, offsets, thickness, style,
skip-ink, skip-spaces, clipping, opacity, scrolling, and source/semantic order
remain unchanged.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, and the 126 parser/layer/offset owners retain their
existing contracts. This remains a horizontal-tb, fixed-cell, local
software-raster contract and does not claim browser-wide CSS conformance or
browser parity.

## Tradeoffs

- A private color declaration enum duplicates the small rollback resolver shape
  rather than prematurely creating a generic CSS-wide keyword engine. The
  public `Option<NativeColor>` artifact stays unchanged.
- The local fallback is deliberately `None`, unlike the inherited fallback of
  the recent skip-ink, style, thickness, and underline-offset slices. This
  preserves the established distinction between an omitted decoration color
  and an explicit color and prevents accidental parent propagation.
- Reusing the existing candidate-slot model proves rollback for the separate
  glyph/decoration paint owner without changing raster patterns or command
  schema.
- The implementation does not add `currentColor`, color inheritance,
  gradients, system colors, color-space conversion, animations, multiple
  origins, `!important` inversion, layer statements, nested/anonymous layers,
  or a general CSS-wide keyword parser. Those boundaries remain typed and
  visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private decoration-color declaration
storage, layer-indexed candidate resolution, case-insensitive `revert-layer`
parsing, and parser/cascade plus display-list/raster regressions. It must not
change package dependencies, feature defaults, crate boundaries, the public
`Option<NativeColor>` artifact, glyph paint, or unrelated style behavior.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for decoration color and typed
  rejection of unsupported CSS-wide, color-space, dimension, and unknown
  forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, rollback from named and unlayered buckets,
  repeated rollback, local no-candidate fallback, and the fact that parent
  decoration colors do not inherit;
- immutable display-list color values and decoded-raster evidence that only
  decoration pixels change while glyph pixels retain their color;
- the existing transparent and omitted-color fallback behavior, all three
  line owners, and the 126 underline-offset/style/thickness/skip owners
  remaining unchanged.

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
