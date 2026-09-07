---
id: native-engine-179
scope: glass-browser/native-engine/cascade-physical-border-color-important-priority
status: complete
depends-on: [native-engine-178]
---

# Native bounded physical border-color `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract from the
background, glyph, and decoration paint-color owners to the existing physical
border-color owner: the one-to-four-value `border-color` shorthand and the
`border-top-color`, `border-right-color`, `border-bottom-color`, and
`border-left-color` longhands.

The slice preserves the existing literal/alpha, `currentColor`, CSS-wide,
`revert-layer`, four-side composition, width/style interaction, layout,
display-list, capture, raster, clipping, point-hit, and semantic consumers.
Only the private cascade priority gains an important partition and reversed
named-layer order.

## Context

Native-engine-169 established the bounded physical border-color owner,
including CSS-wide values and local `revert-layer`; native-engine-177 and
native-engine-178 established the bounded author-origin important partition for
radius and the three non-border paint-color owners. Physical border-color
currently still resolves every declaration through the normal 15-layer
candidate array.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-backgrounds-3/#border-color>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-169.md`
- `docs/plan/tasks/native-engine-178.md`

## Contract

### Supported priority

- `border-color` and each physical color longhand accept the existing bounded
  value or standalone CSS-wide/`revert-layer` form followed by a terminal
  case-insensitive `!important` marker.
- Important declarations remain in the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, custom-property, and
  general cascade-origin behavior remain outside the native contract.
- Existing literal/alpha colors, `currentColor`, `inherit`, `unset`,
  `initial`, `revert`, local `revert-layer`, omitted black fallback, and
  valid-before-invalid preservation remain unchanged.

### Layer and source ordering

- Normal declarations retain the existing order: unlayered author
  declarations outrank later named layers, which outrank earlier named
  layers; selector specificity, source order, declaration order, and inline
  precedence resolve ties within a side and layer.
- Important physical border-color declarations outrank normal border-color
  declarations. Within the important partition, named-layer order is reversed:
  the earliest named layer wins, later named layers follow, and unlayered
  important declarations are the lowest important bucket. Inline important
  values use the unlayered important bucket and retain inline precedence.
- `revert-layer !important` rolls back the current important layer and exposes
  lower-priority important or normal physical border-color candidates. Normal
  `revert-layer` retains its current behavior.
- The four physical sides resolve independently. A top/right/bottom/left
  candidate cannot alter another side's color, width, style, inheritance, or
  fallback.

### Boundary

- Only standalone physical border-color declarations receive this priority
  behavior. Complete `border`/physical side-border shorthands, logical
  border-color declarations, border width/style, and other properties retain
  their current bounded priority behavior.
- Existing normal candidates projected from complete or physical side-border
  shorthands remain normal candidates; this slice does not claim important
  priority for those source declarations.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- Four private doubled side-candidate arrays add a small bounded amount of
  cascade storage while keeping importance isolated from border width/style
  and unsupported logical declarations.
- Reusing the radius/paint layer inversion rules keeps important ordering
  consistent; the tradeoff is an explicit follow-up for logical border-color
  rather than silently losing important metadata during logical-to-physical
  projection.
- The parser's centralized terminal marker handling stays case-insensitive and
  invalid-later preservation remains property-local. No dependency, public
  field, artifact shape, or second border paint path is needed.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for terminal marker parsing,
  important-over-normal precedence, earliest named-layer ordering, unlayered
  and inline important values, independent sides, invalid-later preservation,
  and `revert-layer !important`.
- Run one focused integration fixture covering four-side border commands,
  width/style composition, layout/display-list/raster/PNG consumers, point
  hit, semantic/source order, and invalid diagnostics.
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

Implementation and slice-local certification are complete at `ed1cda27`.

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The locked native-feature check passed with `CARGO_TARGET_DIR=/tmp/glass-179-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused `physical_border_color_important` coverage passed: 2 library unit tests and 1 integration test.
- The full `native_engine` integration target passed 217/217 tests.
- The feature-enabled `glass-browser` library target passed 988 tests with 1 intentionally ignored.
- Strict all-target Clippy with `-D warnings` and warning-denied rustdoc passed for `glass-browser` with `native-engine`.
- The final documentation truth audit passed over 593 Markdown documents with 83 current documents, 57 previous-version hits, 688 semantic-audit hits, and 0 current-claim failures.
- Issue-level workspace, release, security/fuzz, paired-crate, cleanup, and remote-CI gates remain deferred to the final issue #40 certification boundary.

Task-specific cleanup was completed after all local Cargo/Rust processes and
open handles exited: `/tmp/glass-179-focused` measured 4,621,983,231 bytes
across 9,117 files and 894 directories, and the nine named reports measured
193,105 bytes. Bounded exact-path deletion removed only those paths; post-delete
absence checks passed. Available filesystem bytes moved from 77,825,740,800 to
82,471,096,320, an observed increase of 4,645,355,520 bytes. Source,
repository targets, durable data, unrelated workloads, and issue snapshots
were preserved.
