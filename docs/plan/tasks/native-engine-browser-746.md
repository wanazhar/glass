---
id: native-engine-browser-746
scope: glass-browser/standards-html-document-parser
status: complete
depends_on: [native-engine-browser-745]
---

# Glass native-engine browser slice 746: standards HTML document tree builder

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) defines the
  browser-complete objective and native-only production gate.
- [Glass Core Web Profile](../native-engine-browser-profile.md) defines the
  frozen browser contract.
- [Native-engine architecture](../../architecture/native-engine.md) defines
  ownership, process boundaries, and parser integration.
- [Native-engine capability analysis](../analysis/native-engine.md) tracks
  remaining profile gaps and task sequencing.

## Objective

Replace the hand-maintained document-navigation tokenizer/tree-construction
loop with the maintained `html5ever` tokenizer and tree builder, connected to
Glass-owned nodes through a small native `TreeSink`. Apply it to initial
documents, including process-backed HTTP(S) navigation and frame documents
that share `NativeDocument::parse`.

This is a parser-foundation slice, not a browser-completion or full
html5lib/WPT-conformance claim. `innerHTML`, same-turn JavaScript fragment
projection, and XHR `responseType="document"` are separate parser routes and
must remain explicitly identified until they use the same contract.

## Contract

- `html5ever` owns HTML tokenization and document tree construction for the
  selected route; Glass continues to own `NativeDocument`, identities,
  resource processing, style policy, and page lifecycle.
- Implement the parser's `TreeSink` against a temporary bounded parse tree;
  do not use `markup5ever_rcdom` as a production DOM. The temporary tree must
  represent template contents as a distinct parser fragment so insertion and
  foster-parenting decisions use the parser's actual template boundary.
- Bound temporary allocations while parsing to at most twice the configured
  Glass node limit (including transient moves and template fragments), and
  enforce DOM element depth at each attachment/reparent. Return typed limit
  failures before converting a failed parser tree.
- Convert the result into the Glass node arena while enforcing the existing
  document-byte, node-count, and DOM-depth limits. A limit failure rejects the
  document; it must not publish a truncated tree.
- Preserve element/attribute namespace identity, adjusted SVG/MathML names,
  first-duplicate-attribute behavior, doctype data, and document node order.
  Unsupported or unrepresentable parser output must fail explicitly rather
  than silently drop nodes.
- Preserve MathML `annotation-xml` as a foreign-content breakout boundary for
  `</p>` and `</br>`. html5ever 0.40.x omits this dynamic integration point
  from its breakout loop; the token-aware TreeSink bridge may expose the
  boundary as a MathML text integration point for each matching loop query,
  without changing the stored element QName or subsequent parser decisions.
- Existing script/resource/CSP/style post-processing runs against the
  resulting Glass tree. There is no CDP, remote-browser, or legacy-parser
  fallback for the selected navigation path.
- Capture html5ever's parse errors in a bounded diagnostic count; do not
  include page-controlled parser text in ordinary logs.
- Keep the limitation visible: template `DocumentFragment` exposure,
  quirks-layout behavior, script-stream reentrancy, and
  the three non-document parser routes are not completed by this slice.
- Do not force parity with the still-legacy fragment/XHR routes where they
  disagree with document parsing on malformed input. Keep explicit
  route-specific regression expectations. In the document route, `</p>` and
  `</br>` in column-group insertion mode pop the current `<colgroup>` and are
  reprocessed through table rules: the generated HTML `<p>` and `<br>` are
  foster-parented before the table, and a later `<col>` belongs to an implied
  anonymous `<colgroup>`. The legacy fragment/XHR expectations remain distinct.

## Tradeoffs

The maintained parser brings substantially broader standards-driven recovery
than incremental handwritten rules, at the cost of additional dependency
compile work and a temporary adapter tree. The temporary tree allows up to
twice the published-node budget so template fragments and transient parser
moves fit without making parser allocation unbounded. The upstream project
documents remaining tree-builder differences; issue #40 still requires a
pinned conformance corpus and explicit handling of every profile failure. The
adapter—not an upstream sample DOM—keeps Glass's ownership, resource limits,
and security boundary intact.

## Paths

- `crates/glass-browser/Cargo.toml`
- `Cargo.lock`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/html_parser.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-745.md`
- `docs/plan/tasks/native-engine-browser-746.md`

## Verification

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` after the
  implementation batch and before focused tests.
- Run the native document-parser regressions for implied document structure,
  malformed formatting recovery, foster parenting, foreign namespaces,
  MathML `annotation-xml` `</p>`/`</br>` breakouts (including nested SVG),
  templates, doctypes, temporary-tree node/depth limits, and published-arena
  node/depth-limit failures.
- Run the process-backed page-loading regression to verify the selected
  production navigation route, then formatting and `git diff --check`.
- Run the repository documentation truth, depth, shortcut, and coverage gates
  after the design/status records are synchronized.
- Record html5ever's known gaps and all checks that could not run. Do not claim
  all four parser routes, full HTML conformance, or cross-platform certification
  from this task's evidence.

## Evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed on Linux.
- `cargo test -p glass-browser --lib --locked --quiet parser` passed 177/177.
- `cargo test -p glass-browser --test native_engine --locked --quiet native_runtime_session_loads_bounded_external_http_html_without_cdp` passed 1/1 through the process-backed HTTP route.
- Documentation truth passed for 1,374 Markdown files with zero current-claim
  failures. Documentation depth passed for 93 guides/19 contracts; shortcut
  inventory passed for 15 keys/63 markers; coverage passed for 346 MCP tools
  (101 browser-only), 17 examples, and 22 public modules.
- `cargo check -p glass-dev --bin glass --locked --quiet` and
  `cargo build -p glass-dev --bin glass --locked --quiet` passed to provide the
  current CLI binary for live coverage validation. Formatting and whitespace
  checks passed.
- Remote CI, Windows/macOS runtime checks, and cross-platform certification
  were not run; these remain issue #40 gates.
