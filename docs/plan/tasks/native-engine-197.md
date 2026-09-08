---
id: native-engine-197
scope: glass-browser/native-engine/dimension-css-wide-resets
status: complete
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

Implementation is `a0e102b5`; design is `7d71a50c`. The implementation adds a
private `Reset` declaration for standalone case-insensitive dimension
`initial`, `unset`, and one-author-origin `revert`. Winning reset candidates
resolve to the existing optional `None`/auto fallback without falling through;
`revert-layer` remains the separate lower-layer rollback candidate. Omission,
explicit `inherit`, important/source-order, invalid-later, min/max, and all
existing artifact owners remain bounded.

Local evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Locked native-feature check passed in `/tmp/glass-197-focused`.
- Focused parser/cascade reset unit passed: `1 passed; 0 failed`.
- Focused public reset integration passed: `1 passed; 0 failed`.
- Full native integration passed: `235 passed; 0 failed; 0 ignored`.
- Strict native-feature Clippy passed with `-D warnings`.
- Warning-denied native-feature rustdoc passed.

Static release truth and documentation gates passed: 611 Markdown documents
(83 current, 57 previous-version hits, 714 semantic audit hits, 0 current-claim
failures); coverage found 611 Markdown files, 345 full-product MCP tools (100
browser-only), 17 examples, and 22 public modules; depth found 93 current
guides and 19 substantive contracts; feature parity found 14 capabilities
across 4 targets (baseline 0.3.0, next and checkout 0.3.14); TUI inventory
found 15 implementation help keys and 63 documentation markers; version sync
confirmed 0.3.14. Paired-crate, package, workspace, security/fuzz, final
cleanup, issue synchronization, and remote-CI gates remain deferred to the
final issue #40 certification boundary. No remote CI, push, release, tag,
registry publication, browser-parity, security-boundary, or promotion claim is
made.
