---
id: native-engine-197
scope: glass-browser/native-engine/dimension-css-wide-resets
status: planned
depends-on: [native-engine-196]
---

# Native dimension CSS-wide resets

## Objective

Add bounded standalone `initial`, `unset`, and one-author-origin `revert`
handling to the existing local dimension cascade owners without introducing a
general CSS-wide keyword engine or new used-value state.

## Contract

- `width`, `height`, `min-width`, `max-width`, `min-height`, and
  `max-height` accept standalone, case-insensitive `initial`, `unset`, and
  `revert`.
- These reset forms resolve to the existing optional dimension fallback
  (`None`/auto in the current engine). This represents the supported initial
  and unset behavior for the bounded non-negative pixel owner; no new zero,
  intrinsic, or containing-block value is invented.
- `revert` is bounded to the one author-origin model already used by this
  engine and therefore resolves to the same current local fallback. It is
  distinct from `revert-layer`, which continues to roll back only the current
  named layer.
- Reset candidates participate in the existing important-over-normal,
  reversed named-layer, inline-important, invalid-later, source-order, and
  explicit `inherit` behavior. A winning reset must not fall through to a
  lower candidate; `revert-layer` may still fall through by design.
- Reset keywords cannot be mixed with lengths, `inherit`, percentages, or
  other tokens. Omission remains local and does not inherit.

## Boundary and tradeoffs

- This slice does not add percentages, negative values, intrinsic sizing,
  aspect ratio, margin collapsing, positioning, replaced-element sizing,
  vertical writing modes, additional origins, transitions, animations, or
  browser-wide CSS sizing conformance.
- The reset is represented by private declaration state and normalized at the
  existing optional computed-dimension owner. Public schemas, layout owners,
  artifact consumers, dependencies, feature defaults, crate boundaries, and
  security boundaries remain unchanged.
- Using the existing `None` fallback keeps the bounded engine fast and
  deterministic, but it does not claim the full browser distinction between
  `auto`, `0`, `none`, intrinsic sizing, and used-value resolution.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone reset forms,
  mixed-token rejection, root/parent fallback, reset-vs-inherit precedence,
  terminal `!important`, invalid-later preservation, source order, and
  `revert-layer` distinction.
- Run one public integration fixture through reset geometry and at least one
  display-list/raster/point-hit/semantic consumer path.
- Near issue completion retain the full native integration, feature library,
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
