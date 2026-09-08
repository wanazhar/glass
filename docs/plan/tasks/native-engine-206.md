---
id: native-engine-206
scope: glass-browser/native-engine/local-text-indent-overflow-css-wide-resets
status: complete
depends-on: [native-engine-205]
---

# Native local text-indent and text-overflow CSS-wide resets

## Objective

Extend the existing bounded local `text-indent` and `text-overflow` owners
with standalone CSS-wide reset keywords without changing first-line geometry,
overflow state, display artifacts, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-205.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `text-indent` owner accepts standalone, case-insensitive
  `initial`, `unset`, and one-author-origin `revert`, in addition to its finite
  non-negative pixel values and `revert-layer`.
- The bounded local `text-overflow` owner accepts standalone, case-insensitive
  `initial`, `unset`, and one-author-origin `revert`, in addition to its finite
  `clip|ellipsis` values and `revert-layer`.
- All three reset forms are terminal for the winning local declaration:
  `text-indent` resolves to the existing `0px` fallback and `text-overflow`
  resolves to the existing `clip` fallback. Invalid later declarations
  preserve the preceding valid declaration; important/source-order and
  named-layer rollback remain unchanged.
- `text-indent` continues through first-line layout, display-list, fixed-cell
  raster, point-hit, and semantic consumers. `text-overflow` continues through
  its existing computed-style owner without introducing ellipsis layout or
  glyph truncation.
- `inherit`, parent propagation, percentages, negative/fractional lengths,
  additional origins, generic CSS-wide machinery, and browser-wide overflow
  conformance remain outside this local bounded contract.

## Boundary and tradeoffs

- Add a private reset-aware parser route for these two declarations rather than
  widening every local declaration parser. Existing border, background,
  display, opacity, visibility, dimension, padding, margin, and box-sizing
  contracts remain unchanged.
- Reuse the existing `LocalCascadeDeclaration::Reset` fallback path. No public
  computed-style field, display-command field, dependency, or crate is
  introduced.
- Keeping `inherit` unsupported avoids implying parent propagation for these
  local owners; callers receive the existing typed unsupported-value diagnostic.
  The native engine does not claim browser-wide text-overflow truncation.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, finite local
  fallbacks, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and `revert-layer`.
- Run one public fixture through text-indent layout, display-list, fixed-cell
  raster/PNG, point-hit/semantic consumers, and typed unsupported-value
  diagnostics for excluded `inherit`/unsupported values.
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

Implementation is `b7bd9ace`; the docs-first design checkpoint is `82c31c9a`.
The locked native-feature check, focused parser/cascade coverage (3 unit tests),
and public integration fixture (1 test) passed. Full native integration passed
244/244 and the native-feature library passed 1,026 tests with 1 ignored test.
Paired browser/dev builds, strict Clippy, warning-denied rustdoc, workspace
all-target/all-feature checking, formatting, six nightly fuzz targets at 512
runs each, cargo-deny, cargo-audit, package assembly, and the exact packaged
dependency check passed locally. The workspace all-target/all-feature matrix
passed with one test thread: browser library 1,027 passed with 1 ignored;
native integration 244 passed; browser smoke 18 passed; daemon recovery 1
passed; `glass-dev` 365 passed; development runtime 4 passed; PTY 15 passed;
remaining targets reported no failures. The first default-parallel matrix run
hit an existing process-ID temporary-directory teardown collision in the
unrelated `glass-dev` `composer_slash_compact_routes_to_native_pi_command`
test; the named test passed in isolation and the serial matrix passed.
Workspace doctests passed: 4 browser and 1 dev.

Packages were 196 files / 6.0 MiB for `glass-browser` and 69 files / 2.6 MiB
for `glass-dev`; the packaged dev archive resolves `glass-browser` exactly at
`0.3.14`. Direct registry-backed dev verification remains blocked by the
immutable public `glass-browser 0.3.14` API surface; the canonical local
patched/no-verify package route passes.

Static documentation audits passed: release truth reported 620 Markdown
documents, 83 current-version documents, 59 previous-version hits, 740
semantic hits, and zero current-claim failures; coverage reported 345
full-product MCP tools (100 browser-only), 17 examples, and 22 public modules;
depth reported 93 current guides and 19 substantive contracts; parity reported
14 capabilities across 4 targets; TUI reported 15 implementation help keys and
63 documentation markers; reliability reported 6 scenarios across 4 targets;
read-only adapters reported 5; Web IR reported 8 fixtures, 8 scenarios, and 11
categories; knowledge migration certified the v1 round-trip/v2 rejection over 6
records; version remained synchronized at `0.3.14`. The preceding clean-install
transition gate remains the latest install evidence because this slice changed
only native CSS parsing/cascade and synchronized documentation, not package
metadata or dependencies.

Only exact task-specific regenerable targets and reports remain for cleanup
after Cargo/Rust process and open-handle checks; source, durable data,
repository targets, issue snapshots, and unrelated workloads remain untouched.
Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary certification, and promotion remain outside this local task.
