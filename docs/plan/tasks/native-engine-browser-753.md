---
id: native-engine-browser-753
scope: glass-browser/same-origin-frame-template-fragments
status: complete
depends-on: [native-engine-browser-752]
---

# Glass native-engine browser slice 753: same-origin frame template fragments

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- Slice 752 preserves template contents as native `DocumentFragment` nodes
  for top-level navigation documents and exposes them in the live page realm.
- Same-origin frame documents use a separate JavaScript projection. Its
  snapshot currently creates element, text, comment, and doctype nodes but
  omits native node-type-11 fragments, so template contents do not have their
  live DOM boundary in `iframe.contentDocument`.

## Objective

Project native template-content fragments into same-origin frame documents
with the same essential identity, ownership, traversal, and bounded mutation
behavior established for the top-level live document.

## Contract

- A same-origin frame template exposes a stable `.content` `DocumentFragment`
  with node type 11 and its native snapshot identity; nested template
  fragments remain distinct nodes.
- Frame-document and template-element child collections, selectors, and
  traversal do not enter template contents. The content fragment traverses its
  own descendants, whose parent/root edges match the frame snapshot.
- The frame document has one shared inert template-contents owner document,
  distinct from the live frame document. Parsed template descendants report
  that owner; moving a node into the frame document does not silently adopt it.
- `template.innerHTML`, template serialization, fragment insertion, and child
  movement route bounded mutations to the owning frame document. The parent
  browsing context cannot gain access to a cross-origin frame through this
  projection.
- Preserve frame identity, generation/revision ownership, node/depth limits,
  origin checks, and the top-level DOM behavior from slice 752.
- This slice does not claim clone/import/adopt parity, full frame/DOM/HTML
  conformance, cross-platform certification, remote CI, or issue #40
  completion.

## Tradeoffs

The frame projection must now represent detached fragment nodes and retain an
inert owner-document identity in addition to its existing frame-owned node and
mutation maps. This closes a concrete same-origin frame API gap while keeping
the parent and child browsing-context state owners separate.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-753.md`

## Verification

- Run `cargo fmt --all -- --check` and `git diff --check`.
- After the complete frame-projection batch, run one locked check for the
  affected `glass-browser` library/test targets, then the focused DOM unit and
  process-backed same-origin-frame template regression.
- Run release-documentation truth, depth, TUI-shortcut, and coverage gates.
- Keep remote CI, cross-platform certification, and full-profile conformance
  claims separate from this Linux-local slice.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --tests --locked --quiet` passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_same_origin_frame_template_content_uses_native_fragments -- --nocapture`
  passed (1 process-backed test). It verifies stable nested template fragments,
  inert owner-document identity, tree/query boundaries, append/move mutations,
  persistence across script calls, and `innerHTML` replacement.
- Maintainer gates passed: release truth covered 1,381 Markdown files with
  zero current-claim failures; depth covered 93 current guides and 19
  contracts; shortcut inventory covered 15 implementation keys and 63
  markers; coverage found 346 full-product MCP tools (101 browser-only), 17
  examples, and 22 public modules. Clone/import/adopt behavior, full
  frame/DOM/HTML conformance, cross-platform certification, remote CI, and
  issue #40 completion remain open.
