---
id: native-engine-183
scope: glass-browser/native-engine/cascade-logical-border-width-important-priority
status: in-progress
depends-on: [native-engine-182]
---

# Native bounded logical border-width `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the six
supported horizontal-tb logical border-width declarations:
`border-block-width`, `border-block-start-width`, `border-block-end-width`,
`border-inline-width`, `border-inline-start-width`, and
`border-inline-end-width`.

Logical width candidates retain the existing integer-pixel values, CSS-wide
forms, `revert-layer` behavior, and physical width/style/color consumers. The
slice adds only the private important partition and preserves its reversed
named-layer ordering while projecting logical candidates through resolved
`ltr`/`rtl` direction into physical width streams.

## Context

Native-engine-174 established the bounded horizontal-tb logical border
component families; native-engine-180 established the corresponding logical
border-color important partition; and native-engine-181 established bounded
important priority for physical border width. Logical border width still uses
the normal 15-layer/unlayered candidate array, so an important logical width
can lose its importance metadata before logical-to-physical projection.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-logical-1/#border-width>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-180.md`
- `docs/plan/tasks/native-engine-181.md`
- `docs/plan/tasks/native-engine-182.md`

## Contract

### Supported priority

- The six logical border-width declarations accept the existing bounded
  integer-pixel width grammar, CSS-wide forms, and `revert-layer` followed by
  a terminal case-insensitive `!important` marker.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing one- or two-value logical block/inline expansion, per-side
  source/declaration ordering, CSS-wide resolution, invalid-later
  preservation, zero fallback, and width/style/color composition remain
  unchanged.

### Layer and direction ordering

- Normal logical width declarations retain the existing 15-layer/unlayered
  order and selector-specificity/source/declaration/inline tie breakers.
- Important logical widths outrank normal logical widths. Inside the important
  partition, earliest named layers win, later named layers follow, and
  unlayered important declarations are lowest. Inline important values use the
  unlayered important bucket and retain inline precedence within that bucket.
- `revert-layer !important` rolls back the current logical width layer before
  lower important or normal candidates are resolved. Each logical side remains
  independent.
- Resolved horizontal-tb `direction:ltr|rtl` projects the logical candidates
  into the existing physical top/right/bottom/left width streams without
  changing semantic/source order.

## Boundary

- Only the six logical border-width declarations receive this priority
  behavior. Complete/side border shorthands, logical border style/color,
  physical border style/color, vertical writing modes, and other properties
  retain their current bounded behavior.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- A private doubled logical-width candidate array preserves important metadata
  until logical-to-physical projection; it adds bounded storage without
  exposing cascade state through public border artifacts.
- Projection explicitly carries the private important partition into the
  physical stream so direction changes do not collapse it back into normal
  layer ordering. This keeps the existing physical component owner but adds a
  small projection-specific merge path.
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
  preservation, direction projection, and `revert-layer !important`.
- Run one focused integration fixture covering logical pair expansion,
  `ltr`/`rtl` projection, existing width/style/color composition,
  layout/display-list/raster/PNG consumers, point hit, semantic/source order,
  and invalid diagnostics.
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

