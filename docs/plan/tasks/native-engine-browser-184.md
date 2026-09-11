# Native engine browser slice 184: live textContent mutation

Status: completed locally.

## Objective

Make element `textContent` and `innerText` replacement immediately visible as
a real same-turn text child across every currently supported native script
realm, while preserving one Rust-authoritative mutation transaction.

## Contract

- Assigning bounded element `textContent` or `innerText` replaces the existing
  child projection with one text node before the current script returns.
- Same-turn `firstChild`, `childNodes`, `textContent`, `innerHTML`, and parent
  links agree in local, process-backed HTTP(S), and same-origin frame realms.
- A replacement queues one bounded `childList` observer record with direct
  added and removed nodes, including the live text node in `addedNodes`.
- Preview construction does not emit duplicate create/attach IPC; the host
  sends one `setTextContent` command and Rust remains authoritative.
- Empty assignment removes the preview child and retains the existing
  bounded-value and failure behavior.

## Implementation

- Materialized a bounded detached text node from local and projected element
  setters under the existing host-command suppression scope.
- Updated local and frame mutation shadows to derive added/removed nodes from
  the live preview children and maintain parent/child indexes.
- Detached prior frame subtrees consistently before replacing projected
  children, preserving ownership and later snapshot refresh behavior.
- Added local, HTTP(S) content-worker, and same-origin frame assertions for
  same-turn child identity, serialization, parent links, and observer payloads.

## Tradeoffs and follow-up

The text node is a temporary JavaScript preview identity until the next native
snapshot; the final `setTextContent` command is still the durable source of
truth. This deliberately avoids a second staging tree and keeps IPC bounded,
but it does not claim complete Web IDL descriptors, HTML parser behavior,
observer coalescing, character-data edge cases, or browser-wide parity. Those
remain issue #40 completion work.

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
- implementation checkpoint: `6c59c511`
