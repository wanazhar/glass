---
id: native-engine-browser-690
scope: native-engine/browser/namespace-aware-html-special-text-modes
status: complete
depends-on: [native-engine-browser-689]
---

# Objective

Select HTML RCDATA/RAWTEXT handling from the element namespace, not just its
local name. `script`, `style`, `title`, and `textarea` in SVG/MathML content
must keep the tokenizer in data state so nested markup is tokenized and
inserted under the appropriate foreign-content or integration-point rules.
Preserve special-text behavior for the corresponding HTML elements across
document parsing, Rust fragment commit, same-turn JavaScript projection, and
XHR HTML-document parsing.

## Contract

- HTML `script`/`style` content remains RAWTEXT; HTML `title`/`textarea`
  content remains RCDATA, including existing NUL/entity behavior.
- Foreign SVG/MathML elements with those local names do not enter HTML special
  text modes. Their child markup remains tokens and receives namespace
  selection from the current foreign/integration context.
- SVG `title` children use the HTML integration-point rules; ordinary SVG and
  MathML foreign children retain their namespace.
- This slice does not claim general HTML tokenizer/tree-builder conformance or
  change script execution policy.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-689.md`
- [WHATWG HTML parsing](https://html.spec.whatwg.org/multipage/parsing.html),
  especially tokenizer states, the tree-construction dispatcher, and foreign
  content handling

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-690.md`

## Verification

- Add one exact-tree fixture spanning SVG `title`, `style`, and `script`, plus
  MathML `textarea`/`title`, with nested children and exact namespace/parent
  assertions across all four parser routes.
- Retain existing HTML `script`/`style` RAWTEXT and `title`/`textarea` RCDATA
  regression coverage.
- Run the locked `glass-browser` library/test-target check before focused
  special-text and HTML parser tests.
- Run formatting, diff whitespace, and maintainer documentation validators.
- Do not claim remote CI, publication, browser conformance, or completion of
  issue #40 from this slice.

## Outcome

HTML RAWTEXT/RCDATA modes now depend on the element namespace. HTML special
elements retain their established text behavior, while same-named SVG/MathML
elements parse nested markup under the foreign-content or integration-point
context across document parsing, Rust fragment commit, same-turn JavaScript,
and XHR HTML-document parsing.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- The exact four-route namespace/parentage regression — 1 passed.
- The focused `html_` parser batch, excluding the pre-existing user CSS-selector
  draft test — 27 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed after formatting.
- Maintainer documentation gates passed: 1,318 Markdown documents with zero
  current-claim failures; 93 guides and 19 contracts; 346 MCP tools (101
  browser-only), 17 examples, 22 public modules, and 15 shortcut help keys/63
  documentation markers.
- Local validation only. No remote CI, publication, or issue #40 completion is
  claimed.
