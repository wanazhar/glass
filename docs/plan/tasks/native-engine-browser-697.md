---
id: native-engine-browser-697
scope: native-engine/browser/rawtext-fragment-context
status: done
depends-on: [native-engine-browser-696]
---

# Objective

Initialize HTML fragment parsing in RAWTEXT for `style`, `xmp`, `iframe`,
`noembed`, and `noframes`, plus `noscript` when scripting is enabled. Preserve
literal source text and resume ordinary fragment parsing after the appropriate
context end tag.

## Contract

- The listed HTML fragment contexts begin in RAWTEXT. Markup-looking source
  before the first appropriate context end tag remains text; character
  references are not decoded, and U+0000 becomes U+FFFD. If no matching end
  tag exists, all remaining input stays text.
- Consume the matching context end tag without inserting or popping the
  fragment target. Parse any suffix with the ordinary fragment insertion and
  namespace rules.
- Apply the same RAWTEXT element set to document/XHR tokenization, fragment
  commits, same-turn JavaScript/frame projections, and HTML serialization.
  `noscript` uses RAWTEXT because this browser runtime has scripting enabled.
- Preserve foreign-content reprocessing for same-named SVG/MathML elements and
  existing RCDATA behavior. Script-data and `plaintext` fragment states remain
  separate follow-ups; this is not a general HTML conformance claim.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-696.md`
- [WHATWG HTML fragment parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-697.md`

## Verification

- Cover every listed context, literal markup and entities, U+0000 replacement,
  EOF without a closing tag, end-tag delimiter handling, and ordinary suffix
  parsing.
- Compare direct document parsing, Rust fragment commit, same-turn JavaScript
  projection, and XHR HTML-document parsing; verify serialization keeps
  RAWTEXT text literal.
- Retain foreign SVG/MathML namespace and integration-point regressions.
- Run the locked scoped package check before focused HTML and foreign parser
  batches. Exclude the separate user-owned selector draft test.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run documentation inventory/link coverage only
  when its debug inventory binaries are available; record any prerequisite
  failure without rebuilding solely for this non-final gate.
- Record remote CI separately. Do not claim full parser conformance or issue
  #40 completion.

## Outcome

HTML RAWTEXT initialization now covers `style`, `xmp`, `iframe`, `noembed`,
`noframes`, and scripting-enabled `noscript` across document parsing, Rust
fragment commit, same-turn JavaScript/frame projection, XHR HTML parsing, and
HTML serialization. Fragment-context RAWTEXT remains literal through the
appropriate end tag, replaces nulls with U+FFFD, consumes the context end tag
without popping the target, and resumes normal fragment insertion. Existing
foreign SVG/MathML reprocessing and RCDATA behavior remain unchanged.

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- The focused `html_rawtext` tests — 2 passed; the foreign namespace special
  text route regression — 1 passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Release-documentation truth validated 1,325 Markdown documents (83 current),
  63 previous-version hits, 1,454 semantic audit hits, and zero current-claim
  failures. Documentation depth covered 93 guides and 19 contracts; shortcut
  validation covered 15 implementation keys and 63 markers. Documentation
  inventory/link coverage was not run because
  `target/debug/glass` and `target/debug/glass-browser` are absent; they were
  not rebuilt for this non-final gate.
- Local validation only. Remote CI was not run; script-data and `plaintext`
  fragment states, general parser conformance, and issue #40 remain open.
