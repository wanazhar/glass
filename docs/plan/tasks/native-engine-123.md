---
id: native-engine-123
scope: glass-browser/native-engine/cascade-layers-skip-ink-revert-layer
status: complete
depends-on: [native-engine-122]
---

# Native bounded `text-decoration-skip-ink` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery from native-engine-122 to support the
explicit CSS-wide `revert-layer` keyword for the existing inherited
`text-decoration-skip-ink` property. Keep the resolved `Auto|None` paint value,
immutable text command, and software raster owner unchanged.

## Context

Native-engine-122 introduced first-appearance ordering for at most 15 named
top-level layers, an implicit unlayered bucket above them, and private
`revert-layer` rollback for `text-decoration-skip-spaces`. The native engine
already resolves `text-decoration-skip-ink` as an inherited finite
`Auto|None` value, but its declaration storage still assumes that every valid
candidate is immediately resolved. This slice exercises the same layer
boundary with a second inherited decoration property while preserving the
private/public separation.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-ink-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-122.md`

## Contract

### Declaration and cascade state

- `text-decoration-skip-ink: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `auto` and `none` values remain the only resolved public values.
  The private declaration state may contain `Value(Auto)`, `Value(None)`, or
  `RevertLayer`; no unresolved keyword may reach `NativeDisplayCommand` or
  the software rasterizer.
- Stylesheet candidates use the 122 layer registry and candidate slots:
  first-appearance layer rank precedes selector specificity, existing source
  order remains the tie-breaker within one layer, and inline declarations stay
  in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants therefore receive their parent `Auto|None` value; root
  fallback remains `Auto`, the property's current native initial value.
