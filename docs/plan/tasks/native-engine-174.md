---
id: native-engine-174
scope: glass-browser/native-engine/cascade-logical-border-family
status: planned
depends-on: [native-engine-173]
---

# Native bounded logical border family

## Objective

Add the bounded logical border shorthand and component-longhand family to the
native CSS surface:
`border-block`, `border-block-start`, `border-block-end`, `border-inline`,
`border-inline-start`, `border-inline-end`, plus their `-width`, `-style`, and
`-color` counterparts. Map those declarations to the existing physical
top/right/bottom/left border cascade, computed style, box-model, display-list,
raster, capture, point-hit, and semantic owners.

This closes the documented logical-border gap without creating a second border
implementation. It is a horizontal-tb native boundary: block-start/end map to
top/bottom, while inline-start/end map to left/right for `ltr` and right/left
for `rtl` using the existing inherited `direction` owner.

## Context

Native-engine-018 established independently cascaded physical border sides.
Native-engine-145 established bounded layer rollback for those sides, and
native-engine-159 through native-engine-173 completed the private border
component and CSS-wide declaration families. The physical owners already
carry declaration order, named-layer precedence, inline precedence,
`currentColor`, `none`, `hidden`, and the four CSS-wide keywords. Logical
properties currently have no parser or projection state, so they are ignored
before reaching any border consumer.

Normative references:

- <https://www.w3.org/TR/css-logical-1/#border-logical>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-173.md`
- `docs/plan/tasks/native-engine-018.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Supported declarations

- Complete logical shorthands accept the existing bounded complete-border
  grammar: one width, one supported style, and one color; exact standalone
  `none`, `hidden`, `inherit`, `unset`, `initial`, `revert`, and
  `revert-layer`; and the already-supported `currentColor` forms.
- Logical width and style pair shorthands accept one or two existing bounded
  component values. Logical color pair shorthands accept one or two existing
  bounded color values, including `currentColor`. A one-value pair applies to
  both logical sides; two values apply to logical start then logical end.
- Logical side longhands accept one existing bounded component value. CSS-wide
  keywords are standalone only; mixed CSS-wide/concrete pairs remain invalid
  and preserve the prior valid declaration according to the existing parser
  contract.
- Supported property names are the six logical border shorthands and their
  `-width`, `-style`, and `-color` forms. No logical radius, image, or table
  conflict owner is implied.

### Cascade and mapping

- Logical declarations are collected in private logical start/end candidate
  streams with the same named-layer, unlayered, specificity, source-order,
  inline, and `revert-layer` behavior as physical declarations.
- Before border resolution, the already-resolved `direction` for the current
  node selects the physical destination: block-start/top, block-end/bottom,
  inline-start/left and inline-end/right for `ltr`; inline-start/right and
  inline-end/left for `rtl`.
- Logical and physical declarations compete in the same per-side component
  streams. A later or stronger physical declaration can override a logical
  declaration and vice versa; a logical `revert-layer` rolls back only the
  mapped physical side/component. Complete logical declarations project into
  the existing width/style/color streams so independent component composition
  remains intact.
- Declaration mapping is resolved from the current node's direction after
  the direction cascade is complete. This keeps `direction` and logical
  border declarations in one coherent computed snapshot, including inline
  styles and inherited RTL descendants.

### Existing owners preserved

- No logical sentinel reaches `NativeComputedStyle`, public border values,
  display commands, PNG bytes, raster pixels, hit testing, semantics, or
  diagnostics transport. The public physical side ordering remains
  top/right/bottom/left.
- Existing physical parsing, CSS-wide/reset behavior, `currentColor`
  substitution, no-paint `none`/`hidden` state, rounded geometry, clipping,
  opacity, scroll projection, source order, and failure-atomic navigation
  remain unchanged.
- The feature remains local-resource-only, integer-pixel, fixed-cell,
  non-table, and explicitly non-browser-parity. `writing-mode`, vertical
  writing, logical `border-radius`, `border-image`, gradients, table border
  conflict resolution, multiple origins, `!important` inversion, and
  browser-wide logical-border conformance remain outside this slice.

## Tradeoffs

- Reusing physical candidate arrays avoids a second layout/paint path and
  keeps the public API stable, at the cost of a private projection pass after
  direction resolution.
- Supporting horizontal-tb plus inherited ltr/rtl gives useful logical CSS
  coverage while avoiding a false writing-mode claim. Vertical writing will
  require an explicit future owner rather than silently using the wrong axis.
- Pair shorthand parsing is intentionally limited to one or two values and
  the existing finite value grammars. Broader CSS tokenization, percentages,
  variables, and arbitrary omitted defaults remain explicit diagnostics or
  ignored unsupported input.
- No dependency is added. Implementation remains inside `glass-browser`'s
  default-off `native-engine` feature, preserving the two-crate boundary and
  build profile.

## Touched owners

- `crates/glass-browser/src/browser/native_engine/css.rs`: private logical
  declaration storage, pair parsing, cascade collection, direction mapping,
  and physical-stream projection.
- `crates/glass-browser/tests/native_engine.rs`: end-to-end logical cascade,
  direction, layout, border command, raster, capture, hit, semantic, and
  diagnostics coverage.
- After behavior is complete, synchronize the architecture record, analysis
  matrix, plan history, root/crate capability docs, and this task's evidence
  and cleanup sections. No public schema or crate manifest changes are
  expected.

## Verification plan

- Format and run one locked native-feature `cargo check` before tests.
- Run focused parser/cascade unit coverage for all logical property families,
  pair expansion, invalid mixed values, direction mapping, physical/logical
  precedence, repeated rollback, and inline precedence.
- Run one focused integration fixture covering ltr/rtl descendants,
  block/inline side placement, complete and component forms, `none`/`hidden`,
  CSS-wide values, `currentColor`, layer rollback, physical overrides, box
  geometry, display commands, decoded raster/PNG capture, point-hit, semantic
  order, and bounded diagnostics.
- Near completion run full native integration and feature-enabled browser
  library tests, strict all-target/all-feature Clippy, warning-denied native
  rustdoc, paired locked `glass-dev` check/build, both locked package flows,
  and repository static/workspace gates.
- Record exact command results, counts, known warnings, remote-CI boundary,
  and cleanup evidence here before marking the task complete.

## Cleanup

Pending exact-path regenerable-output cleanup until all Cargo/Rust processes
and open handles exit. Only task-specific isolated targets and reports may be
removed; source, durable data, repository history, and unrelated workloads
must remain untouched.

## Evidence

Implementation: pending.

Documentation/certification: pending.

Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary, and promotion claims: not made.
