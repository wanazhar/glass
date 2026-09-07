---
id: native-engine-186
scope: glass-browser/native-engine/cascade-logical-border-shorthand-important-priority
status: in-progress
depends-on: [native-engine-185]
---

# Native bounded logical border shorthand `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the six
supported horizontal-tb logical complete/side border shorthands:
`border-block`, `border-block-start`, `border-block-end`, `border-inline`,
`border-inline-start`, and `border-inline-end`.

The existing logical-to-physical direction projection and private physical
width/style/color component streams remain the owners of computed values and
artifacts. This slice carries the terminal importance marker through logical
component candidates before resolved `ltr`/`rtl` projection.

## Context

Native-engine-174 established bounded horizontal-tb logical border shorthands
and their width/style/color component families. Native-engine-180, 183, and
184 established important partitions for logical color, width, and style
component declarations. Native-engine-185 established the same priority for
complete physical and physical side-border shorthands. Logical complete/side
shorthands still enter the logical component streams as normal candidates, so
they cannot consistently outrank normal logical components or survive
direction projection with important metadata.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-logical-1/#border-shorthands>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-180.md`
- `docs/plan/tasks/native-engine-183.md`
- `docs/plan/tasks/native-engine-184.md`
- `docs/plan/tasks/native-engine-185.md`

## Contract

### Supported priority

- The six logical complete/side shorthands accept the existing bounded
  `border` grammar, CSS-wide forms, private omitted `none`/`hidden` forms,
  and `revert-layer` followed by a terminal case-insensitive `!important`
  marker.
- Important logical shorthand projections remain in the existing single
  author origin. User, user-agent, transition, animation, scoped-origin,
  custom-property, and general origin behavior remain outside the native
  contract.
- A valid logical complete/side shorthand projects its declared width, style,
  and color components with the same importance and declaration order. Existing
  logical component declarations can still override a shorthand per logical
  side and component according to their own importance and cascade order.
- Invalid later shorthand declarations do not erase an earlier valid logical
  shorthand or its importance bit. Inline logical shorthands retain existing
  inline precedence.

### Layer, direction, rollback, and composition ordering

- Normal logical shorthand declarations retain the existing 15-layer/unlayered
  order and selector-specificity/source/declaration/inline tie breakers.
- Important logical shorthand projections outrank normal projections. Inside
  the important partition, earliest named layers win, later named layers
  follow, and unlayered important declarations are lowest. Inline important
  values use the unlayered important bucket and retain inline precedence within
  it.
- `revert-layer !important` rolls back each logical projected width/style/color
  component before lower important or normal candidates are resolved. Pair
  shorthands remain independently addressable per logical side.
- Resolved horizontal-tb `direction:ltr|rtl` carries the private important
  partition into the existing physical top/right/bottom/left streams. Block
  sides map to top/bottom; inline start/end map to left/right for `ltr` and
  right/left for `rtl`.
- Existing private `none`/`hidden` style outcomes continue to suppress current
  non-table paint while retaining their private distinction. Width/style/color
  components continue to compose only after independent resolution.

## Boundary

- Only the six logical complete/side shorthands receive this priority behavior.
  Vertical writing modes, logical radius, other properties, and browser-wide
  logical-border conformance remain outside the native contract.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- One private importance bit per logical side is sufficient because the
  existing logical width/style/color candidate arrays already preserve the
  doubled important partition. No second complete logical candidate matrix or
  public provenance field is needed.
- The complete logical marker remains a normal local candidate while its
  projected component values carry authoritative importance, matching the
  physical shorthand strategy and preserving independent component composition.
- Applying complete logical projections before direction mapping keeps the
  importance metadata local and makes `ltr`/`rtl` transport explicit. It adds a
  bounded projection step but avoids changing the physical consumers.
- Vertical writing modes and logical complete shorthand semantics beyond the
  six horizontal-tb declarations remain excluded so direction mapping does not
  silently imply full CSS logical-property support.

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
  important values, logical pair/side expansion, invalid-later preservation,
  CSS-wide and omitted `none`/`hidden` forms, component composition,
  direction projection, and `revert-layer !important`.
- Run one focused integration fixture covering logical pair/side shorthand
  projection in `ltr` and `rtl`, no-paint/paint artifacts, component
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

Implementation, certification, documentation synchronization, and cleanup
evidence will be recorded here when the slice is complete.
