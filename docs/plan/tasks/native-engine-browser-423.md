# Native XHR XML document response (423)

status: complete
scope: native-engine/xhr-xml-response
issue: 40
depends-on: [native-engine-browser-422]

## Objective

Implement the page `XMLHttpRequest` XML-document response surface without
touching the live page DOM. XML responses must be observable through
`responseXML`, and `responseType = "document"` must use the same bounded,
native response owner instead of returning a text-only placeholder or routing
through CDP.

## Contract

- Window XHR accepts the standard `responseType = "document"` value.
- A default-type XHR whose final MIME type is an XML MIME type exposes a
  bounded detached XML `Document` through `responseXML` after `DONE`.
- A `responseType = "document"` XHR exposes a detached XML `Document` through
  both `response` and `responseXML` when the response MIME type is XML; a
  non-XML document response remains a typed null/failure result.
- The XML view preserves element and attribute case, namespace declarations,
  comments, processing instructions, CDATA, document type metadata, exact
  parent/child identity, `textContent`, `getElementById`, tag-name lookup, and
  XML serialization without creating native page commands or mutating the
  live HTML document.
- XML parsing is strict and bounded: malformed markup, mismatched tags,
  undeclared prefixes, unknown entities, excessive depth, and excessive node
  count produce `null` rather than a partial document or an exception escaping
  the XHR completion turn.
- `responseXML` throws `InvalidStateError` for response types other than the
  empty string or `document`, and returns `null` before `DONE` or after a
  failed XML parse.
- This slice does not claim HTML `responseType = "document"`, synchronous XHR,
  external entity loading, DTD validation, XSLT, streaming XML, or complete
  XML/Web IDL conformance.

## Implementation path

- Add one bounded XML parser and detached DOM materializer to the page host
  bootstrap; keep it separate from the live Rust-owned HTML DOM and command
  queue.
- Admit `document` in the page XHR response-type gate, parse XML MIME
  responses at completion, and expose the cached response object through the
  `responseXML` getter.
- Keep worker XHR behavior unchanged: the standard `responseXML` attribute is
  Window-exposed, and worker `document` selection is not promoted by this
  slice.
- Add a real HTTP witness for default XML detection, explicit document mode,
  namespaces, comments/CDATA/PI/doctype, identity/lookup/serialization, and
  malformed XML; retain the XHR regression group.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- The detached XML view is read-only and intentionally cannot enqueue DOM
  commands. This prevents a response document from corrupting the live HTML
  navigation snapshot, at the cost of leaving XML mutation/import/adoption for
  a later document-model wave.
- Parsing in the persistent page realm avoids a new Cargo dependency and keeps
  the two-crate build graph stable, but it is not a replacement for a full XML
  parser or WPT XML certification.
- The current loader already bounds response bytes and decodes response text;
  the slice reuses that owner and therefore leaves non-UTF XML encodings for a
  later encoding/conformance task.

## Evidence

- Local source checkpoint is recorded by the focused Conventional Commit and
  the linked issue #40 checkpoint comment.
- `cargo fmt --all` passed.
- `cargo check --quiet -p glass-browser --tests --locked` passed.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_exposes_bounded_xml_response_document --locked -- --nocapture` passed.
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` passed: 10 tests.
- The real HTTP witness covers default XML MIME detection, explicit document mode, malformed XML nulling, namespace and ID lookup, node ownership, immutable detached nodes, and XML serialization.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
