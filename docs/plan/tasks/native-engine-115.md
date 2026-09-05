---
id: native-engine-115
scope: glass-browser/native-engine/text-decoration-skip-spaces
status: complete
depends-on: [native-engine-114]
---

# Native bounded text-decoration skip-spaces

## Objective

Expose a bounded inherited `text-decoration-skip-spaces:none|all` control
through the existing fixed-cell text-decoration path. `all` must interrupt
decoration replay over ASCII-space advances, including the already-resolved
word, letter, and final-line justification spacing attached to those advances;
`none` must preserve the established continuous decoration output.

## Context

The CSS Text Decoration Level 4 property is inherited and controls whether
decoration lines skip spacers. Its full grammar also includes `start` and
`end`; this slice deliberately keeps those boundary-sensitive modes as typed
unsupported diagnostics until a line-start/line-end contract exists. The
standards reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

The existing native text path already owns fixed-cell source text, word
spacing, letter spacing, final-line justification spacing, decoration style,
thickness, offset, clipping, scrolling, opacity, capture, and semantic/source
order. This slice adds one inherited value to that path rather than creating a
second geometry or decoration owner.

## Contract

The native CSS grammar accepts case-insensitive inherited `none|all` values.
Omission computes to `none` to preserve the current renderer's output.
`start`, `end`, CSS-wide keywords, unknown, empty, and other out-of-contract
values remain bounded typed diagnostics without raw stylesheet echo. The value
is represented by a dedicated `NativeTextDecorationSkipSpaces` and is carried
in the existing immutable `TextRun` command.

With `none`, every selected decoration pixel follows the existing line style,
thickness, offset, clipping, scroll, opacity, capture, and command replay
behavior. With `all`, an individual decoration pixel is suppressed when its
fixed-cell x offset lies in an ASCII-space interval emitted by that same text
run. The interval begins at the space's character start minus the preceding
letter-spacing advance (clamped to the run start) and ends after the space's
fixed-cell advance, its following letter-spacing advance, its word-spacing
advance, and any final-line justification advance. Adjacent space intervals
may overlap and are treated as one skipped region.

`all` applies to underline, overline, and line-through, as the bounded
decoration-space rule is independent of the 114 ink-only rule. Glyph pixels
remain owned by the text replay; only decoration writes are suppressed.
Non-space characters, the existing decoration pattern phase, decoration color,
thickness, underline offset, line origins, text width, wrapping, alignment,
overflow geometry, clipping, scrolling, opacity, capture, hit testing,
semantic projection, and source order continue to use their existing owners.

The slice remains integer-pixel, fixed-cell, ASCII-space-only,
horizontal-tb, fixture-first, default-off in the sense of no output change,
and local-only. It does not add Unicode whitespace classification, line-start
or line-end modes, cross-fragment space ownership, font metrics, shaping,
bidi, writing modes, antialiasing, or browser-wide text conformance.

## Tradeoffs

- Reusing the exact text advance calculation makes spaces and their authored
  spacing observable without geometry drift, but it does not model Unicode
  typographic character units or browser-specific spacing heuristics.
- Skipping the complete fixed-cell space interval gives deterministic clean
  gaps, but does not shape decoration endpoints around glyph contours or
  preserve continuity across separate immutable text runs.
- Supporting only `none|all` keeps cascade and diagnostics auditable, but
  leaves the standard's `start`/`end` controls for a later line-boundary slice.
- Applying `all` to line-through follows the bounded property-level space
  contract, while the 114 ink rule remains limited to underline/overline;
  complete ancestor-decoration propagation is still outside scope.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Verification

The implementation must provide parser/cascade coverage for case-insensitive
`none|all`, omission, inheritance, explicit override, inline precedence, and
unsupported `start`/`end` diagnostics. Native integration must prove that
`all` skips underline, overline, and line-through over ASCII-space intervals,
including word/letter/justification spacing, while `none` preserves the prior
output and non-space decoration remains unchanged. Existing skip-ink, solid,
dashed, dotted, double, and wavy behavior must remain green. Full native,
feature-library, strict lint, warning-denied rustdoc, locked package,
dependency, offline fuzz, documentation, static, security, and formatting
gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

Implementation checkpoint: `1c0bd484`. Synchronized documentation checkpoint:
`b739c7d0`. The focused skip-spaces parser, raster, and integration checks
passed 3/3, 1/1, and 1/1. The native-engine library passed 914 tests with one
ignored, and its integration suite passed 152/152. The all-feature
`glass-browser` library passed 915 tests with one ignored. Serial `glass-dev`
passed 365 unit tests, 4 integration tests, and 15 PTY tests.

Workspace strict Clippy, no-default-feature browser Clippy, warning-denied
workspace rustdoc, explicit debug builds, formatting, and `git diff --check`
passed. Locked browser and dev package archives were produced and the dev
archive's exact `glass-browser = 0.3.14` dependency was verified without path
or feature leakage. The browser registry-backed publish dry-run passed. The
dev registry-backed dry-run was intentionally not published and failed during
tarball verification because the already-published immutable `glass-browser
0.3.14` does not expose the current `BrowserRuntime`, `browser_runtime`, and
`browser_endpoint` APIs required by `glass-dev 0.3.14`; the dev no-verify
packaging dry-run passed. This is a public-registry compatibility blocker, not
a native-engine test failure, and no upload was attempted.

Offline fuzz-workspace all-target checking, `cargo deny`, `cargo audit` (with
the repository's existing allowed warnings), version/feature/TUI/depth/public
adapter/reliability/Web IR validators, release-documentation validation, and
documentation coverage passed. Final documentation counts were 529 Markdown
files, 83 current-version hits, 57 previous-version hits, 609 semantic hits,
and zero current-claim failures; coverage found 345 full-product MCP tools,
100 browser-only tools, 17 examples, and 22 public modules.

The first native-library invocation also reproduced the repository's existing
deep test-thread stack requirement; rerunning with
`RUST_MIN_STACK=8388608` passed. This is recorded so the default-stack failure
is not mistaken for a feature regression.

## Cleanup

The isolated target `/tmp/glass-115-target` grew to approximately 11.63 GB
during the complete certification pass. The exact cleanup inventory found 81
user-owned, non-symlink `/tmp/glass-*`/`/tmp/forgebuild-*` roots totaling
11,632,848,896 bytes, plus the generated
`target/.rustc_info.json` (4,096 bytes). No Cargo, rustc, rustdoc, Clippy,
fuzz, or rust-analyzer writer and no open file descriptor referenced any
candidate. Bounded `find -P ... -xdev -depth -delete` removed all 82 exact
outputs. The isolated target, report files, repository package output, and fuzz
target are absent; no matching temporary roots remain. Filesystem availability
returned to 80 GB (59% used). The active repository `target/debug` tree and
its three running Glass processes were preserved. Shared Cargo registries,
toolchains, source, durable data, and other projects were not touched.
