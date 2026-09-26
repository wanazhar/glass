---
id: native-engine-browser-756
scope: glass-browser/context-local-document-adopt-node
status: planned
depends-on: [native-engine-browser-755]
---

# Glass native-engine browser slice 756: adopt nodes within a browsing context

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- The [DOM Standard adoption algorithm](https://dom.spec.whatwg.org/#concept-node-adopt)
  detaches a node from its parent, updates the node document of its inclusive
  descendants and element attributes, and returns the same node. `Document`
  input throws `NotSupportedError`; `ShadowRoot` input throws
  `HierarchyRequestError`.
- The [HTML template adoption steps](https://html.spec.whatwg.org/multipage/scripting.html#the-template-element)
  move a template's content fragment into the appropriate inert template
  owner document associated with the template's newly adopted node document.
- The current native frame command protocol routes mutations to a distinct
  frame owner and cannot transfer one live node identity between those native
  browsing-context owners. This slice therefore adopts only within one
  browsing-context command owner, including that context's associated inert
  template-owner Document. Cross-context transfer is a separate issue #40
  gate and must fail before changing the source node.

## Objective

Expose bounded `Document.adoptNode()` in native live and inert Documents. Keep
the original JavaScript node identity, detach it from its current parent,
rehome its supported subtree to the target Document, and apply the HTML
template-content adoption step without crossing native browsing-context
owners.

## Contract

- Expose the method on the live top-level Document, same-origin frame
  Documents, and inert template-owner Documents.
- Accept only native Nodes whose current owner and requested target belong to
  the same browsing-context command owner. A node from another live frame or
  top-level context fails with `NotSupportedError` before detachment or owner
  mutation; it is never cloned or silently left in the wrong native tree.
- Support the node kinds already modeled by the native DOM projection:
  Elements, Attrs, Text, Comments, DocumentTypes, and DocumentFragments.
  Invalid non-Node values throw `TypeError`; a Document throws
  `NotSupportedError`; a ShadowRoot throws `HierarchyRequestError`.
- Return the identical Node object. If the node has a parent, remove it using
  that parent's normal live mutation path. Adoption within the same Document
  still detaches the node but keeps its node document unchanged. Adopting an
  Attr preserves its `ownerElement`; element attributes follow the adopted
  element's document.
- When the target Document differs, update the node document for the adopted
  node, supported descendants, and element attributes. For each adopted HTML
  template, move its existing content fragment and descendants to the target
  Document's appropriate inert template-owner Document; apply this recursively
  to nested templates. Do not clone nodes or fragments.
- Preflight the entire supported adoption walk against native node and depth
  limits before detaching anything, so a bound/type/context failure leaves
  parentage and ownership unchanged.
- Preserve per-context command routing and persistence across script-realm
  refresh. Do not add another installable crate or change the CDP migration
  path.
- This slice does not implement cross-context node-identity transfer,
  `adoptedCallback` reactions/custom-element registries, Shadow DOM adoption,
  all DOM node kinds, full DOM conformance, remote CI, cross-platform
  certification, or issue #40 completion.

## Tradeoffs

The command boundary treats each live browsing context as an independent
native document owner. Reusing a source frame node in another context without
an engine-level transfer protocol would corrupt identity or send commands to
the wrong owner, so this slice fails closed for that case. The same-context
operation establishes recursive owner reassignment and inert-template
semantics; a later transfer slice must add explicit source/destination
ownership and preserve identity across the native command boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-756.md`

## Verification

- `cargo fmt --all -- --check` and `git diff --check`.
- Run `cargo check -p glass-browser --tests --locked --quiet` after the
  implementation batch.
- Focused process-backed adoption tests in the top-level Document and a
  same-origin frame Document, including each context's inert template owner,
  nested templates, same-Document detach, source persistence, typed failures,
  bounds preflight, and cross-context fail-closed behavior.
- Maintainer release-truth, documentation-depth, TUI-shortcut, and live
  documentation-coverage gates.
- Evidence is local Linux only; broad DOM conformance, remote CI, and
  cross-platform certification remain issue-level gates.

## Evidence

Pending implementation and verification.
