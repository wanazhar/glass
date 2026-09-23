---
id: native-engine-browser-689
scope: native-engine/browser/html-integration-point-child-namespaces
status: complete
depends-on: [native-engine-browser-688]
---

# Objective

Choose namespaces for parsed child elements according to the HTML tree
construction dispatcher rather than blindly inheriting the parent namespace.
Keep document parsing, Rust `innerHTML` commit, same-turn JavaScript fragment
projection, and XHR HTML-document parsing in agreement.

- SVG `foreignObject` and `desc` children use HTML rules. Within those rules,
  `svg` and `math` start tags still enter SVG and MathML respectively.
- MathML text integration points (`mi`, `mo`, `mn`, `ms`, `mtext`) use HTML
  rules for child start tags except `mglyph` and `malignmark`, which remain
  MathML.
- MathML `annotation-xml` with ASCII-case-insensitive `encoding="text/html"`
  or `encoding="application/xhtml+xml"` uses HTML rules for child start tags.
  A `svg` start tag inside any MathML `annotation-xml` enters SVG.
- Outside those integration cases, child elements in SVG and MathML retain the
  foreign parent namespace. When HTML rules apply, `svg` and `math` start tags
  enter their respective namespaces.

This slice corrects element namespace selection only. It does not implement a
complete foreign-content dispatcher, insertion-mode reprocessing, or general
HTML conformance. SVG `title`'s context-sensitive RCDATA tokenizer behavior is
tracked separately; the existing NUL slice still applies its character-data
integration-point rule there.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-688.md`
- [WHATWG HTML parsing](https://html.spec.whatwg.org/multipage/parsing.html),
  especially the tree-construction dispatcher and MathML/HTML integration-point
  definitions

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-689.md`

## Verification

- Add a single exact-tree fixture with direct namespace and parent assertions
  across all four parser routes. Cover SVG `desc`/`foreignObject`, MathML text
  integration points and `mglyph`/`malignmark` exceptions, both HTML
  `annotation-xml` encodings, the `annotation-xml`-`svg` exception, and ordinary
  foreign SVG/MathML children.
- Run the locked `glass-browser` library/test-target check before the focused
  HTML integration-point tests.
- Run formatting, diff whitespace, and maintainer documentation validators.
- Do not claim remote CI, publication, browser conformance, or completion of
  issue #40 from this slice.

## Outcome

The four parser routes now select child namespaces consistently at the covered
SVG and MathML integration points. Exact namespace and parentage assertions
cover SVG `foreignObject`/`desc`, MathML text points and their exceptions, both
HTML `annotation-xml` encodings, the `annotation-xml` SVG exception, and
ordinary foreign descendants.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- The focused integration-point route test — 1 passed.
- The focused `html_` parser batch — 26 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Maintainer docs gates — passed over 1,317 Markdown files, with zero
  current-claim failures; 93 current guides routed/audited, 19 substantive
  contracts, 346 MCP tools (101 browser-only), 17 examples, 22 public modules,
  and 15 implementation help keys/63 documentation markers.
- Local validation only. No remote CI or issue #40 completion is claimed.
