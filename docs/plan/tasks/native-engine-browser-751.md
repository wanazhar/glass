---
id: native-engine-browser-751
scope: glass-browser/xhr-html-template-content
status: complete
depends-on: [native-engine-browser-750]
---

# Glass native-engine browser slice 751: XHR template content fragments

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- The HTML parser already retains `<template>` contents as a separate parser
  `DocumentFragment`. The XHR response-document projection currently flattens
  those nodes into the template element, losing the DOM boundary.
- The [HTML Standard template contract](https://html.spec.whatwg.org/multipage/scripting.html#the-template-element)
  requires `HTMLTemplateElement.content` to return its associated fragment;
  template content is not a child of the template element and uses the
  document's inert template-contents owner document.
- This slice is limited to detached, read-only HTML XHR response documents.
  Initial navigation and mutable live-document template projection remain
  separate work.

## Objective

Preserve parsed template-content fragments through the bounded XHR HTML
response-document materializer and expose their identity and tree boundary
without making the response document mutable or executing/fetching content.

## Contract

- A parsed HTML template is an `HTMLTemplateElement`; its stable `.content` is
  a `DocumentFragment` with node type 11 and `parentNode === null`.
- Template children live under `.content`, not under the template element.
  `template.children`, `template.childNodes`, `template.querySelector()`, and
  response-document traversal do not cross into template content;
  `template.content` traversal does.
- The fragment and its descendants have correct parent/root identity and a
  shared inert HTML template-contents owner document distinct from the XHR
  response document.
- `template.innerHTML`, template/document serialization, and serialization
  of the fragment include template content. Response-document and fragment
  mutation APIs continue to throw `NoModificationAllowedError`.
- Parsed-node, published-node, and depth bounds continue to include template
  subtrees; malformed or over-limit parser output is rejected, never partially
  materialized.
- Preserve XHR's detached/read-only behavior, encoding, URL/MIME, scripting
  disabled parser, and no-referenced-resource-load contract. XML and text XHR
  routes are unchanged.
- This does not implement mutable template content in navigation documents,
  clone/import/adopt semantics, declarative shadow roots, or full DOM/XHR/WPT
  conformance.

## Tradeoffs

The work stays in the existing bounded response-document bridge and keeps the
main mutable DOM model unchanged. It closes a concrete XHR DOM identity gap,
but does not claim ordinary live-page template parity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-751.md`

## Verification

- Run `cargo fmt --all -- --check` and `git diff --check`.
- After the complete code batch, run one locked `cargo check` for the affected
  `glass-browser` library/tests before tests.
- Deterministic response-document coverage verifies fragment identity,
  ownerDocument, parent/root boundaries, selectors/traversal, HTMLTemplateElement
  identity, serialization, and read-only errors.
- The focused parser/DOM unit test and process-backed HTML XHR regression pass;
  XML, inert-script, no-resource, and encoding coverage are retained.
- Affected maintainer documentation gates pass. Do not claim live-page
  template parity, complete XHR conformance, remote CI, or issue #40 completion.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --tests --locked --quiet` passed; only the
  pre-existing retired-parser dead-code warnings remain.
- `cargo test -p glass-browser --lib --locked --quiet
  xhr_html_response_document_keeps_disabled_script_and_foreign_tree_semantics`
  passed (1 test).
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_xhr_exposes_bounded_html_response_document` passed
  (1 test), including nested template fragments, inert owner-document
  identity, DOM/query/serialization boundaries, and mutation rejection.
- The test's page scripts are 11,763 and 2,446 bytes, each below the 16 KiB
  script limit. The probe is split into a chained script so it does not alter
  the runtime limit.
- Maintainer documentation gates pass: release truth covered 1,379 Markdown
  files with zero current-claim failures; depth covered 93 current guides and
  19 contracts; shortcut inventory covered 15 implementation keys and 63
  markers; coverage found 346 full-product MCP tools (101 browser-only), 17
  examples, and 22 public modules.
- Remote CI, live navigation-document template semantics, mutable
  clone/import/adopt behavior, and complete XHR/DOM conformance remain open.
