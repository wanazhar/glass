---
id: native-engine-116
scope: glass-browser/native-engine/text-decoration-skip-spaces-line-edges
status: complete
depends-on: [native-engine-115]
---

# Native bounded text-decoration skip-spaces line edges

## Objective

Extend the existing inherited `text-decoration-skip-spaces` control with
explicit `start`, `end`, and `start end` values. The line-edge modes must skip
only the leading or trailing fixed-cell space intervals on a bounded horizontal
line, including their adjacent letter, word, and justification spacing, while
preserving the existing `none|all` behavior and the no-output-change omission
fallback.

## Context

The CSS Text Decoration Level 4 definition gives
`text-decoration-skip-spaces` the grammar `none | all | [ start || end ]`,
with `start` and `end` applying to spacers at the corresponding line edge. The
normative reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

The 115 slice already owns inherited `none|all` parsing, fixed-cell space
intervals, letter/word/final-line justification advances, decoration style,
thickness, offset, clipping, scrolling, opacity, capture, and immutable text
replay. Layout already groups text runs by line while flushing a flow. This
slice adds only explicit line-edge provenance to that existing path; it does
not add a second layout, decoration, or semantic owner.

## Contract

The native CSS grammar accepts case-insensitive `none`, `all`, `start`,
`end`, and an unordered pair `start end` or `end start`. `none` remains valid
only by itself, and `all` remains valid only by itself. Omission continues to
compute to `none` so existing output does not change. CSS-wide keywords,
duplicates, mixed `none` combinations, empty values, unknown values, and
other out-of-contract syntax remain bounded typed diagnostics without raw
stylesheet echo. The computed value uses one dedicated
`NativeTextDecorationSkipSpaces` value carried by the existing immutable
`TextRun` command.

For `start`, decoration pixels are suppressed only over the contiguous leading
ASCII-space intervals of a text run that is the first text item in its flushed
line. For `end`, decoration pixels are suppressed only over the contiguous
trailing ASCII-space intervals of a text run that is the last text item in its
flushed line. `start end` applies both rules. Each selected space interval
includes the preceding letter-spacing advance and the following fixed-cell,
letter-spacing, word-spacing, and final-line justification advances using the
same integer calculation as 115. Internal spaces and spaces in an interior
text run remain decorated under the edge-only modes. Adjacent selected
intervals may overlap and are treated as one skipped region.

Line-edge provenance is assigned by the existing flow flush from its ordered
`FlowItem` text ranges. It is carried as immutable metadata alongside the
display list's text-command sequence, not recomputed from pixels, x positions,
or a second geometry model. The bounded line is horizontal-tb and the current
fixed-flow line identity; this deliberately excludes atomic-inline boundaries,
ancestor decoration propagation, bidi reordering, vertical writing, and
cross-fragment line continuity.

The modes apply to underline, overline, and line-through because 115's
space-skip owner is independent of 114's underline/overline-only skip-ink
owner. Glyph pixels remain unchanged. Decoration color, pattern phase,
thickness, offset, origins, width, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their current owners.

## Tradeoffs

- Recording line edges during the authoritative flow flush avoids a fragile
  raster inference from x coordinates and preserves exact alignment/indent
  behavior, but it does not model line identity across separate immutable
  fragments or atomic inlines.
- Supporting both edge tokens and their unordered pair matches the bounded
  grammar while keeping the value finite and auditable; CSS-wide inheritance,
  Unicode White_Space, narrow no-break-space rules, and full typographic
  character units remain unsupported diagnostics.
- Keeping omission as `none` avoids a silent default-output change even though
  the standards draft lists `start end` as its initial value; callers must opt
  into edge skipping explicitly in this experimental backend.
- Reusing the 115 interval calculation makes letter/word/justification spacing
  deterministic, but it does not provide font metrics, shaping, bidi,
  antialiasing, or browser text-paint parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

The parser now accepts the bounded case-insensitive edge grammar and keeps
`none` as the omission fallback. The existing flow flush marks only the
authoritative block-owned first and last text items; nested inline temporary
flows cannot claim a line edge. Paint carries that provenance as immutable
sidecar metadata in text-command order, and raster replay applies the selected
leading/trailing interval suppression to every decoration line. The fixed-cell
spacing calculation is shared with the 115 `all` path, so letter, word, and
final-line justification advances remain covered without introducing another
geometry owner.

