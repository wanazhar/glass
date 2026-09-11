# Native engine browser slice 163: DOM node construction

Status: completed locally.

## Objective

Make ordinary script-driven DOM construction commit real Rust-owned nodes in
the local and content-worker page realms. The construction surface includes
`document.createElement()`, `document.createTextNode()`, `appendChild()`, and
`insertBefore()`, including detached construction before insertion and moves
that preserve an existing subtree.

## Contract

- Every created element or text node receives a bounded temporary script index
  and becomes a real generational node only when the command batch commits.
- Element names, text-node values, node counts, and command counts remain
  bounded and validated before publication.
- `appendChild()` and `insertBefore()` validate element parents, reject
  document nodes and hierarchy cycles, validate the reference child, preserve
  the moved node's descendants, and maintain deterministic child order.
- Existing attached nodes can move to another element in the same command
  batch. Detached script-created nodes can be populated and nested before
  their first attachment.
- The JavaScript host maintains same-evaluation parent identity, text content,
  and bounded markup for constructed nodes; the next evaluation refreshes the
  projection from the committed native snapshot.
- Text nodes are Rust-owned even though the current semantic snapshot exposes
  element entries only; their text participates in ancestor text and markup
  serialization.
- The local and content-worker paths cross only as bounded typed commands.
  Same-origin frame projections retain the typed command allow-list but do not
  yet expose a detached construction factory; that is the next frame bridge
  unit.

## Implementation

- Added `CreateElement`, `CreateTextNode`, `AppendChild`, and `InsertBefore`
  to the script command protocol and frame-command validation surface.
- Added transactional temporary-index resolution, detached arena allocation,
  raw detached-tree mutation, cycle checks, reparenting, and deterministic
  insertion to `NativeDocument`.
- Extended `textContent` and form-state mutation helpers to resolve nodes
  created earlier in the same script batch.
- Added local host factories and internal parent/child/markup state so
  constructed objects behave coherently before the next snapshot refresh.
- Added unit coverage plus local and HTTP content-worker integration coverage
  for element creation, text creation, nested insertion, ordering, identity,
  and post-commit serialization.

## Tradeoffs

Temporary script indices avoid exposing arena allocation details and let a
single script batch construct a detached tree, but the host currently resets
the allocator for each evaluation and retains the bounded arena tombstones
until the next document generation. The insertion API supports element and
text children, while document fragments, comments, processing instructions,
live child collections, and mutation observers remain separate units. Frame
projection construction is deferred so no object from one JavaScript realm is
mistaken for an object owned by another realm.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --lib script_create_element_and_insert_child_commits_owned_nodes`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_web_idl_identity_and_dom_collections -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
