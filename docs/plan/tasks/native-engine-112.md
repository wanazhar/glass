---
id: native-engine-112
scope: glass-browser/native-engine/text-decoration-style-double
status: complete
depends-on: [native-engine-111]
---

# Native bounded double text-decoration style

## Objective

Expose the bounded inherited `text-decoration-style:double` value through the
existing fixed-cell decoration path. The value must remain distinct from
border styling, travel through the current immutable text command, and paint
two deterministic solid bands without adding a second layout or display-list
owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-111.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the case-insensitive inherited
`text-decoration-style:double` keyword alongside the already supported
`solid|dashed|dotted` values. Omission continues to compute to `solid`, and
existing specificity, declaration order, inline precedence, and inheritance
remain unchanged. The text-decoration style is represented by a dedicated
`NativeTextDecorationStyle`; the existing `NativeBorderStyle` and border CSS
grammar do not gain `double` as a side effect.

For every selected decoration line, `double` paints two solid horizontal bands.
The first band begins at the existing line origin and retains the resolved
`text-decoration-thickness`; the second begins one transparent pixel after
the first band and has the same thickness. Thus the bounded fixed-cell
vertical footprint is `2 * thickness + 1` pixels, with the existing
`1px..=4px` thickness bound and normal surface/clip clipping. The underline
origin is still translated by the existing signed `-4px..=4px`
`text-underline-offset` before both bands are placed. Overline and
line-through retain their existing origins and do not consume underline
offset state.

The two bands are solid across the immutable text run width and remain
anchored at its x origin. Dashed and dotted styles retain their 110 horizontal
pattern helper and one-band-per-thickness behavior. The resolved dedicated
style travels beside decoration color, line flags, thickness, and underline
offset in the same immutable `TextRun`; clipping, root scroll translation,
opacity replay, capture, hit testing, semantic projection, and source order
continue to reuse their existing consumers.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
decoration color, thickness, underline offset, font, shaping, bidi, writing
mode, accessibility projections, hit testing, capture, opacity grouping, or
source/semantic order. It does not add decoration-origin propagation,
fragment continuity, CSS centering, or browser-wide text conformance. The
existing integer-pixel, fixed-cell, horizontal-tb, fixture-first,
default-off `native-engine` boundary remains in force.

`wavy`, CSS-wide keywords, unknown, empty, and other unsupported syntax
remains bounded typed diagnostics without raw stylesheet echo. The
`text-decoration` shorthand is not extended with style components by this
slice.

## Tradeoffs

- A dedicated text-decoration style type prevents `double` from silently
  becoming a border style and makes the boundary explicit, at the cost of
  touching the public native display-command style type.
- Retaining the full resolved thickness for each solid band makes 110's
  thickness meaning stable and deterministic, but the total double footprint
  is `2 * thickness + 1` rather than a browser font-metric fit.
- The one-pixel separation and positive-y band placement are deliberately
  simple fixed-cell rules. They may overlap glyphs or adjacent decorations,
  while clipping and the existing signed underline offset remain the only
  bounds owners.
- Reusing the existing immutable command, line origins, x anchoring, clip,
  scroll, opacity, capture, and software replay keeps one artifact pipeline,
  but does not provide real typographic decoration geometry or fragment
  continuity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

Implemented in `3bdd3b54` after the design checkpoint `b5668e87`. Parser and
cascade coverage proves case-insensitive `double`, default solid behavior,
inheritance, explicit override, omission, inline precedence, and unsupported
`wavy` diagnostics. Native integration proves two solid bands for underline,
overline, and line-through, composition with thickness and underline offset,
unchanged line origins, x-origin anchoring, clipping, immutable command
propagation, and unchanged layout geometry. Existing solid/dashed/dotted and
underline-offset regressions remain green.

The local certification matrix passed:

- focused CSS parser/cascade tests: 2 passed;
- focused double-decoration integration: 1 passed;
- focused raster replay: 1 passed;
- decoration regression family: 7 passed;
- underline-offset regression: 1 passed;
- complete native integration: 149 passed;
- complete `glass-browser --all-features` library/integration/doctest suite:
  908 passed, 1 ignored;
- complete `glass-dev` unit/integration/PTy/doctest suite: 399 passed;
- strict browser clippy, workspace clippy, and no-default-feature clippy;
- warning-denied workspace rustdoc;
- explicit `glass-dev` and feature-gated `glass-browser` binary builds;
- locked `cargo package` for `glass-browser` with archive verification and
  locked local-path `cargo package` for `glass-dev`;
- locked publish dry-runs for both crates, with uploads aborted by dry-run;
- packaged dependency check proving `glass-dev` resolves `glass-browser`
  exactly at `0.3.14`;
- offline locked fuzz workspace check across all targets;
- `cargo deny check` and `cargo audit`;
- version, feature-parity, TUI shortcut, documentation-depth, release-docs,
  documentation-coverage, public-adapter, reliability, and Web IR validators;
- `cargo fmt --all -- --check` and `git diff --check`.

The release documentation audit reported 526 Markdown files, 83 current
documents, 57 previous-version hits, 603 semantic audit hits, and zero
current-claim failures. The native integration and full crate suites use the
isolated `/tmp/glass-112-target` target. Representative measured gate costs
were:

| Gate | Elapsed | Peak RSS |
|---|---:|---:|
| `glass-browser --all-features` tests | 16:38.83 | 2,425,564 KiB |
| `glass-dev` tests | 20:21.38 | 1,997,588 KiB |
| workspace clippy | 10:53.60 | 1,908,668 KiB |
| browser clippy | 9:00.84 | 1,911,240 KiB |
| no-default-feature clippy | 4:44.92 | 1,797,956 KiB |
| warning-denied rustdoc | 4:35.42 | 1,633,028 KiB |
| browser package verification | 15:22.64 | 2,218,596 KiB |
| offline fuzz workspace check | 9:04.54 | 1,569,076 KiB |

Remote CI, browser parity, release, registry publication, and a third crate
are not claimed by this local task.

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after
certification. Do not remove shared Cargo registries, toolchains, source,
durable user data, or other projects' non-regenerable artifacts.

Cleanup completed on 2026-09-04 UTC after all gates finished. The exact
user-owned `/tmp/glass-*` and `/tmp/forgebuild-*` inventory contained 81 roots
and 12,350,939,136 allocated bytes, including the isolated
`/tmp/glass-112-target`, test-created TUI/tool/context directories, and the
two release-documentation reports. The writer scan for Cargo, rustc, rustdoc,
clippy, cargo-fuzz, and rust-analyzer was empty; recursive `lsof` checks found
no open handles. The paths were removed with `find -P ... -xdev -depth
-delete`, without following symlinks. The repository's user-owned
`target/.rustc_info.json` (1,298 logical bytes; 4,096 allocated bytes) was
removed separately, leaving the `target` directory empty. Total allocated
space reclaimed was 12,350,943,232 bytes (11.50 GiB). A post-removal scan found
no matching temporary roots or target artifacts; `/dev/sda1` reported 113G
used, 80G available, and 59% utilization. Shared Cargo registries, toolchains,
source, durable data, and other projects were not touched.
