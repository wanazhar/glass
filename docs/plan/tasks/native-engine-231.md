---
id: native-engine-231
scope: glass-browser/native-engine/text-overflow-inherit
status: complete
depends-on: [native-engine-230]
---

# Native explicit `text-overflow: inherit`

## Objective

Extend the bounded single-line truncation owner with standalone,
case-insensitive `text-overflow: inherit`. An explicitly authored declaration
must copy the computed parent `clip|ellipsis` value through the existing
private ancestor-style chain while preserving omitted-property locality and
all existing truncation and artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-230.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `text-overflow` declaration accepts standalone,
  case-insensitive `inherit` in addition to its existing `clip|ellipsis` and
  `initial|unset|revert|revert-layer` forms.
- A winning `text-overflow: inherit` resolves the computed parent value. At
  the root, or when a direct unit computation has no supplied parent value,
  it uses the existing bounded `clip` fallback.
- Omitted `text-overflow` remains local and resolves to `clip`; explicit
  values, source order, specificity, `!important`, reset semantics,
  `revert-layer`, and invalid-later preservation remain unchanged. Mixed or
  token-bearing forms such as `inherit ellipsis` remain invalid and do not
  replace a preceding valid candidate.
- The resolved value continues through the existing eligible clipped-nowrap
  truncation, text-fragment, display-list, fixed-cell raster/PNG, overflow,
  point-hit, semantic/source-order, and typed-diagnostic consumers. No public
  computed-style field, dependency, feature default, layout schema, or crate
  boundary is added.

## Boundary and tradeoffs

- Reuse the private inherited truncation value and existing local candidate
  stream. Do not create a public inheritance API, duplicate ellipsis logic, or
  change the layout/artifact schemas.
- This slice covers only explicit `text-overflow` inheritance. Multi-line or
  nested-inline truncation, custom ellipsis strings, fade, percentages,
  additional origins, transitions, animations, generic CSS-wide machinery,
  browser parity, and browser-wide overflow conformance remain outside the
  boundary.
- Explicit inheritance copies the computed parent value rather than source
  declarations. Omission remains local by contract even though broader CSS
  inheritance behavior is outside this bounded native engine.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone inheritance,
  supplied parent/root fallback, omitted local fallback, finite values,
  mixed-invalid forms, `!important`, source order, reset behavior, and
  `revert-layer`.
- Run one public fixture through inherited clipped-nowrap truncation,
  non-truncating omission/reset controls, text fragments, display-list,
  fixed-cell raster/PNG, point-hit, semantic/source order, overflow behavior,
  and typed diagnostics for excluded forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation and local certification are complete at `9a9ef2c7` (design
`57e37dfe`). The focused parser/cascade group passes 3/3 tests, including
case-insensitive standalone inheritance, omitted-property non-inheritance,
mixed-invalid preservation, source order, important priority, and reset
fallbacks. The public inheritance fixture passes 1/1 and verifies inherited
clipped-nowrap truncation, non-truncating omission/invalid controls,
text-fragment output, display-list projection, fixed-cell raster/PNG, point
hit testing, semantic/source order, bounded overflow, and the typed diagnostic
for the excluded mixed form. Full native integration passes 269/269, and the
feature-enabled `glass-browser` library passes 1,048 tests with 1 ignored.
Static documentation checks also pass: release truth reports 645 Markdown
documents (83 current, 59 previous-version hits, 829 semantic audit hits, 0
current-claim failures); coverage reports 645 Markdown files, 345 full-product
MCP tools (100 browser-only), 17 examples, and 22 public modules; depth reports
93 current guides and 19 substantive contracts; feature parity reports 14
capabilities across 4 targets; TUI reports 15 implementation help keys and 63
documentation markers; and version sync reports `0.3.14`. Formatting and diff
checks pass. Issue-level strict lint, rustdoc, paired
two-crate, package, security/fuzz, static-documentation, workspace,
clean-install, remote-CI, issue-sync, and bounded-cleanup gates remain for
the broader issue completion boundary; remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
