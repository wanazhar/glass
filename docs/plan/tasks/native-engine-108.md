---
id: native-engine-108
scope: glass-browser/native-engine/text-decoration-line
status: complete
depends-on: [native-engine-107]
---

# Native bounded text-decoration-line longhand

## Objective

Expose the existing fixed-cell decoration-line bitset through the
`text-decoration-line` longhand. The longhand must share the current
immutable text command, fixed-pixel decoration owner, and artifact consumers
without introducing a second line-state or geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-106.md`
- `docs/plan/tasks/native-engine-107.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-decoration-line` with the same bounded
line tokens already accepted by the `text-decoration` shorthand:
`none`, `underline`, `overline`, and `line-through`. The three line tokens may
appear once each in any order; `none` is valid only by itself. Matching is
case-insensitive through the existing parser. The longhand maps directly to
the existing `TextDecorationValue` bitset.

Within one declaration list, `text-decoration` and
`text-decoration-line` share one declaration-order-aware line-state slot, so
the last supported declaration wins. Across stylesheet rules they retain the
existing specificity, rule-order, and inline precedence. An explicit `none`
clears the current inherited line state. When the longhand is omitted, the
existing inherited fixed-cell line state remains unchanged. This is a bounded
alias inside the current inherited decoration owner; full CSS distinctions
between computed longhand inheritance and decoration propagation are not
claimed.

The selected bits continue through the existing immutable `TextRun`, clipping,
root scrolling, opacity replay, capture, hit testing, semantic/source order,
and software raster paths. The longhand changes only which of the existing
underline, overline, and line-through pixels are emitted. It does not change
text width, line formation, wrapping, alignment, overflow geometry, color,
thickness, style, offset, font metrics, shaping, bidi, writing mode, or
accessibility projections.

CSS-wide keywords, unknown tokens, duplicate line tokens, mixed `none` values,
`text-decoration-style`, `text-decoration-thickness`, shorthand color
components, and other unsupported syntax remain bounded typed diagnostics
without raw stylesheet echo. The bounded implementation remains integer-pixel,
fixed-cell, horizontal-tb, fixture-first, and default-off behind the existing
`native-engine` feature.

## Tradeoffs

- Normalizing both declarations into the current line bitset keeps one source
  of truth for line flags and makes declaration order observable without
  duplicating cascade or paint state.
- Reusing the existing parser provides combinations and rejection behavior
  already covered by the shorthand, but intentionally does not add the full
  CSS longhand inheritance/propagation model.
- Keeping the display command and raster geometry unchanged limits regression
  surface and preserves all 107 color behavior, at the cost of not modeling
  independent decoration-origin propagation.
- An explicit `none` remains meaningful and clears inherited bits, while an
  omitted longhand preserves the existing inherited state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Implementation

Designed in `ea1bf881` (`docs(native-engine): define decoration line slice`)
and implemented in `4981ff82` (`feat(native-engine): support decoration line
longhand`). The parser recognizes `text-decoration-line` as a bounded alias of
the existing shorthand line grammar, preserves declaration order between both
properties, and carries the resulting bitset through the existing immutable
text command and fixed-pixel decoration raster path. The native integration
fixture proves combined overline and line-through flags and pixels while
preserving the separate 107 decoration-color channel. The unsupported-value
fixture proves typed diagnostic redaction for an invalid longhand value. No
crate, dependency, feature default, renderer, display-list owner, layout owner,
or artifact schema was added.

## Verification

The focused parser/cascade test passed 1/1; the focused decoration integration
test passed 1/1; and the focused diagnostic-redaction test passed 1/1. Full
native integration passed 145/145, and the feature-enabled browser library
passed 898 tests with 1 expected ignored test. The complete all-feature browser
package suite passed 900 library tests, 145 native integration tests, and all
other protocol, reliability, TUI, Web IR, workspace, and example targets.
The complete serial `glass-dev` suite passed 365 unit tests, 4 integration
tests, and 15 PTY tests. The serial run is the authoritative local result
because it avoids host-load contention between independent Pi-runtime startup
tests; no test was skipped or weakened.

The remaining local gates passed against the isolated
`CARGO_TARGET_DIR=/tmp/glass-108-target`:

- `git diff --check` and `cargo fmt --all -- --check`;
- all-feature workspace Clippy and no-default-feature browser Clippy with
  `-D warnings`;
- warning-denied workspace rustdoc with `--no-deps` (4m08s);
- locked `glass-dev` and `glass-browser` binary builds;
- locked browser and dev package validation, including the exact paired
  `glass-dev 0.3.14` dependency check;
- locked dependency fetch and nightly/offline fuzz checking (11m03s);
- version sync, feature parity, TUI, documentation-depth,
  release-documentation, public-readonly, reliability, Web IR, and fresh
  source-built documentation coverage validators.

The final static audit measured 522 Markdown files, 83 current-release
documents, 57 previous-version hits, 596 semantic hits, and 0 current-claim
failures. Fresh binaries covered 522 Markdown files, 345 full-product MCP
tools, 100 browser-only tools, 17 examples, and 22 public modules. The
reliability matrix covered 6 scenarios and 4 targets; adapter coverage was 5;
the Web IR corpus covered 8 fixtures, 8 scenarios, and 11 categories. The
only emitted warnings were the pre-existing fuzz-bin naming, manual README,
redundant homepage, and yanked-lockfile advisories.

Every gate used the isolated target named above with a recorded purpose.
Remote CI, browser parity, release, registry publication, complete CSS
decoration conformance, and a third crate remain outside this local task.

## Cleanup

The final cleanup ran after all local gates and documentation certification.
Before removal, the isolated target `/tmp/glass-108-target` measured 9.1G and
`/tmp/glass-108-release-documentation-final.json` measured 164K. The complete
top-level `/tmp/glass-*` inventory measured 815 entries and 9.15 GiB, while
the exact top-level `/tmp/forgebuild-*` inventory measured 8 files and 34.14
MiB. The inventory contained only `ubuntu`-owned regular files/directories;
no symlinks or non-user-owned entries were present.

The process check found no `cargo`, `rustc`, `rustdoc`, `clippy-driver`,
`cargo-fuzz`, or `rust-analyzer` process. `lsof -nP` found no open handle under
`/tmp/glass-*` or `/tmp/forgebuild-*`. Each exact top-level path was removed
with guarded `find -P ... -xdev -depth -delete`: 815 Glass entries and 8
ForgeBuild entries were deleted, with 0 failures and 0 skips. Shared Cargo
registries, toolchains, source, durable user data, and other projects'
non-regenerable artifacts were not removed.

Post-removal checks found no top-level `/tmp/glass-*` or
`/tmp/forgebuild-*`, no accessible temporary `target` directory, no
`/home/ubuntu/work/glass/fuzz/target`, and 4.0K repository and ForgeBuild
targets. Filesystem usage is 113G used, 80G available, 59% on `/`.
