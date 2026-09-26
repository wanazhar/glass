---
id: native-engine-browser-752
scope: glass-browser/navigation-html-template-fragments
status: complete
depends-on: [native-engine-browser-751]
---

# Glass native-engine browser slice 752: live navigation template fragments

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- Slice 751 retained template-content fragment boundaries in detached,
  read-only HTML XHR response documents. Top-level navigation documents still
  flattened `<template>` children into their host node, so the live page realm
  could not expose the platform's `.content` identity or traversal boundary.
- The [HTML Standard template contract](https://html.spec.whatwg.org/multipage/scripting.html#the-template-element)
  associates each HTML template with a distinct `DocumentFragment`. Its
  children are not ordinary template-element children, and they use the
  document's inert template-contents owner document.

## Objective

Preserve template-content fragments in parsed top-level navigation documents
and connect their identity and bounded mutation behavior to the live page
realm without crossing the existing node/depth or script-mutation limits.

## Contract

- Parsed and dynamically created HTML templates expose a stable `.content`
  `DocumentFragment` with node type 11, separate from the template's ordinary
  child list.
- Template content is not traversed by document/template child collections,
  selectors, or tree walking. The fragment's own traversal sees its children;
  moving a child between the fragment and the document updates both trees.
- The fragment and its descendants report the shared inert template-contents
  owner document, distinct from the navigation document. Fragment identity is
  preserved when `template.innerHTML` replaces its subtree.
- Template-content insertion and `innerHTML` updates use the native DOM
  mutation path and retain configured node, depth, and script-operation bounds.
- HTML serialization reads template content from its fragment. Ordinary
  navigation and the detached read-only XHR route retain separate, correct
  template fragment boundaries.
- This slice does not claim same-origin frame-document projection,
  clone/import/adopt semantics, declarative shadow roots, complete DOM or HTML
  conformance, remote CI, cross-platform certification, or issue #40
  completion.

## Tradeoffs

Template contents now participate in the native node graph as a separately
budgeted fragment, and the page-realm bridge must preserve that identity across
snapshots and mutation commits. This closes the top-level navigation flattening
gap while leaving frame projection and broader DOM ownership algorithms as
explicit follow-up work.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-752.md`

## Verification

- Run `cargo fmt --all -- --check` and `git diff --check`.
- Type-check the affected `glass-browser` library and test targets once after
  the implementation batch; keep behavioral tests focused on template
  fragment parsing, live identity/ownership, traversal boundaries, insertion,
  child movement, and `innerHTML` replacement.
- Run the maintainer documentation truth, depth, shortcut, and inventory
  gates. Do not promote this local evidence to remote CI, cross-platform, or
  full-profile conformance evidence.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --tests --locked --quiet` passed; the
  pre-existing retired-parser dead-code warnings remain non-fatal.
- `cargo test -p glass-browser --lib --locked --quiet
  html_template_contents_use_an_inert_fragment_outside_document_traversal`
  passed (1 test).
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_exposes_live_template_content_fragments` passed
  (1 test), covering live `.content` identity, inert owner-document identity,
  query/traversal boundaries, fragment mutation, moving a child into the
  document, and template `innerHTML` replacement.
- Maintainer gates passed: release truth covered 1,380 Markdown files with
  zero current-claim failures; depth covered 93 current guides and 19
  contracts; shortcut inventory covered 15 implementation keys and 63
  markers; coverage found 346 full-product MCP tools (101 browser-only), 17
  examples, and 22 public modules. Same-origin frame projection,
  clone/import/adopt behavior, full HTML/DOM conformance, remote CI,
  cross-platform certification, and issue #40 completion remain open.
