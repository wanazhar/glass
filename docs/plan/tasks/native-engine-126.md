---
id: native-engine-126
scope: glass-browser/native-engine/cascade-layers-underline-offset-revert-layer
status: complete
depends-on: [native-engine-125]
---

# Native bounded `text-underline-offset` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery and private rollback declaration
boundary proven by native-engine-125 to support the explicit CSS-wide
`revert-layer` keyword for the existing inherited `text-underline-offset`
property. Keep the finite signed pixel value, immutable text command, and
underline-only software raster owner unchanged.

## Context

The native engine already supports inherited signed underline offsets from
`-4px` through `4px`. The resolved value translates only the underline in the
existing immutable text command; overline and line-through retain their fixed
origins. Slices 122-125 established bounded top-level named-layer order and
private rollback for four inherited decoration properties. Underline offset is
the next adjacent consumer and must exercise the same cascade boundary without
widening the paint or geometry contract.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-underline-offset-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-125.md`

## Contract

### Declaration and cascade state

- `text-underline-offset: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `-4px` through `4px` integer values remain the only resolved public
  values. The private declaration state may contain `Value(-4..=4)` or
  `RevertLayer`; no unresolved keyword may reach `NativeDisplayCommand` or the
  software rasterizer.
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
  used. Descendants receive their parent offset; root fallback remains `0`,
  the property's current native initial value.
- `auto`, percentages, fractional values, positive-sign syntax, dimensions
  outside `-4px` through `4px`, `revert`, `inherit`, `unset`, `initial`,
  `all`, mixed tokens, duplicates, and unknown values remain unsupported typed
  diagnostics without raw stylesheet echo.

### Existing owners preserved

The resolved offset continues through the existing inherited computed style,
immutable text command, display-list, capture, and decoded raster path. Only
the underline origin is translated; overline and line-through continue to use
their existing fixed origins. Thickness, style, color, skip-ink, skip-spaces,
text layout, and all existing decoration geometry remain unchanged.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, the 125 parser and layer owners, and all unrelated
style diagnostics retain their existing contracts. This remains a
horizontal-tb, fixed-cell, local software-raster contract and does not claim
browser-wide CSS conformance or browser parity.

## Tradeoffs

- A fifth explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine. The
  type keeps the signed offset artifact separate from rollback-only keyword
  state.
- Reusing the existing candidate-slot model proves the same layer semantics for
  the existing underline translation owner. Other properties retain their
  current unsupported-keyword diagnostics.
- Root fallback remains `0` rather than changing omitted-property behavior.
  This preserves the current no-offset default and distinguishes inherited
  fallback from a concrete declaration.
- The implementation does not add `auto`, percentages, fractional or
  font-derived offsets, overline/line-through offsets, `all`, multiple origins,
  `!important` inversion, layer statements, nested/anonymous layers, or a
  general CSS-wide keyword parser. Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Implemented at `3ffe86f8` in
`crates/glass-browser/src/browser/native_engine/css.rs` and
`crates/glass-browser/tests/native_engine.rs`. The parser now accepts
case-insensitive `text-underline-offset: revert-layer` into a private
declaration-only state. Candidate storage reuses the bounded 15-layer registry
and unlayered bucket from 122; resolution rolls back the winning layer until a
concrete signed `-4px..=4px` value or the inherited/root fallback is reached.
The immutable display-list command, underline-only translation, and software
raster owners remain finite and unchanged. Parser, cascade, display-list,
command, and decoded-raster regressions cover named and inline layers,
repeated rollback, descendants, root fallback, and stable overline and
line-through origins. No dependency, feature, crate-boundary, layout, or
unrelated style change was made.

## Verification

The focused gate must cover:

- case-insensitive `revert-layer` parsing for underline offset and typed
  rejection of unsupported CSS-wide, dimension, sign, and unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, rollback from named and unlayered buckets,
  repeated rollback, inherited descendant fallback, and root `0px` fallback;
- immutable signed offset display-list values and decoded-raster evidence that
  moves underline only, proving the keyword cannot reach paint replay or change
  overline/line-through origins;
- the existing offset translation and thickness/style/color/skip-ink/
  skip-spaces owners remaining unchanged.

Then run the established native feature library/integration suites, locked
two-crate tests, strict Clippy, no-default-feature Clippy, warnings-denied
rustdoc, locked package/fuzz/deny/audit checks, and the repository's static
documentation/reliability/adapter/Web IR validators. Use isolated targets and
record exact commands, counts, durations, warnings, commits, issue
synchronization, and cleanup evidence here. Remote CI, browser parity,
release, registry publication, and a third crate remain outside local task
evidence unless separately executed and verified.

