---
id: native-engine-125
scope: glass-browser/native-engine/cascade-layers-decoration-thickness-revert-layer
status: complete
depends-on: [native-engine-124]
---

# Native bounded `text-decoration-thickness` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery and private rollback declaration
boundary proven by native-engine-124 to support the explicit CSS-wide
`revert-layer` keyword for the existing inherited
`text-decoration-thickness` property. Keep the existing finite pixel value,
immutable text command, decoration geometry, and software raster owners
unchanged.

## Context

The native engine already supports inherited integer decoration thicknesses
from `1px` through `4px`. The resolved value is carried by the existing text
command and controls the current fixed-cell decoration bands; there is no
separate layout or geometry representation to introduce. Slices 122-124
established bounded top-level named-layer order and private rollback for three
inherited decoration properties. Thickness is the next adjacent consumer and
must exercise the same cascade boundary without widening the paint contract.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-thickness-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-124.md`

## Contract

### Declaration and cascade state

- `text-decoration-thickness: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `1px`, `2px`, `3px`, and `4px` values remain the only resolved
  public values. The private declaration state may contain
  `Value(1..=4)` or `RevertLayer`; no unresolved keyword may reach
  `NativeDisplayCommand` or the software rasterizer.
- Stylesheet candidates use the existing 15 named-layer registry and the
  unlayered bucket: first-appearance layer rank precedes selector specificity,
  source order remains the tie-breaker within one layer, and inline
  declarations stay in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants receive their parent thickness; root fallback remains
  `1`, the property's current native initial value.
