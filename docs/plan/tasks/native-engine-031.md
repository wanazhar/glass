---
id: native-engine-031
scope: glass-browser/native-engine/overflow-clip
status: done
depends-on: [native-engine-030]
---

# Native bounded `overflow: clip`

## Objective

Accept the non-scrolling CSS `overflow: clip` value in the existing bounded
rectangular clipping path. The native engine already derives one clip
intersection for `overflow: hidden`; this slice makes the equivalent
non-scrolling value useful for local fixtures without introducing nested
scroll containers or a second overflow model.

## Contract

The supported `overflow` grammar accepts `hidden` and `clip`. Both values
contribute the element's layout rectangle to the same bounded ancestor clip
intersection consumed by paint, viewport rectangle projection, and point
hit-testing. `overflow: clip` never creates a scroll offset, changes root
scroll behavior, or implicitly scrolls a target into view.

The existing cascade, inline precedence, layout revision, display-list
revision, source-over replay, and semantic visibility/actionability rules are
unchanged. `visible`, `auto`, and `scroll` remain unsupported values and
continue to produce bounded CSS diagnostics. Axis-specific overflow,
independent overflow axes, nested scrolling, scrollbars, rounded descendant
clips, and general CSS overflow conformance remain outside this slice.

Parsing, diagnostics, clip traversal, and all derived consumers remain
bounded. Invalid values must not partially commit a declaration or mutate a
document.

## Tradeoffs

- Supporting `clip` removes a common false diagnostic while reusing the
  already-tested rectangular clip invariant, but it deliberately does not
  implement the scrolling behavior associated with `auto` or `scroll`.
- Treating `hidden` and `clip` as one internal clip bit keeps the renderer,
  viewport projection, and hit-testing paths convergent, but does not preserve
  a public distinction between the two computed overflow values.
- The slice improves local fixture coverage without adding a CSS dependency;
  axis-specific overflow and nested scroll ownership still require separate
  contracts and evidence.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- CSS parser/unit coverage for accepted `hidden` and `clip` values and
  rejected `visible`, `auto`, and `scroll` values;
- diagnostic coverage proving `overflow: clip` is accepted while unsupported
  overflow values remain reported;
- native integration coverage proving `clip` reaches paint, viewport
  projection, and point hit-testing with no scroll mutation;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the `feat(native-engine): support bounded overflow clip`
checkpoint. The native stylesheet path now accepts `overflow: clip` and maps
it to the same bounded non-scrolling rectangular clip consumed by layout,
viewport projection, point hit-testing, and software paint. Unsupported
overflow values remain diagnosed and no public computed-value distinction or
nested scroll owner was introduced.

- Focused overflow-clip integration: 1 passed.
- Native integration suite: 44 passed.
- Native unit suite: 36 passed.
- Strict default-feature and `native-engine` all-target Clippy gates pass with
  warnings denied.
- Full locked `glass-browser` all-target/all-feature matrix: 818 unit tests
  passed, 1 ignored; all integration and example targets passed, including 44
  native integration tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage, depth, release-truth, and feature-parity validators
  pass after task synchronization; current-claim failures remain at zero.
- No new dependency, third crate, nested scroll behavior, scrollbar,
  axis-specific overflow, stable transport capability, or browser-parity
  claim was introduced.
