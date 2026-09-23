---
id: native-engine-browser-694
scope: native-engine/browser/foreign-content-end-tag-dispatch
status: done
depends-on: [native-engine-browser-693]
---

# Objective

Implement the WHATWG foreign-content algorithm for ordinary end tags across
document parsing, Rust fragment commit, same-turn JavaScript fragment
projection, and XHR HTML-document parsing.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-693.md`
- [WHATWG HTML foreign-content parsing](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inforeign)

## Contract

- Apply foreign end-tag dispatch when the current open element is in a
  non-HTML namespace, including SVG and MathML integration-point elements.
- For ordinary end tags other than the `br` and `p` breakouts already handled
  by slice 693, walk the open-element stack from current toward its root. Match
  element local names ASCII-case-insensitively. If a matching non-root element
  is found before an HTML element, pop through the match and consume the token.
- If the walk reaches an HTML element first, reprocess the token through the
  active HTML insertion-mode recovery. If it reaches the stack root first,
  consume the unmatched token without popping the root. A foreign fragment
  target is a root sentinel and must remain the fragment insertion parent.
- Preserve HTML elements encountered at integration points as HTML boundaries;
  do not let a foreign element with the same local name satisfy an HTML
  end-tag target after reprocessing.
- Keep the existing `</br>` and `</p>` breakout behavior, table-mode consumers,
  formatting recovery, and self-closing start-tag behavior unchanged.
- Document parsing, committed fragments, same-turn JavaScript projection, and
  XHR HTML documents must produce identical namespace and parentage results.
- This slice does not claim complete foreign-content, tokenizer, or HTML parser
  conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-694.md`

## Verification

- Cover a matching foreign end tag that closes nested foreign descendants,
  an unmatched end tag that leaves the foreign stack intact, an HTML end tag
  that reprocesses at an HTML ancestor, and matching/nonmatching integration
  point tags.
- Cover a foreign fragment root whose own end tag and unrelated end tags must
  not remove the target or change subsequent child namespaces.
- Compare exact namespace and parentage across direct document parsing, Rust
  fragment commit, same-turn JavaScript projection, and XHR HTML-document
  parsing; retain the slice 693 breakout regressions.
- Run the locked `glass-browser` library/test-target check before focused
  parser regressions, followed by the scoped foreign/HTML parser batch.
- Run formatting, whitespace, release-documentation truth, documentation-depth,
  and shortcut validators. Run full inventory/link coverage at a debug-binary
  checkpoint when its CLI inventories are in scope.
- Do not claim remote CI, release, publication, browser conformance, or issue
  #40 completion from this slice.

## Local evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib foreign_ --locked --quiet` passed 8/8.
- `cargo test -p glass-browser --lib html_ --locked --quiet -- --skip javascript_attribute_selectors_use_html_default_case_rules` passed 34/34; the skipped test is a separate user-owned selector draft.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Release-documentation truth passed for 1,322 Markdown files with zero
  current-claim failures; documentation depth passed for 93 guides/19
  contracts; shortcut validation passed for 15 implementation keys/63 markers.
- Full documentation inventory/link coverage was not run because this parser
  slice changed no CLI/MCP/module inventory and the debug CLI binaries were not
  built. Remote CI was not run. General browser/parser conformance and issue
  #40 completion remain open.
