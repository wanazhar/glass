# Native engine browser slice 191: persistent script node identity

Status: completed locally.

## Objective

Preserve the identity and native ownership of script-created nodes when the
persistent JavaScript realm is refreshed between evaluations. A node created
by page code must remain usable in a later evaluation instead of becoming an
unresolvable temporary index after the host projects a new native snapshot.

## Contract

- The native document carries temporary script-node identities through both
  the parent/content-worker document wire and the JavaScript script snapshot.
- Every published identity is generation-scoped, points at an existing native
  arena node, and uses the reserved temporary-index range; malformed or
  duplicate identities are rejected before document publication.
- A successful script mutation transaction publishes its updated temporary
  identity map atomically. A failed batch does not publish a partial map.
- The persistent JavaScript realm retains created element/text wrappers and
  rebinds them to their committed native arena index on the next host refresh.
- An attached element created in one evaluation is the same object returned by
  a document query in the next evaluation, and its existing wrapper can still
  perform a typed native mutation.
- Local, HTTP(S) content-worker, and same-origin frame realms share the same
  bounded behavior without sharing Rust pointers or untyped process state.

## Implementation

- Added `NativeScriptNodeIdentity` to the typed document wire and script
  snapshot, with generation and range validation during content-worker
  reconstruction.
- Retained the native temporary-index map on `NativeDocument` and seeded each
  script command batch from the previously committed map. Only successful
  temporary identities are retained after the batch.
- Added a persistent JavaScript wrapper map. During projection, wrappers are
  rebound to the current native arena index and reused for matching element or
  text snapshot entries.
- Added local, content-worker, same-origin frame, and nested-frame regression
  witnesses that create an attached subtree, evaluate again, verify `===`,
  root, parent, and containment identity, and remove the old wrapper through
  the native command path.

## Tradeoffs and follow-up

The mapping is deliberately bounded by the existing temporary-index and DOM
limits and is reset with a new document generation. Detached script-created
nodes remain addressable by the native transaction while their arena storage
exists, but detached nodes are not projected as ordinary document snapshot
entries; garbage collection and a full detached-tree object model are later
work. Document, comment, and doctype node kinds, complete HTML tree-builder
behavior, full Web IDL descriptor parity, and browser-complete promotion gates
remain open in issue #40.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine script_exposes_web_idl_identity`
  (2 passed, 0 failed; local and content-worker witnesses)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract`
  (1 passed, 0 failed; same-origin frame element/text identity witness)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain`
  (1 passed, 0 failed; nested-frame regression)
- implementation checkpoints: `82fb2029`, `58db3800`
