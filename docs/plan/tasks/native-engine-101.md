---
id: native-engine-101
scope: glass-browser/native-engine/text-align-justify
status: complete
depends-on: [native-engine-100]
---

# Native bounded text justification

## Objective

Add a bounded inherited `text-align:justify` value to the native engine's
existing fixed-cell inline-flow owner. Expand only eligible collapsed ASCII
word separators on lines ended by soft wrapping, while keeping the existing
physical/logical alignment values, source order, and no-bidi/shaping boundary
intact.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-100.md`
- [CSS Text Module Level 3: text-align](https://www.w3.org/TR/css-text-3/#text-align-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the additional inherited value
`text-align:justify`. The established fixed-cell default remains physical
left alignment when the declaration is omitted or unsupported. Existing
`left`, `center`, `right`, `start`, and `end` values retain their current
behavior.

For eligible horizontal-tb fixed-cell flow:

- only `white-space:normal|pre-line` with `word-break:normal` participates;
  `pre`, `pre-wrap`, `nowrap`, `break-all`, ellipsis, and unsupported text
  modes remain unchanged and fail closed where already required;
- a line is justified only when it ended because a subsequent collapsed word
  could not fit and the line contains at least one emitted ASCII space between
  supported text fragments; the final line, an empty line, a line terminated by
  `<br>` or a source newline, and a line with no positive free space are not
  stretched;
- positive remaining line width is distributed across eligible separators in
  deterministic source order using integer division and a bounded remainder;
  earlier separators receive the remainder pixel, and the sum of added space
  advances never exceeds the line's available width;
- existing `word-spacing` remains part of each separator's base advance and
  `letter-spacing` is unchanged; justification adds a separate bounded
  per-space advance rather than rewriting the authored style;
- direct text, `display:contents` descendants, and already-supported inline
  item subtrees remain in source order. Any shifted later item, box, text run,
  display-list command, raster glyph, viewport projection, overflow result,
  root scroll, capture, and semantic/source-order consumer uses the same
  expanded geometry owner;
- direction is not used to reorder text or inspect strong characters. The
  existing inherited `direction:ltr|rtl` state remains available to
  `start|end`, while justified spacing follows the bounded source-order model.

The slice is intentionally not a general inline-formatting implementation.
Unicode bidi resolution, glyph shaping, mixed-direction runs, tabs, CJK or
language-specific line breaking, `text-align:match-parent|justify-all`,
`text-justify`, logical properties, vertical writing modes, fractional or
font-relative metrics, hyphenation, and browser-wide CSS conformance remain
outside the contract.

## Tradeoffs

- Recording the added separator advance on the immutable text/display path
  keeps layout, paint, raster, overflow, and capture numerically aligned; it
  costs one bounded field per text run and a small raster branch.
- Applying justification only to soft-wrapped non-final lines avoids silently
  stretching short paragraphs or hard-break lines, but it does not claim the
  full CSS last-line and `text-align-last` matrix.
- Source-order remainder allocation is deterministic and easy to audit, but
  it is not visual-order distribution for bidi or shaped text; those features
  remain explicitly excluded rather than being approximated.
- Limiting participation to collapsed normal/pre-line spaces preserves the
  current whitespace model and prevents preformatted literal spacing from
  being rewritten. Wider whitespace and language rules require a separate
  contract with new evidence.
- No new crate, dependency, renderer, or mutable geometry owner is introduced;
  the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation checkpoint: `8ff29aa1` (`feat(native-engine): support
justified text flow`); test checkpoint: `15cf0c85` (`test(native-engine): cover
justified word spacing`); design checkpoint: `959cbbc9`. The focused and full
local gates passed against the isolated `CARGO_TARGET_DIR=/tmp/glass-101-target`
build root:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- CSS `text-align` parser/cascade tests: 3 passed, 889 filtered;
- focused justification integration tests: 2 passed, 136 filtered, including
  deterministic remainder allocation, hard/preformatted/break-all exclusion,
  shared display-list/raster geometry, and authored `word-spacing` composition;
- full native integration suite: 138 passed, 0 failed;
- feature-enabled browser library suite with `RUST_MIN_STACK=8388608`: 891
  passed, 1 existing ignored, 0 failed;
- strict all-feature workspace Clippy passed in 14m19s; the final test-only
  checkpoint rerun passed incrementally in 14.92s;
- strict no-default-feature browser Clippy passed in 6m08s; the final
  test-only checkpoint rerun passed incrementally in 2.03s;
- warning-denied workspace rustdoc passed in 3m55s;
- locked `glass-dev --bins` build passed in 11m49s;
- locked browser and dev packages passed with the local browser patch, and the
  packaged dependency validator confirmed `glass-browser` exactly at 0.3.14;
  packaging emitted only the existing yanked `chacha20 v0.10.1` warning;
- locked fuzz fetch and offline all-targets check passed in 8m49s;
- static validators passed: version and feature parity at 0.3.14; release
  documentation at 515 Markdown documents, 83 current documents, 57
  previous-version hits, 580 semantic audit hits, and 0 current-claim
  failures; TUI inventory 15/63; documentation depth 93/19; coverage
  515/345/17/22; reliability 6/4; public adapters 5; Web IR 8/8/11.

After all validation completed, process and open-file checks found no consumer
of `/tmp/glass-101-target`. The validated non-symlink target resolved to
`/tmp/glass-101-target` and measured 5.9G; the release-documentation report
measured 160K. The disposable target, report, and session logs were removed
with bounded same-filesystem deletion. No `/tmp/glass-*-target` or
`glass-clean-install.*` roots remain; the repository and ForgeBuild targets
are each 4.0K, `fuzz/target` is absent, and disk usage changed from 59G
available / 70% used to 65G available / 67% used. Shared registries,
toolchains, source, and durable project data were preserved.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
