---
id: native-engine-browser-699
scope: native-engine/browser/script-data-fragment-context
status: complete
depends-on: [native-engine-browser-698]
---

# Objective

Initialize HTML `script` fragment contexts in Script Data and recognize an
appropriate `</script>` only when the tokenizer's Script Data, escaped, or
escaped-end-tag states emit it. A `</script>` seen in double-escaped content
must remain text and only transition back to the escaped state.

## Contract

- Preserve script source literally: no character-reference decoding; replace
  U+0000 with U+FFFD and retain preprocessed newlines.
- Follow Script Data, escaped, and double-escaped transitions needed to locate
  the actual appropriate end tag. Matching is ASCII case-insensitive and
  requires the HTML tag-name boundary. A double-escaped `</script>` is text;
  after it changes the state, a later appropriate end tag closes the script.
- The same close boundary and text must appear in direct documents, Rust
  fragment commits, same-turn JavaScript/frame projection, and XHR HTML
  documents. In a script fragment context, source after the actual close is
  parsed as the remaining fragment; EOF without a close leaves the source
  literal.
- Preserve RAWTEXT, RCDATA, PLAINTEXT, and foreign SVG/MathML behavior. This
  is not a general HTML tokenizer or script-execution conformance claim.
- Keep issue #40 open until its full browser profile and native-only gates are
  satisfied.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [`native-engine-browser-698`](native-engine-browser-698.md)
- [WHATWG HTML Script Data tokenizer states](https://html.spec.whatwg.org/multipage/parsing.html#script-data-state)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-699.md`

## Verification and result

- Cover ordinary closes, case-insensitive names and boundaries, invalid
  prefixes, escaped closes, double-escaped `</script>` followed by a later
  real close, EOF without a close, literal references, and U+0000.
- Compare document parsing, Rust `innerHTML` commit, same-turn JavaScript/frame
  projection, and XHR HTML parsing. Assert exact text, parentage, suffix
  construction, and serialization.
- Run the locked scoped package check before targeted script-data and related
  fragment/foreign regressions. Exclude the separate user-owned selector
  draft test.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run inventory/link coverage only when its debug
  binaries are available; do not rebuild solely for this non-final gate.
- Record remote CI separately. Do not claim full script execution,
  general parser conformance, or issue #40 completion.

Implementation and focused checks passed locally:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --tests --locked --quiet`
- `cargo test -p glass-browser --lib html_script_data --locked --quiet`
  (2 passed, including document/XHR routes, fragment commit, same-turn main
  document projection, and same-origin frame snapshot)
- `cargo test -p glass-browser --lib fragment_context --locked --quiet`
  (6 passed)
- `cargo test -p glass-browser --lib html_special_text_modes_respect_element_namespace_across_parser_routes --locked --quiet`
  (1 passed)
- Release-documentation truth, documentation-depth, TUI-shortcut, and
  whitespace checks passed.
- Documentation inventory/link coverage was skipped because no executable
  `glass` or `glass-browser` debug binary exists at `target/debug`.

The Rust fragment tokenizer now initializes Script Data from a `script`
context; the XHR parser and same-turn parser use the same escaped and
double-escaped boundary behavior. Script `textContent` is reconstructed from
the exact node snapshot for main and frame projections instead of reusing the
collapsed locator text. No general tokenizer or script-execution conformance
is claimed. Remote CI was not run; slice 699 does not complete issue #40.
