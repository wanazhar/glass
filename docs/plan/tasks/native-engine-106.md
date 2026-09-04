---
id: native-engine-106
scope: glass-browser/native-engine/text-decoration-combinations
status: complete
depends-on: [native-engine-105]
---

# Native bounded combined text-decoration lines

## Objective

Extend the existing inherited fixed-cell `text-decoration` owner from one
line keyword to a bounded combination of distinct line keywords in the
`text-decoration` shorthand. Preserve the current `none`, `underline`,
`overline`, and `line-through` behavior while allowing deterministic combined
line artifacts through the existing immutable display-list and software
raster paths.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-105.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts one or more distinct ASCII-whitespace-separated
line keywords in the inherited `text-decoration` shorthand:
`underline|overline|line-through`. `none` is valid only by itself. Matching is
case-insensitive; token order is irrelevant to the resulting line state;
duplicates, `none` combinations, unknown tokens, CSS-wide keywords, colors,
and other shorthand components remain typed unsupported-value diagnostics
without raw stylesheet echo.

The computed value is one bounded immutable three-bit line set. Its default is
the empty set (`none`), and an explicit child `none` clears every inherited
line. Each enabled line reaches the existing display-list command and fixed
cell raster owner:

- `overline` paints at `origin_y - 1`;
- `line-through` paints at `origin_y + 3`;
- `underline` paints at `origin_y + 7`.

Combined lines paint together with their existing fixed positions. The line
set does not alter text width, line formation, wrapping, alignment, overflow
geometry, hit testing, semantic/source order, accessibility projections,
scroll offsets, opacity grouping, capture dimensions, or clipping ownership.
The display command may carry multiple enabled line bits, but it remains one
immutable text artifact consumed by all downstream paths.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. `text-decoration-line` longhand semantics,
decoration propagation parity, colors, thickness, styles, offsets, blink,
font metrics, descender-aware placement, Unicode shaping, bidi, vertical
writing, and browser-wide text conformance remain outside the contract.

## Tradeoffs

- A compact line-set value keeps inheritance, `none` clearing, command
  projection, clipping, scrolling, opacity replay, capture, and raster on one
  owner while allowing the common combined shorthand forms.
- Rejecting duplicate tokens rather than silently deduplicating keeps malformed
  or surprising authored CSS visible through the existing diagnostic channel.
- Order-insensitive parsing gives deterministic computed state without adding
  authored token order to layout or paint artifacts.
- Fixed offsets remain deliberately simple and may be clipped or overlap
  glyph pixels under the existing fixed-cell limitations; font-aware metrics
  are deferred.
- No new crate, dependency, renderer, geometry owner, artifact schema, or
  default feature is introduced; the experiment remains inside
  `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

Implementation checkpoint: `baf680ee` (design checkpoint: `c0525afb`). The
implementation keeps the two-crate boundary and adds no dependency, feature,
renderer, geometry owner, or artifact schema. The existing display-list and
fixed-cell raster paths now consume a compact three-bit line set.

The completed local gate evidence is:

- focused CSS parser/cascade coverage: 2/2 passed;
- focused native decoration integration: 3/3 passed;
- existing scroll/clip/raster regression: 1/1 passed;
- unsupported-value diagnostic regression: 1/1 passed;
- full `native-engine` integration suite: 143 passed, 0 failed, 0 ignored;
- feature-enabled `glass-browser` library suite with
  `RUST_MIN_STACK=8388608`: 896 passed, 0 failed, 1 ignored;
- `cargo fmt --all -- --check` and `git diff --check`: passed;
- strict all-feature workspace Clippy: passed in 13m31s; strict
  no-default-feature `glass-browser` Clippy: passed in 5m50s;
- warning-denied workspace rustdoc: passed in 3m17s;
- locked `glass-dev --bins` build: passed in 11m36s;
- locked packages passed for both crates. Cargo emitted only the known
  non-fatal yanked `chacha20 v0.10.1` warning; the packaged-dependency
  validator confirmed `glass-dev` resolves `glass-browser` exactly at
  `0.3.14`;
- locked fuzz fetch completed and nightly/offline all-target checking passed
  in 9m54s. The first stable cargo-fuzz invocation was correctly rejected
  because sanitizer `-Z` flags require nightly; no source failure resulted;
- version sync, feature parity, TUI shortcut inventory, documentation depth,
  read-only adapters, reliability matrix, Web IR, and release documentation
  validators passed. Release documentation reported 520 Markdown files, 83
  current documents, 57 previous-version hits, 594 semantic hits, and 0
  current-claim failures;
- fresh source-built documentation coverage passed with 520 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules. The installed `/home/ubuntu/.cargo/bin/glass` was not used because
  it is a stale 0.3.14 build; coverage used the current source-built
  binaries.

The default 2 MiB test-thread stack still overflows in the pre-existing
`cli::args::tests::agent_readiness_commands_are_explicit` test; the failure
reproduced in the focused run and the documented 8 MiB stack passed the full
suite. Remote CI remains pending because `main` is local-only. No
browser-parity, release, registry-publication, or remote-certification claim
is part of this task.

## Cleanup

After all gates completed on 2026-09-04 UTC, the exact non-symlink path
`/tmp/glass-106-target` (6.5G) and exact regenerable report
`/tmp/glass-106-release-documentation.json` (164K) were checked for active
processes and open files, then removed with bounded same-filesystem deletion.
Neither remains; the repository and ForgeBuild target directories are 4.0K,
`fuzz/target` is absent, and the final filesystem check reports 64G available
(67% used).

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
