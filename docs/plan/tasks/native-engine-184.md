---
id: native-engine-184
scope: glass-browser/native-engine/cascade-logical-border-style-important-priority
status: in-progress
depends-on: [native-engine-183]
---

# Native bounded logical border-style `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the six
supported horizontal-tb logical border-style declarations:
`border-block-style`, `border-block-start-style`, `border-block-end-style`,
`border-inline-style`, `border-inline-start-style`, and
`border-inline-end-style`.

Logical style candidates retain the existing finite paint values and private
`none`/`hidden` no-paint sentinels, CSS-wide forms, and `revert-layer` behavior.
The slice adds only the private important partition and preserves its reversed
named-layer ordering while projecting logical candidates through resolved
`ltr`/`rtl` direction into physical style streams.

## Context

Native-engine-174 established the bounded horizontal-tb logical border
component families; native-engine-180 established the corresponding logical
border-color important partition; native-engine-181 established important
priority for physical border width; native-engine-182 established it for
physical border style; and native-engine-183 established it for logical border
width. Logical border style still uses the normal 15-layer/unlayered candidate
array, so an important logical style can lose its importance metadata before
logical-to-physical projection.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-logical-1/#border-style>
- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-180.md`
- `docs/plan/tasks/native-engine-181.md`
- `docs/plan/tasks/native-engine-182.md`
- `docs/plan/tasks/native-engine-183.md`

## Contract

### Supported priority

- The six logical border-style declarations accept the existing bounded finite
  style grammar, CSS-wide forms, private `none`/`hidden` values, and
  `revert-layer` followed by a terminal case-insensitive `!important` marker.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing one- or two-value logical block/inline expansion, per-side
  source/declaration ordering, private no-paint distinctions, CSS-wide
  resolution, invalid-later preservation, no-style fallback, and
  width/style/color composition remain unchanged.

### Layer and direction ordering

- Normal logical style declarations retain the existing 15-layer/unlayered
  order and selector-specificity/source/declaration/inline tie breakers.
- Important logical styles outrank normal logical styles. Inside the important
  partition, earliest named layers win, later named layers follow, and
  unlayered important declarations are lowest. Inline important values use the
  unlayered important bucket and retain inline precedence within that bucket.
- `revert-layer !important` rolls back the current logical style layer before
  lower important or normal candidates are resolved. Each logical side remains
  independent, including private `none`/`hidden` outcomes.
- Resolved horizontal-tb `direction:ltr|rtl` projects the logical candidates
  into the existing physical top/right/bottom/left style streams without
  changing semantic/source order.

## Boundary

- Only the six logical border-style declarations receive this priority
  behavior. Complete/side border shorthands, logical border width/color,
  physical border style/color, vertical writing modes, and other properties
  retain their current bounded behavior.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- A private doubled logical-style candidate array preserves important metadata
  and the `none`/`hidden` distinction until logical-to-physical projection; it
  adds bounded storage without exposing cascade state through public border
  artifacts.
- Projection explicitly carries the private important partition into the
  physical stream so direction changes do not collapse it back into normal
  layer ordering. This keeps the existing physical style owner and no-paint
  semantics while adding a small projection-specific merge path.
- Complete logical/physical border shorthands remain normal-only in this
  slice, keeping the change narrow while leaving their important semantics for
  a dedicated follow-up.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for terminal marker parsing,
  important-over-normal priority, reversed named layers, unlayered/inline
  important values, independent logical sides, CSS-wide forms, invalid-later
  preservation, private `none`/`hidden`, direction projection, and
  `revert-layer !important`.
- Run one focused integration fixture covering logical pair expansion,
  `ltr`/`rtl` projection, no-paint/paint artifacts, existing width/color
  composition, layout/display-list/raster/PNG consumers, point hit, semantic
  source order, and invalid diagnostics.
- Near issue completion run the full native integration and feature-enabled
  browser library, strict affected/workspace Clippy, warning-denied rustdoc,
  paired locked `glass-dev` checks/build, package, security/fuzz, repository
  static, workspace all-target/all-feature, and cleanup gates. Record exact
  counts and the remote-CI boundary before issue closure.

## Cleanup

Task-specific isolated targets and reports may be reclaimed only after all
Cargo/Rust processes and open handles exit. Only exact paths created for this
task may be removed; source, durable data, issue snapshots, and unrelated
workloads remain untouched. Record measured bytes/files, process/open-handle
checks, post-delete absence, and filesystem free-space delta.

## Evidence

Implementation and slice-local certification are pending.

