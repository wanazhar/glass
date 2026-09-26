---
id: native-engine-browser-748
scope: glass-browser/xhr-html-document-parser
status: complete
depends_on: [native-engine-browser-747]
---

# Glass native-engine browser slice 748: standards XHR HTML documents

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) is the authority
  for native browser completion.
- [GCWP-0.1](../native-engine-browser-profile.md#xhr-document-responses)
  requires HTML/XHR document responses to follow the XHR and HTML parser
  contracts.
- [Native-engine architecture](../../architecture/native-engine.md) owns the
  parser boundary, XHR response-document projection, and resource limits.
- The normative [XHR document-response algorithm](https://xhr.spec.whatwg.org/#document-response)
  requires scripting-disabled HTML parsing and does not load referenced
  resources or apply XSLT.

## Objective

Replace the handwritten `nativeHtmlParseDocument` tree builder used for
page-realm XHR HTML `responseType="document"` responses with the existing
Glass-owned, bounded html5ever document `TreeSink`. Return a structured parsed
tree to the detached read-only JavaScript response-document materializer.
Do not route XML MIME responses through the HTML parser.

## Contract

- Preserve the existing XHR final-MIME selection: HTML MIME uses this route;
  XML MIME retains the strict XML parser; unsupported MIME returns a null
  document response.
- Parse as a full HTML document with `scripting_enabled = false`, as required
  for XHR document responses. HTML parse errors recover through the standard
  tree builder; they do not turn malformed HTML into an XML-style failure.
- Keep the response detached and read-only. Parsing must not execute response
  scripts, fetch referenced resources, mutate the page document, or enqueue
  live-DOM commands. Preserve response URL, normalized content type, doctype,
  head/body/title, namespaces, foreign adjusted names/attributes, and existing
  serializer/lookup behavior.
- Enforce the source-byte, temporary-node, published-node, and DOM-depth
  limits before materialization. A typed sink/limit failure returns no partial
  response document. Bound serialized host output and parser diagnostics.
- Reuse the Glass-owned TreeSink and structured node representation. Do not
  retain a second HTML tree builder or serialize markup and reparse it.
- Template content must not be dropped during conversion. Full
  `HTMLTemplateElement.content`/`DocumentFragment` identity remains a separate
  task; preserve the current response projection until that contract is
  implemented.
- This slice starts at the existing decoded-string boundary. XHR charset
  determination and non-UTF byte decoding remain explicit open conformance
  requirements; do not claim encoding parity from html5ever tokenization.
- Worker XHR behavior, XML response parsing, streaming XHR, and complete XHR
  Web IDL parity are outside this parser route and remain independently
  tracked.

## Tradeoffs

The full-document TreeSink removes a second hand-maintained HTML tokenizer and
tree builder and aligns navigation, fragment, and XHR HTML parsing on the same
standards algorithm. Structured conversion still allocates a bounded parser
tree and a detached read-only DOM projection. The source is already decoded
before this boundary, so byte encoding remains a separate correctness gap.

## Paths

- `crates/glass-browser/src/browser/native_engine/html_parser.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-748.md`

## Verification

- Complete the parser/adapter batch, then run
  `cargo check -p glass-browser --lib --tests --locked --quiet` before tests.
- Cover scripting-disabled `noscript`, malformed HTML recovery, table implied
  containers/foster parenting, SVG/MathML namespaces and adjusted attributes,
  doctype/quirks metadata, template-content retention, and transactional
  source/node/depth-limit failures.
- Run the existing focused XHR HTML-document unit batch and the
  process-backed HTTP XHR HTML-document regression. Confirm the XML response
  parser remains unchanged and response scripts/resources have no effects.
- Run formatting, whitespace, and current documentation truth/depth/shortcut/
  coverage gates when the required CLI inventory binaries are available.
- Keep UTF-8-only decoding, full XHR/Web IDL/WPT conformance, remote CI, and
  cross-platform certification open unless independently verified.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib --locked --quiet xhr_html_response_document`
  passed (6 tests), covering scripting-disabled `noscript`, malformed-table
  recovery, HTML table construction, SVG/MathML namespaces and adjusted
  attributes, templates, and bounded structured-parser output.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_xhr_exposes_bounded -- --nocapture` passed (2 tests),
  covering process-backed XML and HTML responses, response URL, malformed HTML,
  inert scripts, read-only documents, and no referenced-resource requests.
- The response parser no longer contains the handwritten JavaScript HTML
  tokenizer/tree builder. The HTML route uses the bounded html5ever document
  sink with scripting disabled; XML still uses its strict XML parser.
- Maintainer documentation gates passed after building `glass` and
  `glass-browser`: release-truth covered 1,376 Markdown files with zero
  current-claim failures; depth covered 93 current guides and 19 contracts;
  shortcut inventory covered 15 implementation keys and 63 markers; live
  coverage found 346 full-product MCP tools (101 browser-only), 17 examples,
  and 22 public modules.
- UTF-8-only response decoding, full template `DocumentFragment` identity,
  complete XHR/Web IDL and HTML WPT conformance, remote CI, and cross-platform
  certification remain open. This does not close issue #40.
