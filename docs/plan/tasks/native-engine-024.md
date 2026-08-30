---
id: native-engine-024
scope: glass-browser/native-engine/word-wrap
status: done
depends-on: [native-engine-023]
---

# Native bounded word-aware text wrapping

## Objective

Improve direct-text line breaking without importing a font or CSS layout
engine:

- use the existing bounded collapsed-text projection as the input stream;
- keep complete words together when the remaining fixed-width line can hold
  them;
- omit a leading separator when a word moves to a fresh line; and
- split an individual word by fixed character capacity only when that word is
  wider than a complete line.

This gives local fixtures predictable word boundaries while retaining the
023 source-order fragment and actual-flow-origin contract.

## Contract

After the existing bounded whitespace collapse, text is interpreted as words
separated by one ASCII space. A word plus its separator is placed on the
current line only when the complete sequence fits. Otherwise the flow is
flushed and the word starts at the next line without a leading separator. A
word wider than the full available line is split into fixed-width character
fragments. The containing element remains the style, clip, and display-list
owner for every fragment.

This policy is deterministic for the current integer `CHARACTER_WIDTH` model,
works with existing inline-box placement, and preserves source-order paint,
root-scroll translation, line-height floors, revision behavior, and the
default-off feature boundary. It does not add CSS properties or dependencies.

## Tradeoffs

- Word boundaries reduce surprising mid-word breaks for bounded fixtures, but
  the policy is not CSS `white-space`, `overflow-wrap`, or `word-break`.
- A too-wide word still splits by character so a single token cannot make
  layout unbounded; this is predictable but not browser typography.
- Separators are owned by the following word when they fit on a line, so a
  separator is dropped when the word wraps. Whitespace joining across DOM text
  nodes or inline descendants remains unsupported.
- The fixed character width remains cheap and reproducible, but misses font
  metrics, shaping, glyph advances, baselines, bidi, hyphenation, and Unicode
  line-breaking rules.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- `cargo fmt --all -- --check`
- focused word-wrap, long-word, mixed inline, source-order, clipping, and
  root-scroll tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. Direct text now treats the existing bounded
collapsed projection as ASCII-space-separated words, keeps complete words on a
line when they fit, drops a separator when a word wraps, and splits only
over-wide words by fixed character capacity. The 023 fragment origins and
source-order paint entries remain the sole placement path.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Focused word-wrap tests: 3 passed.
- Native integration tests: 34 passed.
- Native unit tests: 33 passed through the full all-target/all-feature matrix.
- Strict default-feature and `native-engine` Clippy gates pass.
- Full locked `glass-browser` all-target/all-feature matrix: 815 passed, 1
  ignored; all integration suites passed.
- Documentation coverage: 438 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
