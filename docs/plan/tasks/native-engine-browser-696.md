---
id: native-engine-browser-696
scope: native-engine/browser/rcdata-fragment-context
status: done
depends-on: [native-engine-browser-695]
---

# Objective

Initialize HTML fragment tokenization from `title` and `textarea` context,
including SVG `title`, so RCDATA content becomes text and tokens after the
appropriate context end tag resume ordinary fragment parsing.

## Contract

- `innerHTML` fragment contexts named `title` or `textarea` begin in RCDATA;
  this includes an SVG `title` context.
- Before the first appropriate context end tag, markup-looking input remains
  text, character references are decoded, and U+0000 becomes U+FFFD. If the
  matching end tag is absent, the entire input remains RCDATA text.
- The appropriate `</title>` or `</textarea>` ends the initial context state.
  The context target is not a synthetic child or an open element to pop; the
  end tag itself is consumed as the fragment-context terminator. Remaining
  input is parsed with the existing fragment insertion and namespace rules.
- Rust `innerHTML` commit and same-turn JavaScript/frame projection expose the
  same immediate and committed tree. Context initialization must not alter
  full-document SVG `title` integration-point behavior or XHR document parsing.
- This is a fragment-tokenizer increment, not a claim of general HTML parser
  conformance or completion of issue #40.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-689.md`
- `docs/plan/tasks/native-engine-browser-690.md`
- [WHATWG HTML fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-696.md`

## Verification

- Cover SVG `title`, HTML `title`, and HTML `textarea` fragment contexts.
- Assert text before a close is not parsed as elements, character references
  decode, nulls become U+FFFD, and EOF without a close preserves all input as
  text.
- Assert an appropriate end tag is consumed without popping/replacing the
  context target and that following markup resumes normal namespace and
  parentage rules.
- Compare same-turn projection with the committed Rust tree; retain the
  existing full-document SVG-title and XHR parser regressions to prove those
  routes are unchanged.
- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before the
  focused test batch. Run the exact fragment regression and the scoped HTML
  parser batch, excluding the separate user-owned selector draft test.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut validators. Record remote CI separately; do not claim
  browser conformance or issue #40 completion.

## Outcome

Fragment parsing now initializes RCDATA for `title` and `textarea` context,
including SVG `title`. The matching context end tag is consumed without
creating or popping a context element, and following markup resumes normal
fragment insertion and namespace selection. Rust commits and same-turn
JavaScript/frame projections agree. Internal foreign-markup reparsing and
full-document/XHR behavior retain their previous tokenization.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- The focused `html_` parser batch — 35 passed, excluding the separate
  user-owned CSS-selector draft test.
- The `foreign_` parser batch — 9 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Release documentation truth validated 1,324 Markdown documents, 83 current
  documents, 63 previous-version hits, 1,452 semantic audit hits, and zero
  current-claim failures; depth covered 93 guides and 19 contracts; shortcut
  validation covered 15 implementation keys and 63 markers.
- Local validation only. Remote CI was not run; issue #40 remains open.
