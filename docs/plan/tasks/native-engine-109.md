---
id: native-engine-109
scope: glass-browser/native-engine/text-decoration-style
status: complete
depends-on: [native-engine-108]
---

# Native bounded text-decoration-style

## Objective

Expose deterministic solid, dashed, and dotted presentation for the existing
fixed-cell decoration lines. The style must share the current immutable text
command, line origin, clipping, and software-raster owner without introducing
a second decoration geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-107.md`
- `docs/plan/tasks/native-engine-108.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the `text-decoration-style` longhand with one
case-insensitive token: `solid`, `dashed`, or `dotted`. The computed style is
bounded-inherited through the existing fixed-cell decoration owner and
defaults to `solid`. A declaration on a descendant replaces the inherited
style for that descendant's emitted text runs; omission preserves the existing
inherited style. Rule specificity, stylesheet order, and inline precedence
remain the existing cascade rules.

`solid` paints every pixel of each selected underline, overline, and
line-through run. `dashed` uses the existing integer pattern helper with a
one-pixel line width: three painted pixels followed by two skipped pixels.
`dotted` uses one painted pixel followed by one skipped pixel. Each pattern is
anchored at the emitted run's x origin and restarts for each immutable text
command. The style is carried beside the existing glyph/decoration colors and
line flags in `TextRun`; it changes only which existing line pixels are
emitted.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line y positions, color, thickness, offset, font metrics,
shaping, bidi, writing mode, accessibility projections, hit testing, capture,
opacity grouping, or source/semantic order. It remains integer-pixel,
fixed-cell, horizontal-tb, fixture-first, and default-off behind the existing
`native-engine` feature. `text-decoration` shorthand style components are not
added by this longhand slice.

`double`, `wavy`, CSS-wide keywords, unknown tokens, empty values, and other
unsupported syntax remain bounded typed diagnostics without raw stylesheet
echo. The bounded inherited style model is an experiment-local alias for the
existing line owner; full CSS decoration-origin propagation and longhand
conformance are not claimed.

## Tradeoffs

- Reusing `NativeBorderStyle` and its existing integer dash/dot helper keeps
  pattern semantics in one implementation and avoids a new renderer or
  dependency, but deliberately limits text styles to the three patterns that
  helper can represent.
- Carrying style in `TextRun` keeps glyphs, line flags, colors, origin, clip,
  scroll, opacity, capture, and raster replay on one immutable command. A
  separate decoration command would duplicate geometry and could drift.
- Anchoring each pattern at the run origin is deterministic and easy to test,
  but it does not model browser-wide decoration continuity across fragments,
  nodes, or line boxes.
- Treating the bounded style as inherited matches the current native line-state
  owner and makes descendant fixtures useful, at the cost of not claiming the
  full CSS decoration-origin/inheritance model.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Implementation

Designed in `93034cbf` (`docs(native-engine): define decoration style slice`),
implemented in `81069084` (`feat(native-engine): support decoration styles`),
and completed with the inherited-style integration fixture in `b8ae87dc`
(`test(native-engine): cover inherited decoration styles`). `NativeBorderStyle`
now supplies the defaulted bounded style value for the inherited native CSS
cascade. `text-decoration-style` accepts only case-insensitive `solid`,
`dashed`, and `dotted` tokens; unsupported values remain typed, bounded
diagnostics. The resolved value travels from the DOM style chain through one
immutable `TextRun` and is consumed by the existing one-pixel software
decoration loop. Solid paints every selected line pixel, dashed uses the
existing three-on/two-off integer pattern, and dotted uses one-on/one-off;
each immutable run starts its pattern at its own x origin.

The native integration fixture proves explicit solid/dashed/dotted output,
run-origin anchoring, inherited dashed style, descendant solid override,
display-command propagation, and unchanged line geometry. No crate,
dependency, feature default, renderer, layout owner, or artifact schema was
added.

## Verification

The focused parser/cascade test passed 2/2, the focused decoration integration
test passed 1/1, and the focused diagnostic-redaction test passed 1/1. Full
native integration passed 146/146. The final all-feature `glass-browser`
package suite passed 902 library tests with 1 expected ignored test, 146
native integration tests, all browser/protocol/reliability/TUI/Web IR/workspace
targets, and 4 doctests. The serial `glass-dev` suite passed 365 unit tests,
4 integration tests, and 15 PTY tests. Its first full run had one transient
rust-analyzer no-diagnostics result; the exact test rerun passed 1/1 and the
clean serial rerun passed 365/365, so no production failure remained.

The remaining local gates passed against the isolated
`CARGO_TARGET_DIR=/tmp/glass-109-target`:

- `cargo fmt --all -- --check` and `git diff --check`;
- strict all-feature workspace Clippy, strict no-default-feature browser
  Clippy, and warning-denied workspace rustdoc;
- locked browser and dev package validation, including the exact paired
  `glass-dev 0.3.14` dependency check and both publication dry-runs;
- locked offline fuzz-workspace all-target checking;
- version sync, feature parity, TUI shortcut, documentation-depth,
  release-documentation, public-readonly, reliability, Web IR, and fresh
  source-built documentation coverage validators;
- `cargo deny check` and `cargo audit`, with only the repository's existing
  duplicate-dependency, unmaintained/yanked advisory warnings.

The final static audit measured 523 Markdown files, 83 current-release
documents, 57 previous-version hits, 597 semantic hits, and 0 current-claim
failures. Fresh binaries covered 523 Markdown files, 345 full-product MCP
tools, 100 browser-only tools, 17 examples, and 22 public modules. The
reliability matrix covered 6 scenarios and 4 targets; adapter coverage was 5;
the Web IR corpus covered 8 fixtures, 8 scenarios, and 11 categories.

Every gate used the isolated target named above with a recorded purpose.
Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, wavy/double styles, and a third crate remain outside
this local task.

## Cleanup

After all local gates completed on 2026-09-04 UTC, the exact isolated target
`/tmp/glass-109-target` measured 12G. The final release-documentation report
`/tmp/glass-109-release-documentation-final.json` measured 164K. The complete
top-level `/tmp/glass-*` inventory measured 158 entries and about 12G; there
were no `/tmp/forgebuild-*` entries.

The inventory contained only `ubuntu`-owned regular files and directories:
there were no symlinks or non-user-owned entries. The process check found no
`cargo`, `rustc`, `rustdoc`, `clippy-driver`, `cargo-fuzz`, or
`rust-analyzer` process, and `lsof -nP` found no open handle under the
validated temporary paths. Each exact top-level Glass path was removed with
bounded `find -P ... -xdev -depth -delete`: 158 top-level paths were selected,
with 0 deletion failures. Shared Cargo registries, toolchains, source,
durable user data, and other projects' non-regenerable artifacts were not
removed.

Post-removal checks found no top-level `/tmp/glass-*` or
`/tmp/forgebuild-*` entries, no `/home/ubuntu/work/glass/fuzz/target`, an
8.0K repository target containing only regenerable `.rustc_info.json` metadata,
and a 4.0K ForgeBuild target. Filesystem usage is 113G used, 80G available,
59% on `/`.
