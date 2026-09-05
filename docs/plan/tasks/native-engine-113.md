---
id: native-engine-113
scope: glass-browser/native-engine/text-decoration-style-wavy
status: complete
depends-on: [native-engine-112]
---

# Native bounded wavy text-decoration style

## Objective

Expose the bounded inherited `text-decoration-style:wavy` value through the
existing fixed-cell decoration path. The value must remain distinct from
border styling, travel through the current immutable text command, and paint a
deterministic fixed-pixel wave without adding a second layout or display-list
owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-112.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the case-insensitive inherited
`text-decoration-style:wavy` keyword alongside the supported
`solid|dashed|dotted|double` values. Omission continues to compute to `solid`,
and existing specificity, declaration order, inline precedence, and inheritance
remain unchanged. The text-decoration style is represented by the existing
dedicated `NativeTextDecorationStyle`; `NativeBorderStyle` and border CSS
grammar do not gain `wavy` as a side effect.

For every selected decoration line, `wavy` paints a continuous fixed-pixel
wave across the immutable text run. Its deterministic horizontal period is
eight pixels and its vertical phase is the repeating sequence
`[0, 1, 2, 1, 0, -1, -2, -1]` pixels. At each x column, the resolved
`text-decoration-thickness` paints that many consecutive pixels beginning at
the phase-adjusted line origin. The nominal line origin is phase zero; the
wave may therefore extend above or below that origin. The existing
`1px..=4px` thickness bound remains in force.

The wave starts at each emitted run's x origin; it does not continue phase
across fragments. Underline phase zero is translated by the existing signed
`-4px..=4px` `text-underline-offset` before the wave is sampled. Overline and
line-through retain their existing origins and do not consume underline
offset state. Decoration color, clipping, root scroll translation, opacity
replay, capture, hit testing, semantic projection, and source order continue
to use their existing consumers.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
decoration color, thickness, underline offset, font, shaping, bidi, writing
mode, accessibility projections, hit testing, capture, opacity grouping, or
source/semantic order. It does not add decoration-origin propagation,
fragment continuity, CSS metric centering, antialiasing, or browser-wide text
conformance. The existing integer-pixel, fixed-cell, horizontal-tb,
fixture-first, default-off `native-engine` boundary remains in force.

`double` retains its two-band behavior, and `solid`, `dashed`, and `dotted`
retain their existing behavior. The `text-decoration` shorthand is not
extended with style components by this slice.

## Tradeoffs

- A fixed eight-pixel phase and two-pixel amplitude make wavy output stable,
  cheap, and testable across platforms, but do not model font metrics, CSS
  stroke centering, or browser wave geometry.
- Applying the resolved thickness at every x column preserves the 110
  thickness contract, but thick waves can merge neighboring phases and occupy
  a larger vertical footprint than a one-pixel wave.
- Anchoring phase at every immutable run keeps the existing artifact ownership
  and avoids hidden cross-fragment state, but visible waves can restart at
  fragment boundaries.
- Reusing the existing line origin, offset, clip, scroll, opacity, capture,
  hit-test, semantic, and source-order paths keeps the change small and
  auditable, while leaving decoration-origin propagation, vertical writing,
  shaping, and full CSS conformance explicitly out of scope.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

Parser/cascade coverage accepts case-insensitive `wavy`, preserves default
solid behavior, inheritance, explicit override, omission, inline precedence,
and unsupported syntax diagnostics. Native integration proves the exact
eight-pixel phase, thickness-scaled vertical strokes, run-origin phase reset,
all three decoration lines, composition with underline offset, unchanged line
origins, clipping, scroll translation, immutable command propagation, and
unchanged layout geometry. Existing solid/dashed/dotted and double regressions
remain green.

The implementation checkpoint is `0c6a9ddc`; synchronized current-claim
documentation is `ebab6def`. Local certification used
`RUST_MIN_STACK=8388608` and an isolated `/tmp/glass-113-target`:

- focused CSS parser/cascade coverage: 2/2; focused wavy native integration:
  1/1; direct raster phase replay: 1/1; decoration-family regression: 7/7;
  double regression: 1/1; diagnostics regression: 3/3; full native
  integration: 150/150;
- all-feature `glass-browser` library: 909 passed, 1 ignored, 0 failed, with
  all integration targets and doctests passing;
- serial `glass-dev --locked` certification: 365 unit tests, 4 integration
  tests, 15 PTY tests, and all doctests passed; elapsed 4:50.05, peak RSS
  387,576 KiB. A prior parallel run exposed the existing load-sensitive
  `rust_analyzer_publishes_real_diagnostics_when_available` timeout; its
  isolated rerun passed, and the final certification used one test thread.
- strict browser Clippy passed in 8:08.78 with peak RSS 1,913,748 KiB;
  strict workspace Clippy passed in 9:25.47 with peak RSS 1,924,520 KiB;
  strict no-default-feature browser Clippy passed in 4:41.02 with peak RSS
  1,800,168 KiB; warning-denied workspace rustdoc passed in 3:10.21 with peak
  RSS 1,644,868 KiB;
- both explicit native/dev binaries built; the locked browser package
  verification passed in 14:57.17 with peak RSS 2,200,796 KiB, and the dev
  package was built with the local browser patch. The package validator
  confirmed `glass-dev` resolves `glass-browser` exactly at `0.3.14`;
  both publish dry-runs completed without upload because `0.3.14` already
  exists in the registry;
- locked offline fuzz all-target checking passed for 290 targets in 8:04.26
  with peak RSS 1,568,392 KiB;
- `cargo deny check` passed with existing duplicate-version warnings, and
  `cargo audit` passed with the four already-allowed findings: unmaintained
  `bincode`, unmaintained `yaml-rust`, the allowed `lru` advisory, and the
  yanked `chacha20` lock entry;
- version, feature-parity, TUI-shortcut, documentation-depth,
  release-documentation, documentation-coverage, public-adapter,
  reliability, and Web IR validators passed: 527 Markdown documents, 83
  current documents, 57 previous-version hits, 604 semantic hits, 0
  current-claim failures; coverage 527/345/100/17/22; TUI 15/63; depth
  93/19; reliability 6/4; adapters 5; Web IR 8/8/11;
- `cargo fmt --all -- --check` and `git diff --check` passed.

Every local gate used an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Cleanup

After certification, the exact `/tmp/glass-113-target` and all generated
113 reports/logs were measured as part of 156 user-owned, non-symlink
`glass-*`/`forgebuild-*` temporary roots totaling 12,352,401,408 allocated
bytes (11.504 GiB). The repository `target/.rustc_info.json` measured 4,096
allocated bytes. Final process and recursive `/tmp` open-file checks found no
Cargo/Rust writer and no open Glass/ForgeBuild candidate. The exact 156 roots
and the metadata file were removed with bounded `find -P ... -xdev -depth
-delete` operations; no shared Cargo registries, toolchains, source, durable
user data, or other projects' non-regenerable artifacts were touched.

Post-cleanup verification found no matching `/tmp/glass-*` or
`/tmp/forgebuild-*` entries, an empty repository `target` (4.0 KiB directory
allocation), and no `fuzz/target`. Filesystem state is 80 GiB available and
59% used on `/dev/sda1` (`df -h / /tmp`).
