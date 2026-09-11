# Native engine browser slice 189: structural DOM primitives

Status: completed locally.

## Objective

Close the structural DOM gap that prevents page code from identifying the
owning tree and replacing an attached element through the native document
owner.

## Contract

- `Node.getRootNode()` returns the owning `Document` for attached local and
  same-origin frame nodes and the detached node/tree for detached elements and
  fragments.
- Attached local, HTTP(S) content-worker, and same-origin frame elements expose
  live `outerHTML` serialization with current attributes and child content.
- Assigning bounded `outerHTML` parses element/text replacement content through
  the existing host tokenizer, inserts every replacement node at the original
  position, and removes the replaced element in the same script turn.
- Replacement nodes immediately participate in selectors, live collections,
  parent/root identity, text/markup serialization, and the next host refresh.
- Detached elements reject `outerHTML` replacement rather than silently
  mutating an owner they are not attached to.
- Temporary script-created node identities remain unique across repeated host
  bootstrap refreshes and remain within the existing command/storage limits.

## Implementation

- Added the shared `getRootNode()` tree walk to local, content-worker, and
  frame node projections.
- Added live `outerHTML` accessors to initial and script-created local/frame
  elements, using the existing bounded fragment parser and typed create,
  attribute, attach, and remove commands.
- Made the temporary-node allocator runtime-global so reused element wrappers
  cannot retain a reset allocator closure after a host refresh.
- Added local, HTTP(S) content-worker, and same-origin frame witnesses for root
  identity and attached-element replacement, including frame state restoration
  so later contract checks observe the original fixture.

## Tradeoffs and follow-up

The replacement path intentionally reuses the native host tokenizer and
command transaction instead of introducing a second DOM tree. It covers the
bounded element/text surface already supported by fragment parsing; document
root replacement, shadow-root composition, namespace-aware insertion modes,
and complete HTML tree-builder/Web IDL conformance remain later issue #40
promotion work.

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
- implementation checkpoint: `8b4b0f61`
