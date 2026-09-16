# Native XHR HTML document response (425)

status: complete
scope: native-engine/xhr-html-response
issue: 40
depends-on: [native-engine-browser-424]

## Objective

Complete the page XHR `responseType = "document"` branch for ordinary
`text/html` responses. HTML documents must be useful detached documents with
the same bounded read/query/serialization surface as native XML responses,
without becoming a second live page DOM or a hidden CDP path.

## Contract

- Page XHR `responseType = "document"` parses bounded `text/html` responses
  into a detached HTML `Document`; XML MIME responses retain the XML parser
  and all existing `responseXML` identity behavior.
- HTML document responses expose `response` and `responseXML` as the same
  detached document, with `responseText` remaining empty.
- The HTML document provides `documentElement`, `head`, `body`, `title`,
  `textContent`, element/attribute access, ID/tag lookup, simple selectors,
  parent/child ownership, and read-only HTML serialization.
- The bounded tokenizer handles case-insensitive HTML names, quoted and
  unquoted attributes, comments, doctypes, void elements, basic paragraph/list
  recovery, and raw-text `script`/`style` plus RCDATA `title`/`textarea`.
- Malformed or over-limit input fails closed to a typed null document; the
  parser never enqueues live page commands or mutates the current HTML
  document.
- Worker XHR behavior remains unchanged. This slice does not claim complete
  HTML tree-builder, template, form-control, custom-element, encoding, CSS,
  script-execution, or HTML/Web IDL conformance.

## Implementation path

- Add a bounded HTML raw-tree tokenizer beside the strict XML parser and feed
  both through the detached read-only DOM materializer.
- Extend page XHR explicit document completion to select HTML or XML by the
  final MIME type while leaving default-type HTML responses as text.
- Add document `head`/`body`/`title` projections and HTML-aware lookup and
  serialization behavior without sharing mutable live-DOM node factories.
- Add a real HTTP witness for a document with head/title/body, void and
  raw-text elements, comments, recovery, identity, lookup, immutability, and
  response projections; retain XML and XHR regression groups.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- A dedicated bounded tokenizer makes HTML document responses useful now while
  keeping parser recovery explicit; it is not presented as the full HTML5
  tree-builder and does not execute returned scripts.
- The detached document is intentionally read-only and cannot be adopted into
  the live page. This prevents response inspection from changing navigation
  state, at the cost of deferring import/adopt and full document mutation
  parity.
- Response text continues to come from the loader's bounded decoded text
  owner, so non-UTF encoding fidelity remains a later gate.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_exposes_bounded_html_response_document --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (12 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-425.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

All scoped checks and the complete XHR regression group passed locally. Remote
CI, push, release, tag, and registry publication are outside this local
checkpoint.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
