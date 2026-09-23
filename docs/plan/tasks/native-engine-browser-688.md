---
id: native-engine-browser-688
scope: native-engine/browser/contextual-html-null-character-handling
status: complete
depends-on: [native-engine-browser-687]
---

# Objective

Implement context-correct U+0000 handling across document parsing, Rust
`innerHTML` commit, same-turn JavaScript fragment projection, and XHR
`responseType="document"` parsing:

- In ordinary HTML character data, ignore literal U+0000. In foreign SVG and
  MathML content, replace it with U+FFFD; honor SVG HTML integration points
  and MathML text/HTML integration points when selecting that behavior.
- In script/style RAWTEXT and title/textarea RCDATA, replace literal U+0000
  with U+FFFD.
- Replace literal U+0000 in HTML comments, bogus comments, and attribute values
  with U+FFFD.
- Resolve numeric character references for U+0000 to U+FFFD. Do not decode
  character references in comments or RAWTEXT.
- Resolve references against the original text before applying the tree-builder
  rule to literal NUL, so dropping NUL cannot join source characters and
  manufacture a character reference.

Keep the submitted-source byte accounting unchanged. Do not generalize this
slice to NUL in tag names, attribute names, doctypes, or unsupported tokenizer
states such as PLAINTEXT; do not claim full HTML tokenizer conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-687.md`
- [WHATWG HTML parsing](https://html.spec.whatwg.org/multipage/parsing.html),
  including tokenizer character states, in-body insertion, and foreign content

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-688.md`

## Verification

- Add a four-route fixture covering literal and referenced NUL, including an
  entity-looking sequence split by NUL, in ordinary
  HTML text, attributes, comments, script/style raw text, RCDATA, foreign SVG
  text, and HTML/MathML integration-point text.
- Require exact U+FFFD/ignored results, parentage, and text order through direct
  parsing, same-turn projection, XHR HTML documents, and committed fragments.
- Run the locked `glass-browser` library/test-target check before the targeted
  HTML parser test batch.
- Run formatting, diff whitespace, and maintainer documentation validators.
- Do not claim remote CI, publication, browser conformance, or completion of
  issue #40 from this slice.

## Outcome

Completed across direct document parsing, Rust fragment commit, same-turn
JavaScript projection, and XHR HTML-document parsing. The four-route fixture
checks literal and numeric-reference NUL, integration-point selection, comment
and RAWTEXT reference preservation, parentage, and an entity-looking sequence
split by literal NUL.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib html_ --locked --quiet -- --skip javascript_attribute_selectors_use_html_default_case_rules` — 25 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Maintainer docs gates — passed over 1,316 Markdown files, with zero current-claim failures; 93 current guides routed/audited, 19 substantive contracts, 346 MCP tools (101 browser-only), 17 examples, 22 public modules, and 15 implementation help keys/63 documentation markers.
- Local validation only. No remote CI or issue #40 completion is claimed.
