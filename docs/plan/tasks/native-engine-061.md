---
id: native-engine-061
scope: glass-browser/native-engine/inherited-word-break
status: complete
depends-on: [native-engine-060]
---

# Native bounded inherited word break

## Objective

Add a bounded inherited `word-break` flow property to the existing fixed-cell
text layout owner. Explicit local fixtures should be able to choose the
existing word-aware wrapping or deterministic character-boundary wrapping
without adding a second inline formatter, changing the semantic source text,
or importing font/Unicode line-breaking dependencies.

## Contract

The native CSS grammar accepts only `normal` and `break-all` for `word-break`.
The property is inherited through the existing DOM style walk and has `Normal`
as its initial value. `normal` preserves the current behavior: collapsed text
is placed by ASCII-space-separated words and only an over-wide word is split
at fixed-cell character boundaries. `break-all` permits the same fixed-cell
character-boundary split for every collapsed word when the remaining line
capacity cannot hold the next word.

Collapsed whitespace remains one logical ASCII separator. A separator is
attached to the next word only when it and at least one character of that word
fit on the current line; otherwise the current line is flushed and the next
word begins without a leading separator. This retains the existing
source-whitespace boundary policy while making the word's characters eligible
for deterministic line splitting.

`break-all` applies to the bounded collapsed `white-space: normal` and
`white-space: pre-line` paths. `white-space: nowrap` and `white-space: pre`
remain unwrapped, so `word-break` does not override those modes. The existing
`pre-wrap` fixed-cell chunking already splits authored runs at character
capacity; `break-all` adds no second behavior there. Text transformation,
word/letter spacing, text fragments, alignment, line-height, root overflow,
display-list projection, raster replay, and point hit testing continue to use
the same shared flow measurements and owners.

The source semantic text and accessible projection remain unchanged. Layout
text fragments contain the same presented characters, in the same order, but
may have different fragment boundaries, origins, line count, and flow height
under `break-all`. Every fragment still uses the existing fixed-cell advance;
no glyph metric or paint command changes are introduced.

Unsupported `keep-all`, `break-word`, CSS-wide keywords, malformed values, and
other word-breaking syntax are diagnosed through the existing sanitized CSS
diagnostic surface and do not replace the inherited computed value. The slice
does not implement Unicode line-breaking classes, CJK policy, grapheme-cluster
boundaries, hyphenation, `overflow-wrap`, bidi, writing modes, font metrics,
shaping, or browser conformance parity. Splitting is at existing Rust
character boundaries and remains a deterministic native fixture rule.

## Tradeoffs

- Character-breaking every word gives narrow fixed-cell fixtures an explicit
  way to keep lines filled, but it changes fragment boundaries and flow height
  compared with the default word-aware behavior.
- Reusing the current word/separator and fixed-capacity helpers preserves
  spacing, alignment, overflow, and hit-test ownership, but it intentionally
  cannot model browser Unicode line-breaking or grapheme policy.
- Leaving `pre`, `pre-wrap`, and `nowrap` on their established paths avoids
  conflicting whitespace semantics and keeps the patch small, but the
  property is not a complete cross-mode `word-break` implementation.
- Keeping the value in computed style rather than display commands avoids
  renderer changes and dependencies, but callers inspecting layout must treat
  `break-all` as a geometry-changing presentation choice.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, initial value, stylesheet/inline cascade, inheritance, child
  override, and invalid-value diagnostics are covered by unit tests;
- `normal` retains word-aware wrapping while `break-all` splits ordinary words
  at deterministic fixed-cell character boundaries;
- collapsed separators are not painted at wrapped line starts, and leading
  or pending separators follow the existing boundary policy;
- `white-space: pre`, `pre-wrap`, and `nowrap` retain their existing behavior;
- transformed text, spacing, text fragments, flow height/overflow, display
  projection, and semantic source text remain internally consistent;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation is committed locally as `479f3a3` and the synchronized
architecture, analysis, plan, and public capability documentation is ready for
the documentation closeout checkpoint. Local verification completed with:

- native integration tests: 76 passed, 0 failed;
- `cargo test -p glass-browser --features native-engine --lib --locked`: 851
  passed, 1 ignored, 0 failed;
- `cargo clippy -p glass-browser --all-targets --all-features --locked --
  -D warnings`: passed;
- `cargo clippy -p glass-browser --no-default-features --all-targets --locked
  -- -D warnings`: passed;
- `cargo build -p glass-dev --locked`: passed in 13m00s, confirming the
  feature-gated browser implementation still links through the existing
  two-crate workspace;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`:
  passed in 5m18s;
- version, feature-parity, release-documentation, documentation-depth,
  TUI-shortcut, reliability-matrix, public-readonly-adapter, and Web IR
  validators: all passed; the release-documentation validator found 0 current
  claim failures;
- documentation coverage: 475 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules;
- `cargo fmt --all -- --check` and `git diff --check`: passed.

The full workspace test wrapper was not rerun because this is a
feature-gated `glass-browser` native-engine slice; the browser library,
feature integration suite, both lint configurations, and `glass-dev` linkage
were exercised directly. Remote CI remains pending until the branch is
pushed. The final Cargo target cleanup is recorded in issue #40 after all
verification commands complete.
