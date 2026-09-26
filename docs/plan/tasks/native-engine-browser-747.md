---
id: native-engine-browser-747
scope: glass-browser/html-fragment-parser
status: complete
depends_on: [native-engine-browser-746]
---

# Glass native-engine browser slice 747: standards fragment parsing

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) is the authority
  for the browser-complete native-engine objective.
- [GCWP-0.1](../native-engine-browser-profile.md) requires standards HTML
  tokenization/tree construction and normal Glass DOM operations.
- [Native-engine architecture](../../architecture/native-engine.md) defines
  parser ownership, bounded allocations, and the current document/fragment
  routes.
- [Capability analysis](../analysis/native-engine.md) records remaining
  parser and browser-profile gaps.

## Objective

Replace the hand-maintained contextual-fragment tree builder used by
`NativeDocument::apply_script_inner_html` and the same-turn JavaScript/frame
projection with html5ever fragment parsing. The JS projection consumes the
structured result from the synchronous parser host call; the ordered Rust
commit reparses the same source through the same bounded algorithm and actual
context. A script's immediate DOM reads and the committed Glass tree must
agree without serializing markup through a second parser.

## Contract

- Use html5ever's fragment algorithm with the context element's local name,
  namespace, attributes, and scripting flag. Do not serialize and reparse the
  result through another parser.
- Keep one Glass-owned bounded `TreeSink`; do not use `markup5ever_rcdom` as a
  production DOM. Preserve foreign element/attribute namespaces, template
  parser fragments, adjusted names, and source/tree order in the structured
  result.
- Use the same bounded parser algorithm and context for the Rust DOM commit and
  synchronous JavaScript/frame projection. The source is parsed once per
  surface, not passed as an identical tree instance between runtimes. There is
  no handwritten-parser fallback for a fragment request.
- Enforce the configured source-byte, temporary-node, published-node, and
  DOM-depth limits before publishing. Temporary allocations remain bounded by
  the issue's two-times node-budget contract. A typed limit or tree-sink error
  must leave the old `innerHTML` subtree intact; never publish a partial
  fragment.
- Preserve DOM setter behavior around the parser: script elements created by
  `innerHTML` remain inert, and attached CSP metadata is captured by the
  existing policy owner after the replacement is committed.
- Bound parser diagnostics and do not log source text or page-controlled
  parser diagnostics.
- Cover context-sensitive tokenization and tree construction (including
  RCDATA/RAWTEXT/script/plaintext, tables/foster parenting, SVG/MathML, and
  integration points) with direct parser and JavaScript read-after-write
  tests.
- XHR `responseType="document"` was outside this slice and is now covered by
  completed slice 748. Template `DocumentFragment` exposure, quirks-mode
  layout, parser-stream reentrancy, complete HTML WPT conformance, and
  cross-platform/remote certification remain open.

## Tradeoffs

The synchronous JS-to-Rust parser callback and ordered DOM commit each parse the
source and convert the result, adding duplicate parser work to `innerHTML`
writes. This keeps each surface on the same standards parser and context while
avoiding a second handwritten tree builder or serialized-markup round-trip.
The same-turn and committed-tree parity tests are required to detect any
divergence. html5ever still needs explicit GCWP/WPT differential coverage; its
use does not itself establish browser conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/html_parser.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-747.md`

## Verification

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` after the
  implementation batch and before focused tests.
- Test the parser's context algorithm, malformed-tree recovery, namespace and
  template handling, transactional node/depth-limit failures, and parser error
  bounds.
- Test script `innerHTML` writes followed by same-turn reads and compare the
  JS projection with the committed Rust tree for the same input/context.
- Run process-backed native navigation that exercises an external page script
  writing and reading fragment DOM without CDP.
- Run formatting, `git diff --check`, and repository documentation truth,
  depth, shortcut, and coverage checks after records are synchronized.
- Record local versus remote and cross-platform evidence separately. This
  slice cannot close issue #40 or claim full HTML/browser conformance.

## Evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib --locked --quiet inner_html` passed
  (12 tests); `html_fragment_parser` passed (10 tests).
- The focused selector-default-case and modal argument/result tests passed
  (1 each).
- The process-backed `native_runtime_session_loads_bounded_external_http_html_without_cdp`
  regression passed (1 test).
- `cargo fmt --all -- --check`, `git diff --check`, release-documentation
  truth, documentation depth, TUI shortcut, and Markdown-link checks passed.
- Built `glass` and `glass-browser` for live CLI inventory. The documentation
  truth gate passed for 1,376 Markdown files with zero current-claim failures;
  depth passed for 93 current guides and 19 contracts; shortcut inventory
  passed for 15 implementation keys and 63 markers; coverage passed for 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules.
- Remote CI, cross-platform checks, and full GCWP/WPT conformance remain open;
  this evidence does not close issue #40.