- `revert` remains unsupported for this property in this slice, as do
  `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates, and unknown
  values. Each remains a bounded typed diagnostic without raw stylesheet echo.

### Existing owners preserved

The resolved value continues through the existing inherited computed style,
immutable text command, glyph-intersection check, line-edge provenance,
software decoration replay, display-list, capture, and decoded raster tests.
`Auto` continues to suppress decoration pixels intersecting eligible glyphs;
`None` continues to paint through them. No new skip-ink behavior is invented.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, and the 122 layer parser owners retain their existing
contracts. This remains a horizontal-tb, fixed-cell, local software-raster
contract and does not claim browser-wide CSS conformance or browser parity.

## Tradeoffs

- A second private declaration state duplicates a small amount of resolver
  shape instead of prematurely designing a generic CSS-wide keyword engine;
  the type remains explicit about the distinct inherited property and its
  finite public value.
- Reusing the 122 candidate-slot model proves layer ordering across two
  properties, but only `text-decoration-skip-ink` and `text-decoration-skip-
  spaces` have meaningful rollback semantics. Other properties retain their
  existing unsupported-keyword diagnostics.
- Root fallback remains `Auto` rather than changing the omitted-value
  behavior. This keeps the slice compatible with existing raster expectations
  while distinguishing inherited fallback from a concrete declaration.
- The implementation does not add `all`, multiple origins, `!important`
  inversion, layer statements, nested/anonymous layers, `@import`, or a
  general CSS-wide keyword parser. Those boundaries remain visible and typed.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Complete at `af644112`. The native stylesheet now stores skip-ink candidates
in the existing bounded 15-slot layer registry, with the unlayered/inline
bucket above named layers. A private declaration state distinguishes concrete
`Auto|None` values from `RevertLayer`; rollback blocks only the winning layer,
continues through lower candidates, and reaches the existing inherited/root
fallback without exposing keyword state to the public computed value,
`NativeDisplayCommand`, or the software rasterizer. Parsing is
case-insensitive and remains fail-closed for unsupported CSS-wide, mixed, and
unknown forms. Unit and integration regressions cover parser typing, layer
priority before specificity, repeated layer rollback, inline/unlayered
rollback, inherited fallback, display-list values, and decoded raster output.
No package dependency, feature default, crate boundary, public paint value,
layout owner, or unrelated style behavior changed.

## Verification

The focused gate covered:

- case-insensitive `revert-layer` parsing for skip-ink and typed rejection of
  unsupported CSS-wide/unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, and the existing 15-layer/invalid-form
  diagnostics inherited from 122;
- rollback from a named layer to a lower named layer, unlayered rollback to the
  highest named layer, repeated rollback, inherited descendant fallback, and
  root `Auto` fallback;
- display-list values and decoded raster evidence for both `Auto` and `None`,
  proving the keyword cannot reach paint replay.

The focused and full local gates passed using the isolated target
`/tmp/glass-123-focused`:

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-123-focused cargo test -p glass-browser --features native-engine --lib stylesheet_cascade_revert_layer_rolls_back_skip_ink_candidates` — 1 passed; the clean focused compile took 13m36s and the test body 0.11s.
- `CARGO_TARGET_DIR=/tmp/glass-123-focused cargo test -p glass-browser --features native-engine --test native_engine native_text_decoration_skip_ink_revert_layer_reuses_layers_and_reaches_raster -- --nocapture` — 1 passed; 6m07s compile and 0.07s test body.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-123-focused cargo test -p glass-browser --features native-engine --lib` — 919 passed, 0 failed, 1 ignored; 7.25s test body.
- `CARGO_TARGET_DIR=/tmp/glass-123-focused cargo test -p glass-browser --features native-engine --test native_engine` — 160 passed, 0 failed; 2.55s test body.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-123-focused scripts/check-rust-workspace.sh test` — passed on the rerun in 19m29.34s: the browser all-target/all-feature suite passed with 1 ignored test, native integration passed 160 tests, and the complete `glass-dev` unit, development-runtime integration, and PTY suites passed (365, 4, and 15 respectively). The first full-workspace attempt exposed one pre-existing timing-test race, `browser::session::tests::popup_events_during_every_final_query_fail_at_shared_deadline`; its serial exact rerun passed, followed by the successful full rerun.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-123-focused cargo test -p glass-browser --tests --no-default-features --locked` — passed in 11m23.39s: 777 unit tests passed with 1 ignored, plus browser smoke 18, protocol 5, public 1, reliability CLI 2, reliability fixture 2, reliability scenarios 4, TUI smoke 1, Web IR 6, workflow 1, and workspace 32.

Strict and packaging gates also passed:

- `cargo fmt --all -- --check`, version sync, feature parity, TUI shortcuts, documentation depth, and `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` (9 passed) passed.
- `CARGO_TARGET_DIR=/tmp/glass-123-focused scripts/check-rust-workspace.sh clippy` passed in 15m17.81s; browser no-default-feature Clippy with `-D warnings` passed in 4m38.73s.
- `RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR=/tmp/glass-123-focused cargo doc --workspace --all-features --locked --no-deps` passed in 3m22.03s.
- Default browser and dev binaries built in the isolated target; documentation coverage passed with 537 Markdown files, 345 full-product MCP tools, 100 browser-only tools, 17 examples, and 22 public modules.
- Release-documentation truth passed with 537 Markdown files, 83 current documents, 57 previous-version hits, 628 semantic hits, and 0 current failures. Reliability, public read-only adapter, and Web IR validators passed with 6 scenarios/4 targets, 5 adapters, and 8 fixtures/8 scenarios/11 categories respectively.
- Locked browser and dev packages passed (196 files/5.1 MiB and 69 files/2.6 MiB); packaged dev dependency validation confirmed exact `glass-browser` 0.3.14. Fuzz check passed with the locked offline graph. `cargo deny check` passed with existing duplicate-dependency warnings; `cargo audit` passed with the configured warnings for unmaintained `bincode`/`yaml-rust`, the allowed `lru` advisory, and yanked `chacha20`.

The implementation commit is `af644112`; no remote CI, browser parity,
release, registry publication, or third crate is claimed from this local-only
checkout.

## Cleanup

All expensive gates used `/tmp/glass-123-focused`, which measured
11,934,890,656 bytes across 16,305 files before cleanup. The reports were
`/tmp/glass-123-release-doc.json` (174,249 bytes),
`/tmp/glass-123-reliability.json` (3,482 bytes), and
`/tmp/glass-123-adapters.json` (1,418 bytes). Fresh process and open-handle
checks found no Cargo/rustc/rustdoc/Clippy/fuzz/test writer and no `lsof`
handle for the target. The exact target and reports were deleted with bounded
`find -P ... -xdev -depth -delete`, reclaiming 11,982,045,184 available
filesystem bytes. The stale test scratch roots created by the prior 122 and
current 123 runs were then checked for active consumers and removed by exact
name; they contained only ephemeral `secret.txt`/`Cargo.toml` fixtures and no
nested targets. No process was terminated; the three pre-existing Glass
processes (PIDs 590083, 610599, and 611107) were preserved. Final checks found
no repository `target/`, no `/tmp` target directory, no named task scratch
root, and no current task report candidate.

## Certification

Locally certified at `af644112` after implementation, focused and full native
tests, locked two-crate tests, strict lint, no-default lint, rustdoc,
packaging, fuzz, dependency, formatting, documentation, reliability, adapter,
Web IR, and exact regenerable-output cleanup. The public resolved value
remains finite and unchanged; layer rank and `revert-layer` rollback remain
private to the CSS cascade. Issue #40 was synchronized through the authenticated
wanazhar account in comment `5555996896` after the body update, with the
implementation, task, architecture, analysis, plan, and cleanup markers
verified through the GitHub API. Remote CI, browser parity, release, and
registry publication remain explicitly unclaimed from this local checkout.
