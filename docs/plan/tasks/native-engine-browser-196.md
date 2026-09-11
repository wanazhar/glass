# Native engine browser slice 196: attribute nodes and live NamedNodeMap

Status: completed locally.

## Objective

Close the attribute-node Web IDL gap in every JavaScript realm already served
by the native engine. Page code must be able to create, inspect, attach,
replace, mutate, enumerate, and remove attributes without leaving the
Glass-owned DOM model.

## Contract

- `document.createAttribute(name)` returns a bounded `Attr` with the expected
  `Node`/`Attr` identity, node metadata, value accessors, owner document, and
  detached ownership.
- `Element.getAttributeNode()`, `setAttributeNode()`,
  `removeAttributeNode()`, and their bounded namespace forms preserve
  attribute identity, replacement return values, owner-element transitions,
  and typed DOM exceptions.
- Direct `setAttribute()`/`removeAttribute()` and host projection refreshes
  keep retained `Attr` objects synchronized with the element's current
  value/ownership.
- `Element.attributes` exposes a live `NamedNodeMap` with indexed access,
  iteration, named lookup, replacement, and removal over the native attribute
  store.
- Local, HTTP(S) content-worker, and same-origin frame realms retain the same
  attribute behavior; unsupported non-null namespaces fail explicitly rather
  than being misrepresented as supported.

## Implementation

- Added bounded native `Attr` construction, metadata, value/text/node-value
  accessors, ownership synchronization, and constructor identity.
- Added the shared element attribute-node surface to local elements, frame
  projections, and frame-created detached elements.
- Added a live indexed/iterable `NamedNodeMap` view backed by the element's
  existing attribute store, including replacement and `NotFoundError`,
  `WrongDocumentError`, `InUseAttributeError`, and `NamespaceError` paths.
- Added local and same-origin-frame integration witnesses, with the content
  process Web IDL regression remaining green.

## Tradeoffs and follow-up

This is the bounded no-namespace HTML attribute contract. Namespace-qualified
attribute storage, complete Web IDL property descriptors/iterators, XML
documents, custom elements, and browser-wide conformance remain issue #40
work. Attribute nodes use the existing temporary-node and script-value limits;
they are host objects and do not create independent Rust DOM arena nodes.
No CDP or fallback path changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)

Implementation checkpoint: `fce4146a`.
