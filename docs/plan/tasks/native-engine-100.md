---
id: native-engine-100
scope: glass-browser/native-engine/text-align-logical
status: complete
depends-on: [native-engine-099]
---

# Native bounded logical text alignment

## Objective

Add the bounded logical `text-align:start|end` values to the native engine's
existing fixed-cell inline-flow owner. Resolve those values against the
inherited `direction:ltr|rtl` state delivered by 099, while preserving the
physical behavior of `left|center|right`, source/semantic order, and the
existing no-bidi/shaping boundary.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-099.md`
- [CSS Text Module Level 3: text-align](https://www.w3.org/TR/css-text-3/#text-align-property)
- [CSS Writing Modes Level 4: direction](https://www.w3.org/TR/css-writing-modes-4/#propdef-direction)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts only the additional bounded logical values
`text-align:start` and `text-align:end`. The property remains inherited
through the existing DOM style walk. Its initial/fallback value remains the
current physical-left behavior for compatibility with the native engine's
existing fixed-cell model; this slice does not silently change omitted
`text-align` output.

For eligible horizontal-tb fixed-cell inline flow:

- `start` aligns the line's inline content to the physical left edge under
  `direction:ltr` and to the physical right edge under `direction:rtl`;
- `end` maps to the opposite physical edge;
- `left`, `right`, and `center` retain their existing physical semantics and
  are not reinterpreted through direction;
- wrapped lines resolve their logical alignment independently through the
  existing line flush owner, including direct text, supported inline boxes,
  hard breaks, whitespace modes, spacing, indent, overflow, and vertical
  alignment;
- every shifted inline item subtree and text run continues through the same
  layout, display-list, raster, viewport, hit-test, root-overflow, capture,
  scroll, and semantic consumers; source, semantic, and keyboard order remain
  unchanged;
- selector and inline cascade precedence, inherited direction overrides, and
  unsupported-value diagnostics remain typed and bounded.

The slice remains restricted to the current horizontal-tb, integer-pixel,
fixed-cell implementation. Unicode bidi resolution, glyph shaping, mixed bidi
runs, `unicode-bidi`, `text-align:justify|match-parent|justify-all`, logical
properties, vertical writing modes, grid, floats, and browser-wide text
conformance remain outside the contract and retain fail-closed fallback.

## Tradeoffs

- Reusing `FlowCursor` and its line-flush translation keeps logical alignment
  consistent across text, inline boxes, paint, hit testing, overflow, and
  capture without introducing a second geometry owner.
- Keeping physical `left|right` distinct avoids a breaking reinterpretation of
  existing native documents; callers that want direction-aware alignment must
  opt into `start|end`.
- The feature uses the inherited base direction as the sole logical mapping
  input. It does not inspect the first strong character or reorder text, so it
  provides useful layout alignment without claiming bidi paragraph behavior.
- Omitted `text-align` continues to use the established native default rather
  than adopting the broader CSS initial-value model in this isolated slice.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation checkpoint: `3380978c` (`feat(native-engine): support logical
text alignment`); design checkpoint: `2dae80fc`. The focused and full local
gates passed against the isolated `CARGO_TARGET_DIR=/tmp/glass-100-target`
build root:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused CSS parser/cascade tests: 3 passed;
- focused logical alignment artifact test: 1 passed, covering ltr/rtl
  start/end, wrapped lines, inline boxes, hit testing, display-list, raster,
  and source order;
- focused unsupported-value diagnostic regression: 1 passed;
- full native integration suite: 136 passed, 0 failed;
- feature-enabled browser library suite with `RUST_MIN_STACK=8388608`: 891
  passed, 1 existing ignored, 0 failed;
- `cargo clippy --workspace --all-targets --all-features --locked -- -D
  warnings` passed in 14m14s;
- `cargo clippy -p glass-browser --no-default-features --all-targets --locked
  -- -D warnings` passed in 6m28s;
- warning-denied workspace rustdoc passed in 3m53s;
- locked `glass-dev --bins` build passed in 12m49s;
- direct `glass-browser` packaging and verification passed. A direct unpatched
  `glass-dev` package verification correctly exposed that the published
  crates.io `glass-browser 0.3.14` copy predates this checkout's CLI runtime
  fields; the canonical paired-package path (`scripts/smoke-clean-install.sh`)
  packages both archives with the local browser patch and passed clean
  extraction, verification, core-only/full installs, `--version`, `--help`,
  `capabilities`, and every ownership transition. The normalized dependency
  validator also confirmed exact `glass-browser = 0.3.14`;
- locked fuzz fetch and offline all-targets check passed in 8m15s;
- static documentation/release validators passed:
  - version and feature parity are synchronized at 0.3.14;
  - release documentation truth: 514 Markdown documents, 83 current
    documents, 57 previous-version hits, 578 semantic audit hits, and 0
    current-claim failures;
  - TUI shortcut inventory: 15 implementation help keys and 63 documentation
    markers;
  - documentation depth: 93 current guides and 19 substantive contracts;
  - documentation coverage: 514 Markdown files, 345 full-product MCP tools
    (100 browser-only), 17 examples, and 22 public modules;
  - reliability matrix: 6 scenarios across 4 targets;
  - public read-only adapter inventory: 5 adapters;
  - Web IR corpus: 8 fixtures, 8 scenarios, and 11 categories with live
    evidence verification.

After all validation completed, process and open-file checks found no consumer
of `/tmp/glass-100-target` or the clean-install temporary root. The validated
non-symlink target resolved to `/tmp/glass-100-target` and measured 9.0G; the
release-documentation report measured 160K. Both are disposable certification
outputs and are removed in the final cleanup step. The repository target and
other project-owned build outputs are not part of this cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
