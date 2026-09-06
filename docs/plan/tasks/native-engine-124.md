---
id: native-engine-124
scope: glass-browser/native-engine/cascade-layers-decoration-style-revert-layer
status: complete
depends-on: [native-engine-123]
---

# Native bounded `text-decoration-style` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery from native-engine-122 and the
private declaration boundary proven by native-engine-123 to support the
explicit CSS-wide `revert-layer` keyword for the existing inherited
`text-decoration-style` property. Keep the finite public style enum, immutable
text command, and software raster owner unchanged.

## Context

The native engine already supports the five finite decoration patterns
`solid`, `dashed`, `dotted`, `double`, and `wavy` through one inherited computed
style and one immutable text command. Slices 122 and 123 established bounded
top-level named-layer ordering and private rollback for two inherited
decoration properties. The style property is the next adjacent consumer: its
resolved value already selects the existing pattern helper and has no separate
layout or geometry owner.

The normative references are:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-style-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-123.md`

## Contract

### Declaration and cascade state

- `text-decoration-style: revert-layer` is accepted as one
  case-insensitive token and stored in a private declaration-only state.
- Existing `solid`, `dashed`, `dotted`, `double`, and `wavy` values remain the
  only resolved public values. The private declaration state may contain
  `Value(Solid|Dashed|Dotted|Double|Wavy)` or `RevertLayer`; no unresolved
  keyword may reach `NativeDisplayCommand` or the software rasterizer.
- Stylesheet candidates use the 122 layer registry and candidate slots:
  first-appearance layer rank precedes selector specificity, source order
  remains the tie-breaker within one layer, and inline declarations stay in
  the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining candidate for this property. A named layer
  rolls back to the next lower candidate; the unlayered bucket rolls back to
  the highest named candidate. Repeated rollback continues until a concrete
  value is found or no candidate remains.
- When no candidate remains, the existing inherited computed-style boundary is
  used. Descendants receive their parent style; root fallback remains `Solid`,
  the property's current native initial value.
- `revert`, `inherit`, `unset`, `initial`, `all`, mixed tokens, duplicates, and
  unknown values remain unsupported typed diagnostics without raw stylesheet
  echo.

### Existing owners preserved

The resolved style continues through the existing inherited computed style,
immutable text command, fixed-cell pattern helper, software decoration replay,
display-list, capture, and decoded raster tests. `double` continues to paint
two solid bands separated by one cell; `wavy` continues to use its existing
bounded phase; `solid`, `dashed`, and `dotted` retain their current patterns.

Layout, fixed-cell geometry, whitespace collapsing, spacing arithmetic,
wrapping, alignment, overflow, clipping, scrolling, opacity, hit testing,
semantics, source order, skip-ink, skip-spaces, and the 123 layer/parser
owners retain their existing contracts. This remains a horizontal-tb,
fixed-cell, local software-raster contract and does not claim browser-wide CSS
conformance or browser parity.

## Tradeoffs

- A third explicit private declaration state duplicates a small resolver shape
  instead of prematurely designing a generic CSS-wide keyword engine; the
  type keeps the finite public style values and rollback-only keyword state
  visibly separate.
- Reusing the existing candidate-slot model exercises layer ordering across
  three inherited decoration properties, while style's five public values
  continue to be owned by the existing raster helper. Other properties retain
  their current unsupported-keyword diagnostics.
- Root fallback remains `Solid` rather than changing omitted-property behavior.
  This preserves existing pattern tests and distinguishes inherited fallback
  from a concrete declaration.
- The implementation does not add `all`, multiple origins, `!important`
  inversion, layer statements, nested/anonymous layers, `@import`, or a
  general CSS-wide keyword parser. Those boundaries remain typed and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Implemented at `d6c8bc70` in
`crates/glass-browser/src/browser/native_engine/css.rs` and
`crates/glass-browser/tests/native_engine.rs`. The parser now accepts
case-insensitive `text-decoration-style: revert-layer` into a private
declaration-only state. Candidate storage reuses the bounded 15-layer registry
and unlayered bucket from 122; resolution rolls back the winning layer until a
concrete `Solid|Dashed|Dotted|Double|Wavy` value or the inherited/root fallback
is reached. The immutable display-list command and fixed-cell software raster
owner remain finite and unchanged. Parser, cascade, display-list, command, and
decoded-raster regressions cover all five patterns, named and inline layers,
repeated rollback, descendants, and root fallback. No dependency, feature,
crate-boundary, layout, or unrelated style change was made.

## Verification

The focused gate covered:

- case-insensitive `revert-layer` parsing for style and typed rejection of
  unsupported CSS-wide/unknown forms;
- named-layer ordering before specificity, repeated layer reopening,
  unlayered and inline precedence, and the existing 15-layer/invalid-form
  diagnostics inherited from 122;
- rollback from a named layer to a lower named layer, unlayered rollback to the
  highest named layer, repeated rollback, inherited descendant fallback, and
  root `Solid` fallback;
- immutable display-list style values and decoded raster evidence for all
  existing finite styles, proving the keyword cannot reach paint replay.

Results:

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-124-focused cargo test -p glass-browser --features native-engine --lib stylesheet_cascade_revert_layer_rolls_back_text_decoration_style_candidates`: 1 passed.
- `CARGO_TARGET_DIR=/tmp/glass-124-focused cargo test -p glass-browser --features native-engine --test native_engine native_text_decoration_style_revert_layer_reaches_all_pattern_owners -- --nocapture`: 1 passed; all five finite patterns reached command and decoded-raster assertions.
- The complete native-enabled suites passed: browser library 920 passed/1
  ignored and native integration 161 passed.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-124-focused scripts/check-rust-workspace.sh test`: passed for both installable crates and all targets/features; `glass-dev` reported 365 unit tests, 4 development-runtime integration tests, and 15 PTY tests. An initial concurrent pass exposed the existing extension fixture `Text file busy` race; the exact test passed with `--test-threads=1`, and the complete rerun passed.
- `CARGO_TARGET_DIR=/tmp/glass-124-focused scripts/check-rust-workspace.sh clippy`: passed with warnings denied for both crates (browser 8m18s; dev 7m03s). `CARGO_TARGET_DIR=/tmp/glass-124-focused cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings`: passed (4m52s).
- `RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR=/tmp/glass-124-focused cargo doc --workspace --all-features --locked --no-deps`: passed (3m21s).
- Locked default binary builds passed: `glass-browser` 5m33s and `glass-dev` 1.35s. Documentation coverage passed for 538 Markdown documents, 345 full-product MCP entries, 100 browser-only entries, 17 examples, and 22 public modules.
- Locked packaging passed: browser crate 196 files/5.1 MiB and dev crate 69 files/2.6 MiB; packaged dev dependency validation confirmed exact `glass-browser 0.3.14`.
- `cargo fetch --manifest-path fuzz/Cargo.toml --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-124-focused cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets`: passed; the final fuzz check completed in 8m32s.
- `cargo deny check`: passed advisories, bans, licenses, and sources; existing duplicate-dependency warnings remained. `cargo audit`: passed with the repository's configured/known warnings for bincode, yaml-rust, lru, and yanked chacha20.
- Formatting, version sync, feature parity, TUI shortcut, documentation-depth,
  release-documentation, script unit, reliability-matrix, public-adapter, Web
  IR, and `git diff --check` validators passed. The release-documentation gate
  found 538 Markdown documents, 83 current-version documents, 57
  previous-version references, 631 semantic hits, and zero current-claim
  failures; the script unit suite reported 9 passed. The remaining validators
  reported 93/19 documentation depth, 14 capabilities across 4 targets, 15
  implementation help keys and 63 documentation markers, 6 reliability
  scenarios across 4 targets, 5 public adapters, and 8 Web IR fixtures.

Remote CI, browser parity, release, registry publication, and a third crate
remain outside this local task evidence. The checkout remains local-only, so
no remote-green claim is made.

## Cleanup

All expensive gates used the isolated `/tmp/glass-124-focused` target. Before
deletion it measured 10,071,981,556 bytes across 14,842 files. Four exact
validator reports measured 355,000 bytes total, and the 64 exact
PID-3315480 test scratch roots measured 2,231 bytes total. Process and
recursive handle checks reported no Cargo/rustc/rustdoc/Clippy/fuzz/package
writer and no open handle for those paths. They were removed with bounded
`find -P <exact-path> -xdev -depth -delete` operations; no process was
terminated and no source, fixture, durable data, or unrelated `/tmp` entry was
touched. Available filesystem bytes increased from 75,394,125,824 to
85,510,963,200, a measured delta of 10,116,837,376 bytes (about 9.42 GiB).
The exact target, report, and scratch paths are absent; no repository-local
`target/` and no top-level `/tmp/target/` remain. The three pre-existing Glass
processes were preserved.

## Certification

Local implementation, full validation, exact regenerable-output cleanup, and
documentation audits are complete at `d6c8bc70`. Issue #40 synchronization is
recorded in the maintainer handoff after this certification; remote CI remains
pending because no push was authorized.
