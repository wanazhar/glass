---
id: native-engine-182
scope: glass-browser/native-engine/cascade-physical-border-style-important-priority
status: in-progress
depends-on: [native-engine-181]
---

# Native bounded physical border-style `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
standalone physical `border-style` shorthand and its four physical style
longhands: `border-top-style`, `border-right-style`, `border-bottom-style`,
and `border-left-style`.

Style candidates retain the existing private per-side no-paint/paint values and
resolve before the unchanged width/color composition and
layout/display-list/raster/hit-test consumers. The slice adds only the private
important partition and reversed named-layer order for these five physical
style declarations.

## Context

Native-engine-171 established the bounded physical border-style value and
CSS-wide contract; native-engine-173 established complete physical border
shorthand projection into the width/style/color component streams; and
native-engine-181 established bounded important priority for physical border
width. Physical border-style currently still uses the normal 15-layer/unlayered
candidate array, so an important style declaration can lose its metadata before
style resolution.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-171.md`
- `docs/plan/tasks/native-engine-173.md`
- `docs/plan/tasks/native-engine-181.md`

## Contract

### Supported priority

- The standalone physical `border-style` shorthand and four physical style
  longhands accept the existing bounded finite style grammar, CSS-wide forms,
  and `revert-layer` followed by a terminal case-insensitive `!important`
  marker. The finite style values remain
  `none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset`.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing one-to-four-value physical expansion, per-side source/declaration
  ordering, no-paint sentinels, CSS-wide resolution, invalid-later
  preservation, zero-width/no-style fallback, and style-only no-paint behavior
  remain unchanged.

### Layer and source ordering

- Normal physical style declarations retain the existing 15-layer/unlayered
  order and selector-specificity/source/declaration/inline tie breakers.
- Important physical style declarations outrank normal physical style
  declarations. Inside the important partition, earliest named layers win,
  later named layers follow, and unlayered important declarations are lowest.
  Inline important values use the unlayered important bucket and retain inline
  precedence within that bucket.
- `revert-layer !important` rolls back the current physical style layer before
  lower important or normal candidates are resolved. Each physical side remains
  independent, including the private `none` and `hidden` distinctions.
- Complete physical `border`/`border-top`/`border-right`/`border-bottom`/
  `border-left` shorthands continue to project only normal style candidates in
  this slice; they do not receive important priority here.

### Boundary

- Only the standalone physical `border-style` shorthand and four physical
  style longhands receive this priority behavior. Complete/side border
  shorthands, logical border style, border width/color, vertical writing modes,
  layout algorithms, and other properties retain their current bounded
  behavior.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- A private doubled per-side style candidate array preserves important metadata
  until style resolution; it adds bounded storage without exposing cascade
  state through public border artifacts.
- Complete border shorthands continue to use the normal component path, which
  keeps this slice narrow but intentionally means `border:... !important` does
  not gain important style semantics until a dedicated follow-up.
- The existing centralized terminal-marker parser and per-rule declaration
  representation remain in place. No dependency, alternate rendering path, or
  public artifact change is introduced.

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
  important values, independent sides, CSS-wide forms, invalid-later
  preservation, private `none`/`hidden` behavior, and `revert-layer
  !important`.
- Run one focused integration fixture covering physical style expansion,
  existing width/color composition, no-paint and painted border artifacts,
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

Pending implementation and local certification.
