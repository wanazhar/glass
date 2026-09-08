---
id: native-engine-198
scope: glass-browser/native-engine/inherited-text-css-wide-resets
status: complete
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

Implementation is `95a988d0`; design is `927cccca`. The implementation
replaces the specialized inherited-text declaration paths with one private
five-state declaration model for standalone, case-insensitive `inherit`,
`initial`, `unset`, `revert`, and `revert-layer`. Parent/root fallback,
terminal reset behavior, invalid-later preservation, existing cascade
priority, positive-pixel line-height auto representation, and the public
finite computed-style/artifact schemas remain bounded. `revert` remains the
one-author-origin parent fallback; it does not claim user-agent or user-origin
behavior.

Local evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Locked native-feature test-target check passed in
  `/tmp/glass-198-focused`.
- Focused parser/cascade unit passed: `1 passed; 0 failed`.
- Focused public integration passed: `1 passed; 0 failed`.
- Full native integration passed: `236 passed; 0 failed; 0 ignored`.
- Full native-feature library tests passed with `RUST_MIN_STACK=8388608`:
  `1019 passed; 0 failed; 1 ignored`. The default-stack run reproduces the
  pre-existing large-Clap parser test overflow in
  `cli::args::tests::agent_readiness_commands_are_explicit`; no task-198 test
  is involved.
- Strict native-feature Clippy passed with `-D warnings`.
- Warning-denied native-feature rustdoc passed.
- Static release truth passed: 612 Markdown documents (83 current, 57
  previous-version hits, 715 semantic audit hits, 0 current-claim failures).
- Documentation coverage, depth, feature parity, TUI shortcut, and version
  synchronization passed: 612 Markdown files, 345 full-product MCP tools
  (100 browser-only), 17 examples, 22 public modules, 93 current guides, 19
  substantive contracts, 14 capabilities across 4 targets, 15 implementation
  help keys, 63 documentation markers, and synchronized version `0.3.14`.

Paired-crate, package, security/fuzz, workspace all-target/all-feature, static
documentation, issue synchronization, final cleanup, and remote-CI gates
remain deferred to the final issue #40 certification boundary. Remote CI,
push, release, tag, registry publication, browser parity, security-boundary
certification, and promotion remain outside this local task.
