---
id: native-engine-browser-698
scope: native-engine/browser/plaintext-fragment-context
status: complete
depends-on: [native-engine-browser-697]
---

# Objective

Initialize `innerHTML` fragment parsing in the PLAINTEXT state when the target
context element is `plaintext`. Consume the entire input as literal text; no
closing tag returns tokenization to normal HTML parsing.

## Contract

- Every input character belongs to one text node. Markup-looking syntax,
  character references, and `</plaintext>` remain literal; U+0000 becomes
  U+FFFD and HTML newline preprocessing remains in effect.
- Rust fragment commit and same-turn JavaScript/frame projection expose the
  same text and serialized `innerHTML`. No element is created from source that
  resembles markup, including source after `</plaintext>`.
- Preserve existing RCDATA and RAWTEXT fragment contexts. This slice changes
  fragment-context initialization only; document/XHR `plaintext` start-tag
  tokenization is a separate parser-route task.
- This is one tokenizer-state increment, not general HTML parser conformance
  or completion of issue #40.

## Result (2026-09-24)

The fragment parser now starts `plaintext` targets directly in PLAINTEXT,
replaces U+0000, preserves character references and markup through EOF, and
normalizes CRLF/lone CR before inserting one text node. JavaScript projection
and Rust commit agree; HTML text serialization stays literal. The locked
package check passed, the exact PLAINTEXT test passed 1/1, and the focused
RCDATA/RAWTEXT/PLAINTEXT fragment-context group passed 6/6. Formatting,
whitespace, release-documentation truth, documentation depth, and shortcut
checks passed. Documentation inventory/link coverage was skipped because
`target/debug/glass` and `target/debug/glass-browser` were absent. Remote CI was
not run. Document/XHR `plaintext` start-tag parsing remains separate; issue
#40 remains open.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-697.md`
- [WHATWG HTML fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-698.md`

## Verification

- Set `innerHTML` on a `plaintext` element with markup, named character
  references, U+0000, `</plaintext>`, and markup-like suffix input.
- Assert the source is one literal text node, null replacement, no parsed
  descendants, literal serialization, and same-turn/Rust-commit parity.
- Run the locked scoped package check before the exact plaintext regression;
  retain the RCDATA/RAWTEXT parser regressions. Exclude the separate
  user-owned selector draft test.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run documentation inventory/link coverage only
  when its debug inventory binaries are available; record any prerequisite
  failure without rebuilding solely for this non-final gate.
- Record remote CI separately. Do not claim general parser conformance or
  issue #40 completion.
