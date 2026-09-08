---
id: native-engine-222
scope: glass-browser/native-engine/flex-flow-inherit
status: complete
depends-on: [native-engine-221]
---

# Native explicit `flex-flow: inherit`

## Objective

Complete the bounded CSS-wide inheritance path for the existing `flex-flow`
shorthand. A standalone explicit `inherit` must project to the already
completed `flex-direction` and `flex-wrap` inheritance owners, copying both
computed parent components while preserving finite shorthand expansion,
longhand precedence, rollback, and all current flex layout/artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-208.md`
- `docs/plan/tasks/native-engine-220.md`
- `docs/plan/tasks/native-engine-221.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `flex-flow` shorthand accepts standalone, case-insensitive
  `inherit` in addition to its existing finite one-/two-value forms,
  CSS-wide reset forms, and `revert-layer`.
- A winning `flex-flow: inherit` projects
  `FlexDirectionDeclaration::Inherit` and `FlexWrapDeclaration::Inherit`.
  Each component resolves from the already computed parent-style chain using
  the existing private direction and wrap owners. At the root, or when a
  direct unit computation has no supplied parent values, the existing bounded
  `row` and `nowrap` fallbacks remain the results.
- Omitted `flex-flow` remains local. Finite one-token and two-token shorthand
  forms continue to reset/project their omitted component as today. Mixed
  forms such as `inherit row`, `row inherit`, and `inherit wrap` remain invalid
  and do not replace a preceding valid candidate. Existing longhand/shorthand
  source order, same-block component precedence, important/source order,
  `revert-layer` rollback, and independent direction/wrap ownership remain
  unchanged.
- Both resolved components continue through row/column main-axis mapping,
  wrapping eligibility, line formation and stacking, line sizing, gap and
  margin mapping, flex sizing, complete-subtree movement, display-list,
  fixed-cell raster, PNG, point-hit, semantic/source order, and diagnostics.
  No public computed-style field, layout schema, dependency, feature default,
  or crate boundary is added.

## Boundary and tradeoffs

- Reuse the two completed longhand inheritance paths and the existing shorthand
  projection. Do not add a separate shorthand-level inherited field or a
  second ancestor traversal.
- Explicit `inherit` copies computed component values, not declarations, and
  does not make finite `flex-flow` values implicitly inherited. The bounded
  root fallback is deterministic but is not a claim of browser-wide CSS
  conformance.
- Percentages, intrinsic sizing, additional origins, transitions, animations,
  generic CSS-wide machinery, writing-mode, grid, browser parity, security
  boundaries, and remote CI remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, both
  supplied parent components, root/default fallbacks, omitted local fallback,
  finite one-/two-value projection, invalid mixed/later forms, `!important`,
  source order, longhand precedence, and `revert-layer` preservation.
- Run one public nested flex fixture through shorthand inheritance for both
  direction and wrap, row/column mapping, wrapping and line stacking, gap and
  margin mapping, flex sizing, complete-subtree movement, display-list,
  fixed-cell raster/PNG, point-hit, semantic/source order, and typed
  diagnostics for excluded mixed forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation is `87465d23` and the design checkpoint is `cae84efb`.
The locked scoped native-feature check passed before tests. Focused `flex-flow`
parser/cascade coverage passed 6/6, the public two-component inheritance
fixture passed 1/1, full native integration passed 260/260, and the
feature-enabled `glass-browser` library passed 1,043 tests with 1 ignored
(including the isolated subprocess test). Formatting, workspace all-target/
all-feature check, strict Clippy, paired `glass-browser` / `glass-dev` builds,
and warning-denied rustdoc all passed locally.

The fixture covers standalone shorthand inheritance for direction and wrap,
column main-axis mapping, wrap-reverse line formation, local/root fallbacks,
invalid mixed-form preservation, longhand/component precedence, complete
source/semantic ordering, display-list, fixed-cell raster/PNG, point-hit, and
typed diagnostics. The pre-existing invalid-later fixtures were updated from
standalone `flex-flow: inherit` to the still-invalid mixed form `inherit row`
because standalone inheritance is now supported. No public schema, dependency,
feature default, or crate-boundary change was made.

Static closeout audits also passed: release truth `636 Markdown / 83 current /
59 previous-version / 793 semantic / 0 current-claim failures`, documentation
coverage `636 Markdown / 345 full-product MCP tools / 100 browser-only / 17
examples / 22 public modules`, documentation depth `93 current guides / 19
substantive contracts`, feature parity `14 capabilities across 4 targets`, TUI
inventory `15 implementation keys / 63 documentation markers`, synchronized
version `0.3.14`, reliability `6 scenarios across 4 targets`, public read-only
adapters `5`, Web IR `8 fixtures / 8 scenarios / 11 categories`, and knowledge
migration v1 round-trip/v2 rejection over 6 records.

Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary certification, and promotion remain outside this local task.
