---
id: native-engine-219
scope: glass-browser/native-engine/local-place-content-inherit
status: complete
depends-on: [native-engine-218]
---

# Native explicit `place-content: inherit`

## Objective

Complete the bounded CSS-wide inheritance path for the existing
`place-content` shorthand. A standalone explicit `inherit` must project to
both existing `align-content` and `justify-content` owners, reusing their
private ancestor-style propagation without changing the shorthand's finite
value, reset, rollback, or cascade behavior.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-214.md`
- `docs/plan/tasks/native-engine-215.md`
- `docs/plan/tasks/native-engine-216.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `place-content` shorthand accepts standalone,
  case-insensitive `inherit` in addition to its existing finite one-/two-
  value forms, CSS-wide reset forms, and `revert-layer`.
- A winning `place-content: inherit` projects
  `AlignContentDeclaration::Inherit` and `JustifyContentDeclaration::Inherit`.
  Each component resolves from the already computed parent-style chain using
  the existing private `align-content` and `justify-content` owners. At the
  root, or when a direct unit computation has no supplied parent values, the
  existing bounded `flex-start` fallbacks remain the results for both axes.
- Omitted `place-content` remains local: it does not copy either parent value.
  Explicit `place-content: auto` remains invalid under the existing bounded
  grammar, while mixed forms such as `inherit center` remain invalid and do
  not replace a preceding valid candidate. Existing shorthand-to-longhand
  projection, important/source order, same-block longhand precedence,
  `revert-layer` rollback, finite axis values, wrapped-line distribution,
  main-axis placement, and all existing artifact consumers remain unchanged.
- The resolved values continue through the current flex layout, sizing,
  display-list, fixed-cell raster, PNG, point-hit, semantic, and diagnostic
  consumers. No public computed-style field, layout schema, dependency,
  feature default, or crate boundary is added.

## Boundary and tradeoffs

- Reuse the two completed component inheritance paths; do not add a separate
  shorthand-level inherited field or duplicate ancestor traversal. This keeps
  shorthand semantics aligned with longhand semantics and limits the change
  to parser/projection coverage.
- Explicit `inherit` copies computed parent values, not declarations, and does
  not make finite `place-content` values implicitly inherited. The bounded root
  fallback is deterministic but is not a claim of browser-wide CSS
  conformance.
- Percentages, baseline/safe/unsafe forms, `place-items`, grid, additional
  origins, transitions, animations, generic CSS-wide machinery, browser parity,
  security boundaries, and remote CI remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, both
  supplied parent values, root/default fallback, omitted-property local
  fallbacks, invalid mixed/later forms, `!important`, source order, finite
  one-/two-value expansion, and `revert-layer` preservation.
- Run one public wrapped-flex fixture through parent-to-child shorthand
  projection, both-axis placement, line distribution, main-axis placement,
  flex sizing, display-list, fixed-cell raster/PNG, point-hit, semantic
  consumers, and typed diagnostics for excluded mixed forms.
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

Implementation is `23703765`; the design checkpoint is `0b9e784b`. The locked
native-feature test-target check passed before tests. Focused `place-content`
parser/cascade coverage passed 5 tests, the public inheritance fixture passed
1 test, full native integration passed `257/257`, and the serial
native-feature library passed `1,040` tests with 1 ignored under
`RUST_MIN_STACK=8388608` and one test thread. The fixtures cover standalone
case-insensitive shorthand inheritance, both computed parent components,
omitted-property local fallbacks, mixed-invalid preservation,
important/source order, finite one-/two-value expansion, `revert-layer`,
wrapped-line distribution, main-axis placement, display-list, fixed-cell
raster/PNG, point-hit, semantic ordering, and typed unsupported-value
diagnostics. No public schema, dependency, feature default, or crate-boundary
change was made.

Workspace all-target/all-feature checking, strict two-crate Clippy with
warnings denied, paired browser/dev builds, warning-denied rustdoc for both
crates, formatting, and diff checks passed locally. Static audits also passed:
release truth `633 Markdown / 83 current / 59 previous-version / 784 semantic /
0 current-claim failures`, documentation coverage `633 Markdown / 345
full-product MCP tools / 100 browser-only / 17 examples / 22 public modules`,
documentation depth `93 current guides / 19 substantive contracts`, feature
parity `14 capabilities across 4 targets`, TUI inventory `15 implementation
keys / 63 documentation markers`, synchronized version `0.3.14`, reliability
`6 scenarios across 4 targets`, public read-only adapters `5`, Web IR `8
fixtures / 8 scenarios / 11 categories`, and knowledge migration v1
round-trip/v2 rejection over 6 records. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
