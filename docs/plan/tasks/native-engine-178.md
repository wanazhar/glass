---
id: native-engine-178
scope: glass-browser/native-engine/cascade-paint-color-important-priority
status: in-progress
depends-on: [native-engine-177]
---

# Native bounded paint-color `!important` cascade priority

## Objective

Extends the bounded author-origin `!important` cascade contract from the
completed radius family to the three local paint-color owners:
`background-color`, inherited `color`, and `text-decoration-color`. Their
existing value grammar, inheritance/defaulting rules, and fill/glyph/
decoration consumers remain unchanged; only the private cascade priority gains
the important partition and reversed named-layer order.

This is the next smallest paint-facing priority slice. It makes important
color declarations useful without claiming that every supported CSS property
already implements generic importance or changing the public artifact
schemas.

## Context

Native-engine-163 through native-engine-168 established the bounded color and
decoration-color owners, including `currentColor`, CSS-wide values, and local
`revert-layer`. Native-engine-177 established the private doubled candidate
partition for the radius family. The current parser strips `!important` from
all bounded declarations, but only radius uses the marker to affect
precedence.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-163.md`
- `docs/plan/tasks/native-engine-166.md`
- `docs/plan/tasks/native-engine-168.md`
- `docs/plan/tasks/native-engine-177.md`

## Contract

### Supported priority

- `background-color`, `color`, and `text-decoration-color` accept the
  existing bounded value or standalone CSS-wide/`revert-layer` form followed
  by a terminal case-insensitive `!important` marker.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing `currentColor`, inherited color, optional background fallback,
  decoration fallback, and invalid-later preservation remain unchanged.

### Layer and source ordering

- Normal declarations retain the existing order: unlayered author
  declarations outrank later named layers, which outrank earlier named
  layers; selector specificity, rule order, declaration order, and inline
  precedence resolve ties within a layer.
- Important declarations outrank normal declarations. Within the important
  partition, named-layer order is reversed: the earliest named layer wins,
  later named layers follow, and unlayered important declarations are the
  lowest important bucket. Inline important values use the unlayered
  important bucket and retain inline precedence within that bucket.
- `revert-layer !important` rolls back the current important layer and then
  exposes lower-priority important or normal candidates. Normal
  `revert-layer` retains its current behavior.
- Each owner resolves independently. A background, glyph, or decoration
  candidate cannot alter another owner's cascade, inheritance, or fallback.

### Boundary

- Only the three named paint-color owners receive this priority behavior.
  Border color, border width/style, layout, spacing, flex, text presentation,
  overflow, and other properties retain their current bounded behavior.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- Three private doubled candidate arrays duplicate a small amount of cascade
  storage, but keep importance isolated from the many properties that do not
  yet have a complete priority model.
- A shared paint-priority helper keeps layer inversion and inline handling
  identical across background, inherited glyph, and decoration colors; the
  tradeoff is that border color remains a separate follow-up rather than
  silently receiving partial semantics.
- The parser's terminal marker handling stays centralized and
  case-insensitive. No dependency, public field, artifact shape, or second
  paint path is needed.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for terminal marker parsing,
  normal-versus-important precedence, named-layer inversion, unlayered and
  inline important values, independent owner fallback, and
  `revert-layer !important`.
- Run one focused integration fixture covering background fill, glyph color,
  decoration color, layout/display-list/raster/PNG consumers, semantic/source
  order, and invalid diagnostics.
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
