---
id: native-engine-208
scope: glass-browser/native-engine/local-flex-flow-css-wide-resets
status: complete
depends-on: [native-engine-207]
---

# Native flex-flow CSS-wide resets

## Objective

Extend the existing bounded local `flex-direction`, `flex-wrap`, and
`flex-flow` owners with standalone CSS-wide reset keywords without changing
line formation, direction mapping, shorthand projection, or the two-crate
boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-207.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `flex-direction` owner accepts standalone,
  case-insensitive `initial`, `unset`, and one-author-origin `revert`, in
  addition to its existing finite `row|row-reverse|column|column-reverse`
  values and `revert-layer`.
- The bounded local `flex-wrap` owner accepts the same standalone reset forms,
  in addition to finite `nowrap|wrap|wrap-reverse` and `revert-layer`.
- The bounded local `flex-flow` shorthand accepts the same standalone reset
  forms and projects them through the existing private component streams to
  `flex-direction:row` and `flex-wrap:nowrap`.
- A winning reset is terminal for its local declaration. Invalid later
  declarations preserve the preceding valid declaration; important/source
  order, finite shorthand expansion, component projection, and named-layer
  `revert-layer` remain unchanged.
- Resolved values continue through existing row/column direction mapping,
  wrapping and line formation, flex sizing/justification/alignment,
  display-list, fixed-cell raster, point-hit, semantic, and diagnostic
  consumers. No public computed-style field or layout schema is added.

## Boundary and tradeoffs

- Reuse the existing private direction/wrap candidate arrays and fallback
  resolvers. A reset-aware route is limited to these three local declarations;
  justify/alignment properties, `order`, inherited `direction`, and unrelated
  CSS parsers do not change behavior.
- Resetting to the existing local initial values keeps omitted declarations,
  `unset`, and local `revert` deterministic without introducing parent
  propagation. `inherit`, percentages, additional origins, transitions,
  animations, generic CSS-wide machinery, `wrap-reverse` expansion, and
  browser-wide Flexbox conformance remain outside this bounded contract.
- The shorthand reset is treated as one terminal two-component declaration,
  while ordinary finite `flex-flow` continues to use the existing direction /
  wrap projection. This preserves source-order behavior without rewriting the
  line-formation owner.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, `row` /
  `nowrap` fallbacks, shorthand projection, terminal reset behavior,
  invalid-later preservation, `!important`, source order, and
  `revert-layer`.
- Run one public fixture through row/column direction mapping, no-wrap/wrap
  line formation, display-list, fixed-cell raster/PNG, point-hit/semantic
  consumers, and typed unsupported-value diagnostics for excluded `inherit`
  and malformed shorthand values.
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

Implementation is `fcadc99c`; the docs-first design checkpoint is `e91a358b`.
The locked native-feature test-target check passed, focused flex parser/cascade
coverage passed 27 tests, and the public reset fixture passed. Full native
integration passed 246/246; the native-feature library passed 1,029 tests with
one ignored under `RUST_MIN_STACK=8388608` and one test thread. The default
parallel library run exposed a transient `Text file busy` temporary-script race
in the unrelated extension-host test; the named test passed in isolation and
the serial library passed. The workspace all-target/all-feature check, strict
Clippy, warning-denied rustdoc, both crate builds, workspace doctests, and
formatting/diff checks passed for both crates. The serial workspace
all-target/all-feature matrix passed with browser library 1,030 passed plus one
ignored, native integration 246, browser smoke 18, daemon recovery 1,
`glass-dev` 365, development runtime 4, and PTY 15; remaining targets reported
no failures.

The fixture covers case-insensitive reset parsing, finite `row`/`nowrap`
fallbacks, shorthand/component projection, important/source order,
invalid-later preservation, row/column mapping, wrapping, point-hit,
semantic/source order, display-list, fixed-cell raster/PNG, and typed
unsupported-value diagnostics for excluded `inherit`. No public schema,
dependency, feature default, or crate boundary changed. The six-target
nightly fuzz certification at 512 runs remains current from task 207 because
this slice changes only CSS flex parsing/cascade. Static documentation audits
passed: release truth reported 622 Markdown documents, 83 current-version
documents, 59 previous-version hits, 746 semantic hits, and zero current-claim
failures; coverage reported 345 full-product MCP tools (100 browser-only), 17
examples, and 22 public modules; depth reported 93 current guides and 19
substantive contracts; parity reported 14 capabilities across 4 targets; TUI
reported 15 implementation help keys and 63 documentation markers; reliability
reported 6 scenarios across 4 targets; read-only adapters reported 5; Web IR
reported 8 fixtures, 8 scenarios, and 11 categories; knowledge migration
certified the v1 round-trip/v2 rejection over 6 records; version remained
synchronized at `0.3.14`. Package and security evidence is retained from the
preceding dependency-stable task until the next issue-level validation
boundary.

The direct registry-backed dev verification remains blocked by the immutable
public `glass-browser 0.3.14` API surface; the canonical local patched/no-
verify package route passes. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary certification, and promotion
remain outside this local task.
