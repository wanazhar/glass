---
id: native-engine-115
scope: glass-browser/native-engine/text-decoration-skip-spaces
status: design-ready
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

## Cleanup

Record the exact isolated target and report paths, sizes, process/open-file
checks, deletion counts, and post-removal filesystem state after
certification. Do not remove shared Cargo registries, toolchains, source,
durable user data, or other projects' non-regenerable artifacts.
