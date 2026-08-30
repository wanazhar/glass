---
id: native-engine-006
scope: glass-browser/native-engine/html-parser
status: done
depends-on: [native-engine-005]
---

# Native raw-text and RCDATA parser handling

## Objective

Harden the bounded Phase 2 HTML parser so markup-looking content in raw-text
and RCDATA elements cannot become executable-looking or semantic DOM nodes:

- consume `script` and `style` contents as raw text until their matching
  closing tag;
- consume `title` and `textarea` contents as RCDATA until their matching
  closing tag, while retaining the existing character-reference decoding;
- treat an unclosed raw-text/RCDATA element's remainder as text within the
  document limit; and
- prove that parser state remains deterministic and that fake controls inside
  these elements do not enter semantic projection.

This remains a small, Glass-owned parser slice. It is not an HTML5
conformance claim and does not add JavaScript, CSS, layout, or script
execution.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-005.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

After a `script`, `style`, `title`, or `textarea` start tag, the tokenizer
searches case-insensitively for that element's matching end-tag boundary.
Markup-looking bytes before that boundary remain one text node. `script` and
`style` content is not character-reference decoded; `title` and `textarea`
retain the existing text decoding behavior. If no matching end tag exists,
the remaining bounded source is consumed as text and no nested elements are
created. Existing visible-text filtering still omits `head`, `script`,
`style`, `template`, and `title`; `textarea` remains a semantic textbox.

The slice does not implement insertion modes, adoption-agency recovery,
foreign content, document encodings, CSS parsing, JavaScript execution, or
HTML5 conformance reporting.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
