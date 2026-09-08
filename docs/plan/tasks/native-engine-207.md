---
id: native-engine-207
scope: glass-browser/native-engine/flex-sizing-css-wide-resets
status: complete
depends-on: [native-engine-206]
---

# Native flex sizing CSS-wide resets

## Objective

Extend the existing bounded flex sizing owners with standalone CSS-wide reset
keywords without changing flex placement, shorthand expansion, computed public
schemas, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-206.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `flex-grow`, `flex-shrink`, and `flex-basis` owners accept
  standalone, case-insensitive `initial`, `unset`, and one-author-origin
  `revert`, in addition to their existing finite values and `revert-layer`.
- The bounded local `flex` shorthand accepts the same standalone reset forms.
  Each reset expands through the existing private component streams to the
  finite initial tuple `flex-grow:0`, `flex-shrink:1`, and `flex-basis:auto`.
- Reset forms are terminal for the winning local declaration. Invalid later
  declarations preserve the preceding valid declaration; important/source
  order, shorthand/longhand projection, and named-layer `revert-layer` remain
  unchanged.
- Resolved values continue through the existing flex row placement, free-space
  allocation, display-list, fixed-cell raster, point-hit, semantic, and
  diagnostic consumers. No public computed-style field or layout schema is
  added.

## Boundary and tradeoffs

- Reuse the existing private flex component candidate arrays and fallback
  resolvers. A reset-aware route is limited to the flex sizing declarations so
  unrelated direction, wrap, alignment, order, gap, and local CSS parsers do
  not change behavior.
- `inherit`, parent propagation, percentages, negative or fractional values,
  additional origins, transitions, animations, generic CSS-wide machinery,
  flex-basis intrinsic sizing, and browser-wide flex conformance remain outside
  this bounded contract. The existing finite non-negative fixed-pixel basis
  grammar and `auto` value remain the only accepted basis forms.
- `flex-direction`, `flex-wrap`, `flex-flow`, `place-content`, alignment
  declarations, and `order` are intentionally excluded so this slice remains
  a component-expansion/cascade unit rather than a second flex-layout rewrite.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, finite initial
  fallbacks, shorthand expansion, terminal reset behavior, invalid-later
  preservation, `!important`, source order, and `revert-layer`.
- Run one public fixture through flex sizing/reset projection, row placement,
  free-space allocation, display-list, fixed-cell raster/PNG, point-hit,
  semantic consumers, and typed diagnostics for excluded values.
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

Implementation is `1a43c150`; the docs-first design checkpoint is `6d98fb3f`.
The locked native-feature check passed, focused flex parser/cascade coverage
passed 25 tests, and the public reset fixture passed. Full native integration
passed 245/245; the native-feature library passed 1,027 tests with one ignored.
The default-stack library run reproduced the known pre-existing
`cli::args::tests::agent_readiness_commands_are_explicit` large-Clap stack
overflow; rerunning the same library suite with
`RUST_MIN_STACK=8388608` passed. Paired browser/dev builds, strict Clippy,
warning-denied rustdoc, the workspace all-target/all-feature check, and the
serial workspace all-target/all-feature matrix passed. The matrix reported
browser library 1,028 passed plus one ignored, native integration 245, smoke
18, daemon recovery 1, glass-dev 365, development runtime 4, and PTY 15.
Workspace doctests passed (4 browser, 1 dev). Package assembly produced the
196-file 6.0 MiB `glass-browser` archive and 69-file 2.6 MiB `glass-dev`
archive; the exact packaged dependency check passed. `cargo deny check`,
`cargo audit`, formatting, and diff checks passed. Audit output retained the
known allowed warnings for unmaintained `bincode` and `yaml-rust`, the current
`lru` advisory, and yanked `chacha20`; no new task-specific finding was
introduced. The six-target nightly fuzz certification at 512 runs remains
current from task 206 because this slice changes only CSS flex
parsing/cascade and does not touch fuzz harnesses. Static documentation and
knowledge-migration gates were rerun during the closeout.

The existing parallel workspace test teardown collision in an unrelated
`glass-dev` temporary-directory test remains documented; the serial matrix
passed and the isolated test passed independently. The direct registry-backed
dev verification remains blocked by the immutable public `glass-browser
0.3.14` API surface; the canonical local patched/no-verify package route
passes. Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary certification, and promotion remain outside this local
task.