Implementation checkpoints are `a671a559` and `db7585f7`; synchronized
documentation is completed in this task and its architecture, analysis, and
plan README records. The implementation remains default-off and local-only.

## Verification

The implementation must provide parser/cascade coverage for case-insensitive
`none|all|start|end`, unordered `start end`, omission, inheritance, explicit
override, inline precedence, duplicate/mixed-value rejection, and bounded
diagnostic redaction. Native integration must prove leading-only, trailing-only,
and both-edge skipping across hard/soft bounded lines and preformatted spaces,
including letter/word/justification spacing; interior spaces and `none|all`
must retain their established behavior. Underline, overline, line-through,
solid, dashed, dotted, double, wavy, skip-ink, clipping, scrolling, opacity,
capture, semantic/source order, and layout geometry regressions must remain
green. Full native, feature-library, strict lint, warning-denied rustdoc,
locked package, dependency, offline fuzz, documentation, static, security,
and formatting gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

The focused CSS parser/cascade selection passed 2/2 tests; the focused native
decoration selection passed 2/2 tests; and the exact line-edge layout/raster
regression passed 1/1. The final serial native integration suite passed 153/153
tests. The feature-enabled browser library passed 914 tests with 1 ignored and
0 failures. The first native-library invocation had one existing cancellation
timing failure under parallel scheduling; the exact test passed on an isolated
serial rerun, and the required final serial library run was green.

The serial `glass-dev` suite passed 365 unit tests, 4 integration tests, 15
PTY tests, and 1 doctest. Workspace all-feature Clippy and no-default-feature
browser Clippy passed with warnings denied. Warning-denied workspace rustdoc,
the all-feature debug workspace build, native-enabled optimized
`glass-browser`, and optimized `glass-dev` binary builds all passed. The
optimized package builds measured 18m21s for native-enabled `glass-browser`
and 22m45s for `glass-dev`; the repository release profile uses `opt-level=z`,
thin LTO, and one codegen unit. Cargo also emitted its existing non-fatal
duplicate `glass-browser`/`.dwp` output-name collision when the workspace was
built; package-specific builds completed without treating that warning as a
failure.

Locked browser packaging and its registry-backed publish dry-run passed. The
dev archive was produced and its local `--no-verify` publish dry-run passed;
registry-backed package/publish verification failed only because immutable
published `glass-browser 0.3.14` lacks the current `BrowserRuntime`,
`browser_runtime`, and `browser_endpoint` API required by `glass-dev 0.3.14`.
The packaged dependency validator still resolved `glass-browser` exactly at
0.3.14. No upload was attempted. The known yanked `chacha20 0.10.1` lockfile
warning remained non-fatal.

Locked fuzz fetch plus offline all-target checking passed in 8m32s. `cargo
deny check` passed with its existing duplicate-dependency warnings, and
`cargo audit` passed with the repository's four allowed warnings. Formatting
and diff checks passed. The final static audit passed with 530 Markdown files,
83 current-version hits, 57 previous-version hits, 610 semantic hits, and
zero current-claim failures; documentation coverage passed with 345
full-product MCP tools, 100 browser-only tools, 17 examples, and 22 public
modules; TUI passed 15/63, depth 93/19, reliability 6/4, adapters 5, and Web
IR 8/8/11. Coverage used the freshly validated isolated binaries from
`/tmp/glass-116-target` because the repository target was intentionally kept
empty.

## Cleanup

The final inventory found 149 directories and 10 files under the exact
`/tmp/glass-*`/`/tmp/forgebuild-*` test-output pattern totaling
16,211,804,369 bytes, including the 16,211,626,970-byte
`/tmp/glass-116-target`, the 170,095-byte release-documentation report, and
the 1,418-byte adapter report. No Cargo, rustc, rustdoc, Clippy, fuzz,
rust-analyzer, or Glass test writer and no open descriptor referenced a
candidate. Bounded `find -P ... -xdev -depth -delete` removed the exact
inventory; a fresh scan found no matching roots or reports. The repository
`target` and `fuzz/target` contain no generated build output, and the
filesystem returned to 80 GB available at 59% use. Three pre-existing Glass
processes were preserved; shared Cargo registries, toolchains, source,
durable data, and other projects were not touched.
