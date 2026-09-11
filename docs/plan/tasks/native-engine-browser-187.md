# Native engine browser slice 187: expose live document collections

Status: completed locally.

## Objective

Close the document-facing DOM surface gap for normal page code by exposing the
standard document roots and common collection accessors in every native realm.

## Contract

- Documents expose `head`, `scrollingElement`, `forms`, `links`, `scripts`, and
  `images`.
- `getElementsByTagName()` and `getElementsByClassName()` return live native
  HTML collections that observe attached tree mutations.
- `getElementsByName()` returns a NodeList of attached elements with the exact
  requested `name` attribute.
- Local, HTTP(S) content-worker, and same-origin frame documents share the
  same traversal and collection semantics.
- Existing query-selector behavior, source order, ownership, and command
  limits remain unchanged.

## Implementation

- Extended the existing collection proxy with a descendant-walking mode while
  retaining direct-child behavior for element `children`/`childNodes`.
- Added live document collections and standard document root accessors to the
  local and frame document projections.
- Added local live-collection mutation coverage plus HTTP(S) and frame surface
  identity witnesses.

## Tradeoffs and follow-up

Collections are backed by the bounded native tree walker and retain the
project's current integer/indexed proxy representation. This provides the
required live behavior without duplicating the document tree, but complete
HTMLCollection/NodeList Web IDL descriptor parity, named-property edge cases,
and broader HTML document interfaces remain issue #40 conformance work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `0ddbc3c2`
