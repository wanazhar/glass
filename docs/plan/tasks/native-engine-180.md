---
id: native-engine-180
scope: glass-browser/native-engine/cascade-logical-border-color-important-priority
status: complete
depends-on: [native-engine-179]
---

# Native bounded logical border-color `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract from physical
border-color to the existing horizontal-tb logical border-color family:
`border-block-color`, `border-block-start-color`,
`border-block-end-color`, `border-inline-color`,
`border-inline-start-color`, and `border-inline-end-color`.

Logical candidates retain their existing private direction projection into the
physical top/right/bottom/left border-color streams. The slice adds only the
private important partition and reversed named-layer order; value grammar,
four-side composition, `currentColor`, CSS-wide values, `revert-layer`, border
width/style interaction, layout, display-list, capture, raster, clipping,
point-hit, and semantic consumers remain unchanged.

## Context

Native-engine-174 established the bounded horizontal-tb logical border family
and its `ltr`/`rtl` projection; native-engine-179 established important
priority for standalone physical border-color declarations. Logical
border-color currently still uses the normal 15-layer candidate array, and
projection would otherwise discard an important layer.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-logical-1/#border-color>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-179.md`

## Contract

### Supported priority

- The six supported logical border-color declarations accept the existing
  literal/alpha, `currentColor`, CSS-wide, and `revert-layer` forms followed
  by a terminal case-insensitive `!important` marker.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing logical parsing, two-value pair expansion, invalid-later
  preservation, and physical side resolution remain unchanged.

### Layer, source, and direction ordering

- Normal logical declarations retain the existing 15-layer order and
  selector-specificity/source/declaration/inline tie breakers.
- Important logical declarations outrank normal logical border-color
  declarations. Inside the important partition, earliest named layers win,
  later named layers follow, and unlayered important declarations are lowest.
  Inline important values use the unlayered important bucket and retain inline
  precedence.
- `revert-layer !important` rolls back the current logical important layer
  before the candidate is projected to its resolved physical side(s).
- Projection uses the existing resolved horizontal-tb `direction`: block
  start/end map to top/bottom, and inline start/end map to left/right for
  `ltr` and right/left for `rtl`. Each physical side remains independent.

### Boundary

- Only standalone logical border-color declarations receive this priority
  behavior. Complete `border`/physical side-border shorthands, physical
  border-color (already covered by 179), border width/style, vertical writing
  modes, and other properties retain their current bounded behavior.
- Existing normal candidates projected from logical complete border shorthands
  remain normal candidates; this slice does not claim important priority for
  those source declarations.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- A private doubled logical color candidate array preserves important metadata
  until direction projection; the bounded storage is small and avoids leaking
  cascade state into public border artifacts.
- A dedicated logical-color projection path keeps normal width/style/border
  projection unchanged. The tradeoff is that logical border width/style and
  complete logical border shorthands remain explicit follow-up boundaries.
- The existing centralized terminal marker parser and per-rule declaration
  representation remain in place; no dependency or second rendering path is
  introduced.

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
  important values, logical pair expansion, `ltr`/`rtl` projection,
  independent sides, and `revert-layer !important`.
- Run one focused integration fixture covering logical border commands after
  direction projection, width/style composition, layout/display-list/raster/
  PNG consumers, point hit, semantic/source order, and invalid diagnostics.
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

Implementation and slice-local certification are complete at `491f65fe`; the
strict-lint follow-up is `100d1888`.

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The locked native-feature check passed with
  `CARGO_TARGET_DIR=/tmp/glass-180-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused `logical_border_color_important` coverage passed: 2 library unit
  tests and 1 integration test.
- The full `native_engine` integration target passed 218/218 tests.
- The feature-enabled `glass-browser` library target passed 990 tests with 1
  intentionally ignored.
- Strict all-target Clippy with `-D warnings` and warning-denied rustdoc passed
  for `glass-browser` with `native-engine`.
- The final documentation truth audit passed over 594 Markdown documents with
  83 current documents, 57 previous-version hits, 688 semantic-audit hits, and
  0 current-claim failures.
- Issue-level workspace, release, security/fuzz, paired-crate, cleanup, and
  remote-CI gates remain deferred to the final issue #40 certification
  boundary.

Task-specific cleanup was completed after all local Cargo/Rust processes and
open handles exited: `/tmp/glass-180-focused` measured 4,245,769,887 bytes
across 7,337 files and 846 directories, and the nine named reports measured
191,794 bytes. Bounded exact-path deletion removed only those paths;
post-delete absence checks passed. Available filesystem bytes moved from
78,195,785,728 to 82,462,998,528, an observed increase of 4,267,212,800
bytes. Source, repository targets, durable data, unrelated workloads, and
issue snapshots were preserved.
