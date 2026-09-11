# Native engine browser slice 167: selector and class-token surface

Status: completed locally.

## Objective

Make ordinary DOM query and class-driven application code useful in local,
content-worker, and same-origin frame realms without bypassing the Rust-owned
document or frame ownership model.

## Contract

- Document and element query methods support compound selectors, descendant
  and child combinators, selector lists, universal/type/id/class selectors,
  attribute presence/equality/token/prefix/suffix/substring operators, and
  the bounded common state/structural pseudo-classes.
- `Element.matches()` and `Element.closest()` use the same selector parser and
  current owner tree; scoped element queries never return the owner itself.
- `classList` exposes live token length/item/contains/value/iteration reads and
  validated `add()`, `remove()`, `toggle()`, and `replace()` writes.
- Local and same-origin frame projections share the selector and class-token
  behavior; class writes cross only as bounded typed attribute commands.
- Existing attached-node snapshots, live child collections, origin checks,
  and Rust mutation quotas remain authoritative.

## Implementation

- Added bounded selector-list and selector-chain parsing with compound
  matching, combinator traversal, attribute operators, and common
  state/structural pseudo-classes.
- Added scoped query/match/closest methods to local and projected frame
  elements, and reused the same matcher for document and frame queries.
- Added owner-backed `classList` tokens with native-class mutation commands
  and installation for existing and detached element hosts.
- Extended content and same-origin frame integration coverage for selector
  scope/identity, class mutation, class token reads, and frame projection.

## Tradeoffs

The selector implementation is deliberately bounded and deterministic: it
does not claim every CSS grammar extension, namespace/escape form, selector
pseudo-class, or style-engine selector dependency. Class-list changes reuse
the existing attribute command, so the native snapshot remains authoritative
after the evaluation while same-evaluation reads are immediate. Full CSS
selector and Web IDL descriptor parity remain promotion work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
- `git diff --check`
