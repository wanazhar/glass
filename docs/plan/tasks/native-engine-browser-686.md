---
id: native-engine-browser-686
scope: native-engine/browser/html-active-formatting-and-adoption-agency
status: complete
depends-on: [native-engine-browser-685]
---

# Objective

Implement active-formatting-list reconstruction and the HTML adoption-agency
algorithm as one tree-construction unit across all four native HTML parser
routes: `NativeDocument::parse`, Rust `innerHTML` commit, same-turn JavaScript
detached-fragment projection, and XHR `responseType="document"` parsing.

Maintain HTML formatting entries and markers, reconstruct detached entries
before relevant in-body insertion, apply the three-equivalent-entry Noah's Ark
rule, and handle formatting end tags (including nested anchors) with the
standard bounded outer/inner loop. Update both parser stacks and the formatting
list; preserve token-created attributes when cloning; move existing nodes rather
than serialize/reparse them; and apply the existing HTML namespace, scope,
fragment-root, DOM node-count, and depth boundaries. Marker insertion/clearing
must prevent formatting from leaking across applet/object/marquee/template,
caption, and table-cell contexts.

This slice does not claim complete HTML tokenization, insertion-mode, or Core
Web Profile conformance.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-685.md`
- [WHATWG active-formatting elements](https://html.spec.whatwg.org/multipage/parsing.html#the-list-of-active-formatting-elements)
- [WHATWG adoption-agency algorithm](https://html.spec.whatwg.org/multipage/parsing.html#adoption-agency-algorithm)
- [WHATWG in-body insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inbody)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-686.md`

## Verification

- The exact-tree route fixture covers ordinary closes, reconstruction,
  misnested formatting with a furthest block, nested anchors, table-cell marker
  isolation, cloned attributes, sibling order, and Rust fragment commit. The
  separate Noah's Ark unit checks the three-equivalent-entry cap.
- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib active_formatting --locked --quiet` passed
  2/2 tests (8.51 s).
- `cargo test -p glass-browser --lib table --locked --quiet` passed 46/46 tests
  (11.01 s).
- `check-release-documentation.py --require-previous-version` validated 1,314
  Markdown files with zero current-claim failures; `check-documentation-depth.py`
  validated 93 routed/audited current guides and 19 substantive contracts;
  `check-documentation-coverage.py` validated 1,314 files, 346 MCP tools, 17
  examples, and 22 public modules; `check-tui-shortcuts.py` validated 15
  implementation help keys and 63 documentation markers.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Do not claim remote CI, push, publication, release, or general HTML parser
  conformance from local validation.
