---
id: native-engine-198
scope: glass-browser/native-engine/inherited-text-css-wide-resets
status: planned
depends-on: [native-engine-197]
---

# Native inherited text CSS-wide resets

## Objective

Extend the existing bounded inherited fixed-cell text owners with standalone
CSS-wide keywords without creating a generic CSS cascade engine or changing
public computed-style/artifact schemas.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-197.md`

## Contract

- The inherited `white-space`, positive-pixel `line-height`, `text-transform`,
  `font-weight`, `font-style`, `word-break`, `vertical-align`, `word-spacing`,
  and `letter-spacing` declarations accept standalone, case-insensitive
  `inherit`, `initial`, `unset`, and one-author-origin `revert`, in addition to
  the already supported `revert-layer`.
- `inherit` and `unset` resolve to the computed parent value. In this
  one-author-origin engine, `revert` has the same parent fallback because no
  lower author declaration is present; `revert-layer` remains the only form
  that walks lower named-layer candidates.
- `initial` resolves to the existing finite root defaults: `normal` whitespace,
  absent/auto line height, `none` transform, normal weight/style, normal word
  break, baseline vertical alignment, and zero word/letter spacing.
- A winning CSS-wide keyword is terminal for its property and must not expose a
  lower candidate. Invalid later declarations preserve the preceding valid
  declaration; existing specificity, source order, inline-important, and
  important-over-normal behavior remain unchanged.
- Mixed reset tokens, lengths with keywords, unsupported values, additional
  origins, and generic CSS-wide machinery remain rejected or outside the
  bounded contract.

## Boundary and tradeoffs

- The private inherited declaration enum records the five standalone keyword
  forms but resolves them through the existing per-property fallback owner.
  This keeps the public finite enums, fixed-cell layout, display-list, raster,
  capture, point-hit, semantic/source-order, and two-crate boundaries stable.
- `revert` intentionally follows the current parent fallback rather than
  modeling user-agent/user-origin rules. That is deterministic for the current
  one-author-origin engine but is not browser-origin or browser-conformance
  behavior.
- No unitless/relative/percentage line height, font metrics, Unicode shaping,
  bidi, writing modes, alternate word-breaking modes, negative spacing, or
  browser text-layout parity is added.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser coverage on case-insensitive standalone forms and mixed-token
  rejection for all nine owners.
- Focus cascade coverage on parent/root fallback, terminal reset behavior,
  explicit `inherit`, invalid-later preservation, `!important`, source order,
  and `revert-layer` distinction.
- Run one public integration fixture through inherited text flow and at least
  one display-list/raster/capture/hit/semantic consumer path.
- Near issue completion retain full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

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

To be filled after implementation and local certification. Remote CI, push,
release, tag, registry publication, browser parity, security-boundary
certification, and promotion remain outside this local task.
