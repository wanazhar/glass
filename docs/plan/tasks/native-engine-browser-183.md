# Native engine browser slice 183: live element innerHTML mutation

Status: completed locally.

## Objective

Make element `innerHTML` mutations immediately observable through the same
script-visible DOM tree used by selectors, text access, serialization, and
`MutationObserver`, while preserving one Rust-authoritative mutation command.

## Contract

- Assigning bounded element `innerHTML` constructs nested element/text children
  before the current script returns.
- Same-turn element/document selectors, text, child counts, and serialized
  `innerHTML` reflect the parsed tree; detached nodes are not added to the
  document until attached.
- An element `innerHTML` mutation produces one bounded child-list observer
  record with removed prior children and added direct children, without leaking
  parser-internal create/attribute/attach commands into the host command batch.
- Local, process-backed HTTP(S), and same-origin frame realms share the parser,
  command-suppression, mutation-shadow, and persistence behavior.

## Implementation

- Added a scoped host-command suppression guard for parser preview operations
  and frame command routing.
- Reused the bounded nested fragment parser to populate local and projected
  element children, then emit one authoritative `setInnerHtml` command.
- Updated local and frame mutation shadows to expose live added nodes and
  maintain parent/child indexes for subsequent same-turn operations.
- Added local, HTTP(S) content-worker, and same-origin frame tests for immediate
  nested lookup, text/serialization, and observer payloads.

## Tradeoffs and follow-up

The preview intentionally reuses the existing bounded element/text tokenizer;
it is not a specification-complete HTML tree builder and does not yet cover
raw-text, foreign-content, malformed-input recovery, scripts, or custom
elements. Suppression prevents duplicate IPC and lets Rust remain authoritative,
but preview identities are temporary and are rebuilt from the next native
snapshot. Full HTML parsing, coalescing, and browser-wide issue #40 gates remain.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_document_fragments_preserve_tree_ownership_and_helpers -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_document_fragments_cross_the_http_boundary -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `28542685`
