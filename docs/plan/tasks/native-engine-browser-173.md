# Native engine browser slice 173: bounded MutationObserver delivery

Status: completed locally.

## Objective

Expose a usable mutation-observation contract in native page realms so normal
script-driven applications can react to Rust-owned DOM command batches without
polling or a CDP bridge.

## Contract

- `MutationObserver` is available in local and content-worker JavaScript
  realms and retains observer/callback identity across same-document
  evaluations.
- `observe()` accepts bounded `attributes`, `attributeFilter`,
  `attributeOldValue`, `characterData`, `characterDataOldValue`, `childList`,
  and `subtree` options. Invalid option combinations fail with `TypeError`.
- Attribute, character-data, append/insert, removal, and text/HTML replacement
  commands produce ordered `MutationRecord` values against the current native
  host objects. Multiple records from one script turn are delivered through
  one Promise-job checkpoint, matching the existing native microtask owner.
- `disconnect()` removes registrations and pending records; `takeRecords()`
  drains records without invoking the callback. Record queues and delivery are
  bounded by the existing native effect limit.
- Detached targets and removed nodes remain usable as record payloads without
  being reattached to the document or leaking a native arena pointer.

## Implementation

- Added a shared observer registry and callback scheduler to the native
  JavaScript host bootstrap.
- Added command-to-record shadow state for attributes, text nodes, parents, and
  child order, including old-value capture and reparent/remove records.
- Added bounded detached-node registration and a document owner anchor so
  subtree matching reaches the document root.
- Added a focused integration witness covering attribute, character-data,
  child-list, subtree, old-value, ordering, and callback delivery behavior.

## Tradeoffs

The record source is the typed JavaScript command transaction, so it observes
script-representable DOM changes before Rust commits the batch and keeps one
shared implementation across local and content-worker realms. It does not yet
project observer registrations into another browsing context's Window or make
layout/resource observers (`ResizeObserver` and `IntersectionObserver`) claim
the same contract; those are separate issue #40 promotion gates. The
implementation deliberately keeps the existing bounded command/microtask
limits instead of allowing unbounded callback or record growth.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_mutation_observer_delivers_script_dom_changes -- --exact`
  (1 passed, 0 failed, 441 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_scripts_ -- --nocapture`
  (2 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_applies_bounded_dom_commands_once -- --exact`
  (1 passed, 0 failed, 441 filtered out)
- implementation commit `e6ad246b`
