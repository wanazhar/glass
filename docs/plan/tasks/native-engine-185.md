---
id: native-engine-185
scope: glass-browser/native-engine/cascade-physical-border-shorthand-important-priority
status: complete
depends-on: [native-engine-184]
---

# Native bounded physical border shorthand `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
complete physical `border` shorthand and the four physical side-border
shorthands: `border`, `border-top`, `border-right`, `border-bottom`, and
`border-left`.

The existing private width/style/color component streams remain the source of
computed border values. This slice carries the terminal importance marker from
each complete or side shorthand into all components projected by that
declaration, preserving the current width/style/color composition and public
border artifacts.

## Context

Native-engine-173 established the bounded complete physical and side-border
shorthand grammar, including concrete values, CSS-wide forms, omitted `none`
and `hidden`, `currentColor`, and `revert-layer`. Native-engine-179 through
182 established physical color, width, and style component `!important`
partitions, and native-engine-183/184 established the corresponding logical
width/style partitions. Complete and side physical border shorthands still
projected into normal component candidates only, so an important complete
declaration could lose to a later normal component or fail to outrank normal
complete declarations consistently.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-backgrounds-3/#border-width>
- <https://www.w3.org/TR/css-backgrounds-3/#border-style>
- <https://www.w3.org/TR/css-backgrounds-3/#border-color>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-173.md`
- `docs/plan/tasks/native-engine-179.md`
- `docs/plan/tasks/native-engine-180.md`
- `docs/plan/tasks/native-engine-181.md`
- `docs/plan/tasks/native-engine-182.md`
- `docs/plan/tasks/native-engine-184.md`

## Contract

### Supported priority

- The complete physical `border` shorthand and all four physical side-border
  shorthands accept the existing bounded complete-border grammar, CSS-wide
  forms, private omitted `none`/`hidden` forms, and `revert-layer` followed by
  a terminal case-insensitive `!important` marker.
- Important complete/side shorthands remain in the existing single author
  origin. User, user-agent, transition, animation, scoped-origin,
  custom-property, and general origin behavior remain outside the native
  contract.
- A valid complete or side shorthand projects its declared width, style, and
  color components with the same importance and declaration order. Existing
  component-specific declarations can still override a shorthand per side
  and per component according to the component's own importance and cascade
  order.
- Invalid later shorthand declarations do not erase an earlier valid
  shorthand or its importance bit. Inline complete/side shorthands retain the
  existing inline tie-break behavior.

### Layer, rollback, and composition ordering

- Normal shorthand declarations retain the existing 15-layer/unlayered order
  and selector-specificity/source/declaration/inline tie breakers.
- Important shorthand projections outrank normal projections. Inside the
  important partition, earliest named layers win, later named layers follow,
  and unlayered important declarations are lowest. Inline important values use
  the unlayered important bucket and retain inline precedence within it.
- `revert-layer !important` rolls back each projected width/style/color
  component in its current physical side before lower important or normal
  candidates are resolved. Side shorthands affect only their addressed side.
- Physical complete/side shorthand projections continue to compose with
  standalone physical and logical component declarations through the existing
  independent width/style/color streams. Private `none` and `hidden` style
  outcomes continue to suppress current non-table paint and retain their
  private distinction.
- The existing complete-border marker remains a normal local candidate used by
  the current public-side composition; projected width/style/color candidates
  carry the authoritative importance semantics for computed values and
  artifacts. No public marker or schema is added.

## Boundary

- Only the complete physical `border` and four physical side-border shorthands
  receive this priority behavior. Logical complete/side shorthands, standalone
  component declarations already covered by earlier slices, vertical writing
  modes, and other properties retain their current bounded behavior.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing fixed-cell, integer-pixel, local-resource, horizontal-tb,
  non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- A four-entry private importance array is the smallest state change that
  preserves shorthand importance through the already doubled physical
  component arrays. It avoids a second complete-border candidate matrix and
  does not expose cascade metadata through public values.
- Treating the component projections as the authority keeps width/style/color
  precedence independent, which is required when a side shorthand composes
  with a component longhand. The complete marker remains normal-only, so code
  that inspects private marker provenance is intentionally not broadened by
  this slice.
- Logical complete shorthands remain outside this slice because their
  direction projection and component importance transport need a separate
  contract; expanding them here would obscure physical precedence failures.
- The focused test surface will include concrete, CSS-wide, omitted, invalid,
  layer, rollback, side, inline, logical-component composition, layout,
  display-list, raster, hit-test, semantic-order, and public-surface checks;
  it will not claim arbitrary CSS border or browser conformance.

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
  important values, independent side shorthand projection, component
  composition, CSS-wide forms, omitted `none`/`hidden`, invalid-later
  preservation, and `revert-layer !important`.
- Run one focused integration fixture covering concrete and omitted complete/
  side shorthand artifacts, width/style/color composition, layout,
  display-list/raster/PNG consumers, point hit, semantic source order, and
  invalid diagnostics.
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

Implementation and slice-local certification are complete at `bfcb6dc9`.

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The locked native-feature check passed with
  `CARGO_TARGET_DIR=/tmp/glass-185-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused `physical_border_shorthand_important` coverage passed: 2 library
  unit tests and 1 integration test.
- The full `native_engine` integration target passed 223/223 tests.
- The feature-enabled `glass-browser` library target passed 1,000 tests with 1
  intentionally ignored.
- Strict all-target Clippy with `-D warnings` passed for `glass-browser` with
  `native-engine`.
- Warning-denied rustdoc passed for `glass-browser` with `native-engine`.
- The release documentation truth audit passed over 599 Markdown documents
  with 83 current documents, 57 previous-version hits, 693 semantic-audit
  hits, and 0 current-claim failures.
- Documentation depth passed with 93 current guides and 19 substantive
  contracts; the TUI shortcut inventory passed with 15 implementation help
  keys and 63 documentation markers.
- Issue-level workspace, release, security/fuzz, paired-crate, static
  documentation, and remote-CI gates remain deferred to the final issue #40
  certification boundary.

Task-specific cleanup completed after all Cargo/Rust processes and open handles
exited: `/tmp/glass-185-focused` measured 4,735,324,160 bytes across 9,117
files and 894 directories, and `/tmp/glass-release-documentation-185.json`
measured 192,512 bytes. The process and open-handle checks were empty.
Bounded exact-path deletion removed only those regenerable paths;
post-delete absence checks passed. Available filesystem bytes moved from
77,705,113,600 to 82,440,450,048, an observed increase of 4,735,336,448
bytes. Source, repository targets, durable data, unrelated workloads, and
issue snapshots were preserved.
