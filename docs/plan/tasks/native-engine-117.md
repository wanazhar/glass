---
id: native-engine-117
scope: glass-browser/native-engine/text-decoration-skip-spaces-unicode
status: ready
depends-on: [native-engine-116]
---

# Native bounded text-decoration skip-spaces Unicode whitespace

## Objective

Extend the existing inherited `text-decoration-skip-spaces` implementation
from ASCII space intervals to the native engine's bounded Unicode whitespace
classification. Preserve the 116 line-edge provenance, fixed-cell geometry,
and the established `none|all|start|end|start end` behavior.

## Context

The 116 slice owns parsing, inheritance, line-edge provenance, immutable text
commands, and fixed-cell decoration replay. Normal and no-wrap text already
collapses Unicode whitespace through `char::is_whitespace()` before layout;
`pre`, `pre-wrap`, and the existing literal-text path retain source whitespace
inside fixed-cell text runs. This slice makes the same bounded classification
available to decoration skipping without adding a second whitespace or layout
owner.

The normative property reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-116.md`

## Contract

The existing finite value and cascade remain unchanged. Omission continues to
compute to the explicit no-output-change `none` fallback. `all` skips every
Unicode whitespace character present in an emitted fixed-cell text run.
`start`, `end`, and `start end` skip contiguous leading/trailing Unicode
whitespace only when the 116 block-owned line-boundary metadata selects that
run edge. Internal whitespace remains decorated under edge-only modes.

The classification is Rust's Unicode `char::is_whitespace()` property, which
includes ASCII whitespace, tab, and non-breaking-space characters that survive
the literal/preformatted path. A segment break is already consumed by the
bounded preformatted line-break owner and is not expected inside one emitted
run. Normal and no-wrap collapsing continue to emit the existing ASCII-space
representation, so their geometry and separator behavior do not change.

Each selected whitespace interval retains the existing fixed-cell advance and
letter-spacing boundary calculation. ASCII spaces continue to include their
word-spacing and justification advances; non-ASCII whitespace does not gain a
new word-spacing or tab-stop policy. Glyph pixels, text-run origins, line
formation, wrapping, alignment, overflow, hit testing, capture, scrolling,
opacity, semantics, source order, and all decoration styles remain owned by
their existing paths.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim CSS whitespace-mode conformance, Unicode line breaking, tab
stops, font metrics, shaping, bidi, vertical writing, antialiasing, browser
text-paint parity, or cross-fragment/atomic-inline continuity.

## Tradeoffs

- Reusing `char::is_whitespace()` keeps layout boundary detection and raster
  skip detection auditable and avoids a hand-maintained Unicode table, but it
  intentionally does not model CSS line-breaking or language-specific space
  classes.
- Classifying whitespace at replay preserves the 116 geometry and provenance
  owners, so literal tabs and non-breaking spaces can be skipped without
  changing layout; the tradeoff is that fixed-cell tab stops and real glyph
  metrics remain out of scope.
- Keeping ASCII word/justification spacing special preserves the established
  115/116 arithmetic, but does not invent browser word-spacing behavior for
  every Unicode whitespace character.
- Adding no dependency keeps the two-crate boundary and build profile stable,
  at the cost of retaining the deliberately narrow software renderer.

## Path

- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Verification

Focused tests must prove `all` skips internal tab and non-breaking-space
intervals, while `start`, `end`, and `start end` skip Unicode whitespace only
at the selected line edges. The regression must cover underline, overline,
and line-through replay, preserve glyph/geometry output, and retain the 116
ASCII behavior. Full native, feature-library, strict lint, warning-denied
rustdoc, locked package, dependency, offline fuzz, documentation, static,
security, and formatting gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

