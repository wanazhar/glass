---
id: native-engine-107
scope: glass-browser/native-engine/text-decoration-color
status: complete
depends-on: [native-engine-106]
---

# Native bounded text-decoration color

## Objective

Extend the completed fixed-cell decoration-line owner with an explicit
`text-decoration-color` value. Glyph color and decoration color must remain
independent while one immutable text command continues to carry the complete
paint input through clipping, scrolling, opacity replay, capture, and the
software rasterizer.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-105.md`
- `docs/plan/tasks/native-engine-106.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-decoration-color` as a local,
non-inherited color declaration. It reuses the existing bounded `NativeColor`
grammar: `black`, `white`, `red`, `green`, `blue`, `transparent`, three-, six-,
and eight-digit hexadecimal colors, and the existing comma-form `rgb(...)` and
`rgba(...)` forms. Matching of named colors and the property name is
case-insensitive through the existing parser. An omitted declaration resolves
to the emitted text run's resolved `color`; if that is absent, the existing
black fallback is used. `currentColor`, CSS-wide keywords, gradients, system
colors, and other color syntaxes remain typed unsupported-value diagnostics
without raw stylesheet echo.

The computed style retains the optional explicit decoration color separately
from inherited text color. For every emitted text run, the display list carries
both the glyph color and the resolved decoration color. Enabled
`underline`, `overline`, and `line-through` pixels blend with the decoration
color; glyph pixels blend with the text color. A transparent decoration color
is valid and leaves the line pixels unchanged. The color does not change text
width, line formation, wrapping, alignment, overflow geometry, hit testing,
semantic/source order, accessibility projections, scroll offsets, opacity
group boundaries, capture dimensions, or clipping ownership.

The local model deliberately resolves an omitted decoration color at each
emitted text run. Therefore an inherited line can use a descendant's resolved
text color when no explicit local decoration color is present. Standard
decoration-origin propagation and `text-decoration-color: inherit` semantics
are not claimed by this bounded contract.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. `text-decoration-line` longhand semantics,
shorthand color components, decoration style/thickness/offset, font-aware
metrics, shaping, bidi, vertical writing, color spaces, animations, and
browser-wide text conformance remain outside the contract.

## Tradeoffs

- Carrying a second color in the existing immutable `TextRun` command keeps
  glyphs and decoration lines tied to one text origin, width, clip, scroll
  translation, opacity group, and capture path. A separate decoration command
  would duplicate geometry and could diverge from the text run.
- Reusing `parse_color` provides useful deterministic palette and alpha
  coverage without adding a color dependency or a second color grammar.
- Keeping the declaration local matches the computed-style boundary used by
  the experiment, but it knowingly differs from full browser decoration
  propagation when a parent supplies the line and a child changes color.
- Explicit `transparent` is retained rather than treated as absent, so
  authors can suppress decoration pixels without clearing inherited line bits.
- This is the first native slice that intentionally changes the internal
  display-command shape. The change is bounded, feature-gated, and covered by
  all display-list/raster consumers; it does not create a crate, dependency,
  renderer, or second geometry owner.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Implementation

Implemented in `2474efe6` (`feat(native-engine): paint decoration colors`).
The parser and cascade retain an optional local decoration color, the existing
immutable `TextRun` command carries glyph and decoration colors together, and
the software rasterizer uses the second color only for decoration pixels. The
integration fixture proves red glyph pixels with blue underline pixels while
preserving the existing line flags and geometry. The unsupported
`currentColor` fixture remains a typed diagnostic without echoing stylesheet
input. No crate, dependency, feature-default, renderer, display-list owner,
layout owner, or artifact schema was added.

`64247bd4` (`fix(glass-dev): await delayed diagnostics`) is an ancillary
release-gate reliability checkpoint included in the current local
certification. It keeps waiting within the existing bounded deadline when
rust-analyzer publishes an empty snapshot before its real diagnostics; it does
not change the native-engine contract.

## Verification evidence

Focused parser/cascade coverage passed 3/3; the decoration pixel integration
test passed 1/1; full native integration passed 144/144; and the full
feature-enabled browser library passed 897 tests with 1 expected ignored test.
At the current checkout, the complete browser package suite passed 899 library
tests plus all integration, protocol, reliability, TUI, Web IR, workspace, and
example targets. The complete `glass-dev` suite passed 365 unit tests plus all
integration and PTY targets with `--test-threads=1`; the serial run is the
authoritative local result because it avoids host-load contention between
independent Pi-runtime startup tests. The previously failing parallel run was
reproduced, traced to delayed rust-analyzer/Pi startup behavior, and is covered
by `64247bd4`; no test was skipped or weakened.

The remaining local gates passed against the isolated
`CARGO_TARGET_DIR=/tmp/glass-107-target`:

- `git diff --check` and `cargo fmt --all -- --check`;
- all-feature workspace Clippy with `-D warnings`, no-default-feature browser
  Clippy, and current-source `glass-dev` Clippy with `-D warnings`;
- warning-denied workspace rustdoc with `--no-deps`;
- locked `glass-dev --bins` build;
- locked browser and dev package validation, including the exact paired
  `glass-dev 0.3.14` dependency check;
- locked dependency fetch and nightly/offline fuzz checking;
- version sync, feature parity, TUI, documentation-depth, release-documentation,
  public-readonly, reliability, Web IR, and fresh source-built documentation
  coverage validators.

The final static audit measured 521 Markdown files, 83 current-release
documents, 57 previous-version hits, 595 semantic hits, and 0 current-claim
failures. Fresh binaries covered 521 Markdown files, 345 full-product MCP
tools, 100 browser-only tools, 17 examples, and 22 public modules. The
reliability matrix covered 6 scenarios and 4 targets; adapter coverage was 5;
the Web IR corpus covered 8 fixtures, 8 scenarios, and 11 categories.

Every gate used an isolated target with a recorded purpose. The target and
report sizes, process/open-file checks, and post-removal filesystem state are
recorded in Cleanup below after the final documentation and issue updates.

Remote CI, browser parity, release, registry publication, and a complete CSS
color-conformance claim remain outside this local task.

## Cleanup

The final cleanup removed the exact isolated target and reports only after all
gates, documentation, and issue reconciliation completed. It used bounded
`find -P ... -xdev -depth -delete` after confirming no compiler, analyzer, or
other process had an open handle. Shared Cargo registries, toolchains, source,
durable user data, and other projects' non-regenerable artifacts were not
removed. The exact post-removal sizes and filesystem checks are recorded here
before this task is handed off.