Results:

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-126-focused cargo test -p glass-browser --features native-engine --lib stylesheet_cascade_revert_layer_rolls_back_text_underline_offset_candidates`: 1 passed in 14m01s.
- `CARGO_TARGET_DIR=/tmp/glass-126-focused cargo test -p glass-browser --features native-engine --test native_engine native_text_underline_offset_revert_layer_moves_only_underlines -- --nocapture`: 1 passed in 6m15s; underline command/raster translation changed while overline and line-through remained stable.
- The complete native-enabled suites passed: browser library 922 passed/1
  ignored and native integration 163 passed.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-126-focused scripts/check-rust-workspace.sh test`: passed for both installable crates and all targets/features. The browser matrix included 923 library tests with 1 ignored, 163 native integration tests, 18 browser smoke tests, 1 daemon-recovery test, 5 protocol-conformance tests, 1 public-API test, 2 CLI reliability tests, 2 fixture reliability tests, 4 scenario reliability tests, 1 TUI PTY smoke test, 6 Web IR corpus tests, 1 workflow-authoring test, 32 workspace-contract tests, and all example targets. `glass-dev` reported 365 unit tests, 4 development-runtime integration tests, and 15 PTY tests.
- `CARGO_TARGET_DIR=/tmp/glass-126-focused scripts/check-rust-workspace.sh clippy`: passed with warnings denied for both crates (browser about 8m11s; dev about 7m20s). `CARGO_TARGET_DIR=/tmp/glass-126-focused cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings`: passed in about 4m45s.
- `RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR=/tmp/glass-126-focused cargo doc --workspace --all-features --locked --no-deps`: passed in about 3m20s.
- Locked default binary builds passed: `glass-dev` in 2.79s and
  `glass-browser` after the clean isolated compile, with the final incremental
  verification completing in 1.06s. Formatting and `git diff --check` passed.
- Locked packaging passed: browser crate 196 files/5.1 MiB and dev crate 69
  files/2.6 MiB; packaged dev dependency validation confirmed exact
  `glass-browser 0.3.14`.
- `cargo fetch --manifest-path fuzz/Cargo.toml --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-126-focused cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets`: passed in about 8m22s.
- `cargo deny check`: passed advisories, bans, licenses, and sources with the
  existing duplicate-dependency warnings. `cargo audit`: passed with the
  configured/known warnings for unmaintained `bincode` and `yaml-rust`, the
  allowed `lru` advisory, and yanked `chacha20`.
- Version sync, feature parity, TUI shortcut inventory, documentation depth,
  release documentation, script unit, reliability matrix, public adapters,
  Web IR, and documentation coverage validators passed. Release truth found
  541 Markdown documents, 83 current-version documents, 57 previous-version
  references, 638 semantic hits, and zero current-claim failures; coverage
  found 345 full-product MCP entries, 100 browser-only entries, 17 examples,
  and 22 public modules. Depth found 93 guides/19 contracts; TUI found 15
  implementation keys/63 markers; reliability found 6 scenarios/4 targets;
  adapters found 5; Web IR found 8 fixtures/8 scenarios/11 categories.

## Cleanup

All expensive gates used the isolated task target
`/tmp/glass-126-focused`. Before deletion it measured 10,048,896,513 bytes
across 14,723 files. The exact validator reports measured 10,126 bytes total:
`/tmp/glass-126-reliability.json` (3,482 bytes),
`/tmp/glass-126-adapters.json` (1,418 bytes),
`/home/ubuntu/work/glass/benchmarks/results/.glass-126-web-ir.json` (3,808
bytes), and the duplicate generated `--help` adapter report (1,418 bytes).
The 77 exact PID-142533 test scratch entries contained 80 files and 2,611
bytes. Process and recursive handle checks reported no Cargo/rustc/rustdoc/
Clippy/fuzz/package writer and no open handle for those exact paths. They were
removed with bounded `find -P <exact-path> -xdev -depth -delete` operations; no
process was terminated and no source, fixture, durable data, or unrelated
`/tmp` entry was touched. Available filesystem bytes before and after cleanup,
and the resulting byte delta, are recorded in the certification checkpoint
below after deletion. The exact target, report, and scratch paths are absent;
no repository-local `target/` and no top-level `/tmp/target/` remain. The three
pre-existing Glass processes were preserved.

## Certification

Local implementation and full validation are complete at `3ffe86f8`; exact
regenerable-output cleanup and issue synchronization are pending the final
closeout checkpoint. Remote CI remains pending because no push was authorized.