- `auto`, `from-font`, `0px`, `5px`, fractional values, negative values,
  `revert`, `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates,
  and unknown values remain unsupported typed diagnostics without raw
  stylesheet echo.

### Existing owners preserved

The resolved thickness continues through the existing inherited computed style,
immutable text command, display-list, capture, and decoded raster path. The
current one-to-four-cell decoration geometry and style-specific replay remain
the owners for underline, overline, and line-through. No layout dimensions,
line metrics, wrapping, offsets, skip-ink, skip-spaces, style, color, or paint
pattern semantics change.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, the 124 parser and layer owners, and all unrelated
style diagnostics retain their existing contracts. This remains a horizontal-tb,
fixed-cell, local software-raster contract and does not claim browser-wide CSS
conformance or browser parity.

## Tradeoffs

- A fourth explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine. The
  type keeps the finite `u32` thickness artifact separate from rollback-only
  keyword state.
- Reusing the existing candidate-slot model proves the same layer semantics for
  a property whose concrete value already has dedicated geometry tests. Other
  properties retain their current unsupported-keyword diagnostics.
- Root fallback remains `1` rather than changing omitted-property behavior.
  This preserves the current default thickness and distinguishes inherited
  fallback from a concrete declaration.
- The implementation does not add `auto`, `from-font`, percentages, lengths
  outside `1px`-`4px`, `all`, multiple origins, `!important` inversion, layer
  statements, nested/anonymous layers, or a general CSS-wide keyword parser.
  Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Implemented at `271702ae` in
`crates/glass-browser/src/browser/native_engine/css.rs` and
`crates/glass-browser/tests/native_engine.rs`. The parser now accepts
case-insensitive `text-decoration-thickness: revert-layer` into a private
declaration-only state. Candidate storage reuses the bounded 15-layer registry
and unlayered bucket from 122; resolution rolls back the winning layer until a
concrete `1px|2px|3px|4px` value or the inherited/root fallback is reached.
The immutable display-list command, one-to-four-cell decoration geometry, and
software raster owners remain finite and unchanged. Parser, cascade,
display-list, command, and decoded-raster regressions cover named and inline
layers, repeated rollback, descendants, root fallback, and all three line
owners. No dependency, feature, crate-boundary, layout, or unrelated style
change was made.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for thickness and typed rejection of
  unsupported CSS-wide, dimension, and unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, rollback from named and unlayered buckets,
  repeated rollback, inherited descendant fallback, and root `1px` fallback;
- immutable one-to-four-cell display-list values and decoded-raster evidence
  for underline, overline, and line-through, proving the keyword cannot reach
  paint replay;
- the existing thickness geometry and style-pattern regressions, plus the
  inherited skip-ink, skip-spaces, and style owners remaining unchanged.

Then run the established native feature library/integration suites, locked
two-crate tests, strict Clippy, no-default-feature Clippy, warnings-denied
rustdoc, locked package/fuzz/deny/audit checks, and the repository's static
documentation/reliability/adapter/Web IR validators. Use isolated targets and
record exact commands, counts, durations, warnings, commits, issue
synchronization, and cleanup evidence here. Remote CI, browser parity,
release, registry publication, and a third crate remain outside local task
evidence unless separately executed and verified.

Results:

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-125-focused cargo test -p glass-browser --features native-engine --lib stylesheet_cascade_revert_layer_rolls_back_text_decoration_thickness_candidates`: 1 passed.
- `CARGO_TARGET_DIR=/tmp/glass-125-focused cargo test -p glass-browser --features native-engine --test native_engine native_text_decoration_thickness_revert_layer_reaches_all_line_owners -- --nocapture`: 1 passed; underline, overline, and line-through command/raster assertions passed.
- The complete native-enabled suites passed: browser library 921 passed/1
  ignored and native integration 162 passed.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-125-focused scripts/check-rust-workspace.sh test`: passed for both installable crates and all targets/features. The browser matrix included 922 library tests with 1 ignored, 162 native integration tests, 18 browser smoke tests, 1 daemon-recovery test, 5 protocol-conformance tests, 1 public-API test, 2 CLI reliability tests, 2 fixture reliability tests, 4 scenario reliability tests, 1 TUI PTY smoke test, 6 Web IR corpus tests, 1 workflow-authoring test, 32 workspace-contract tests, and all example targets. `glass-dev` reported 365 unit tests, 4 development-runtime integration tests, and 15 PTY tests.
- `CARGO_TARGET_DIR=/tmp/glass-125-focused scripts/check-rust-workspace.sh clippy`: passed with warnings denied for both crates (browser about 8m44s; dev about 7m15s). `CARGO_TARGET_DIR=/tmp/glass-125-focused cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings`: passed (about 4m50s).
- `RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR=/tmp/glass-125-focused cargo doc --workspace --all-features --locked --no-deps`: passed (about 3m22s).
- Locked default binary builds passed: `glass-browser` about 5m46s and
  `glass-dev` 2.84s. Formatting and `git diff --check` passed.
- Locked packaging passed: browser crate 196 files/5.1 MiB and dev crate 69
  files/2.6 MiB; packaged dev dependency validation confirmed exact
  `glass-browser 0.3.14`.
- `cargo fetch --manifest-path fuzz/Cargo.toml --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-125-focused cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets`: passed (about 9m07s).
- `cargo deny check`: passed advisories, bans, licenses, and sources with the
  existing duplicate-dependency warnings. `cargo audit`: passed with the
  configured/known warnings for unmaintained `bincode` and `yaml-rust`, the
  allowed `lru` advisory, and yanked `chacha20`.
- Version sync, feature parity, TUI shortcut inventory, documentation depth,
  release documentation, script unit, reliability matrix, public adapters,
  Web IR, and documentation coverage validators passed. Release truth found
  539 Markdown documents, 83 current-version documents, 57 previous-version
  references, and zero current-claim failures; coverage found 345 full-product
  MCP entries, 100 browser-only entries, 17 examples, and 22 public modules.

The checkout remained local-only throughout. No remote CI, browser parity,
release, registry publication, push, tag, or third-crate claim is made.

## Cleanup

All expensive gates used the isolated `/tmp/glass-125-focused` target. Before
deletion it measured 10,075,952,454 bytes across 14,842 files. Four exact
validator reports measured 180,429 bytes total, and the 64 exact
PID-3802988 test scratch roots measured 2,231 bytes total. Process and
recursive handle checks reported no Cargo/rustc/rustdoc/Clippy/fuzz/package
writer and no open handle for those paths. They were removed with bounded
`find -P <exact-path> -xdev -depth -delete` operations; no process was
terminated and no source, fixture, durable data, or unrelated `/tmp` entry was
touched. Available filesystem bytes increased from 75,375,235,072 to
85,495,951,360, a measured delta of 10,120,716,288 bytes (about 9.42 GiB).
The exact target, report, and scratch paths are absent; no repository-local
`target/` and no top-level `/tmp/target/` remain. The three pre-existing Glass
processes were preserved.

## Certification

Local implementation, full validation, exact regenerable-output cleanup, and
issue synchronization are complete at `271702ae`/`a112ab1e`. The authenticated
maintainer completion comment is
<https://github.com/wanazhar/glass/issues/40#issuecomment-5557160661>;
remote CI remains pending because no push was authorized.
