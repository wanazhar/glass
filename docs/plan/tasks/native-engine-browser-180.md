# Native engine browser slice 180: document-fragment mutation observation

Status: completed locally.

## Objective

Make detached DOM construction observable through the same bounded
`MutationObserver` model used by attached documents. Scripts observing a
fragment must see its own child-list changes, including sibling context, while
moving attached nodes through fragment staging must keep Rust and JavaScript
state convergent.

## Contract

- `MutationObserver.observe()` accepts a `DocumentFragment` target.
- Fragment insertion and removal deliver ordered `childList` records with
  added/removed nodes, previous sibling, and next sibling values.
- `subtree: true` registrations on a fragment match mutations in descendants;
  detached fragment staging is not incorrectly reported as a document
  mutation.
- Moving an attached element or text node into a fragment emits the native
  removal command required to detach it from the Rust-owned document.
- Moving a node from a fragment into an element reports the fragment removal
  and preserves the existing attached-parent command record.
- Local, process-backed HTTP(S), and same-origin frame realms use the same
  bounded observer queue, delivery checkpoint, and record payload shape.

## Implementation

- Added fragment-target validation and a shared helper for direct fragment
  child-list removal records.
- Added direct insertion/removal records to local and frame fragment methods,
  including sibling context and nested-fragment flattening.
- Added fragment-aware removal bookkeeping to local, content-worker, and
  projected element/text paths; element-to-fragment moves now enqueue typed
  native removal commands.
- Preserved command-owned records for attached tree mutations so fragment
  observation does not duplicate ordinary element/document records.

## Tradeoffs and follow-up

Detached fragment operations have no Rust node index, so their records are
queued directly from the host object while attached changes continue through
the validated command transaction. This keeps one observer API without adding
a second IPC representation, but full specification-level coalescing,
transient-registration, namespace, and Web IDL descriptor parity remain issue
#40 conformance work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine document_fragments -- --nocapture`
  (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_mutation_observer_delivers_script_dom_changes -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `3bdbfd43`
