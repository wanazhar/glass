# Native engine browser slice 179: fragment HTML construction and live queries

Status: completed locally.

## Objective

Make detached fragment construction useful for scripts that receive markup as
data. Assigning fragment `innerHTML` must create an immediately inspectable
subtree, preserve ownership and serialization, and make the result visible to
same-turn selector and collection calls in every native realm.

## Contract

- `DocumentFragment.innerHTML` serializes its current element/text children and
  accepts bounded markup input.
- The setter creates nested element and text nodes, applies quoted, unquoted,
  and boolean attributes, decodes common and numeric character references, and
  terminates void elements without requiring a document attachment.
- Fragment `querySelector`, `querySelectorAll`, `getElementsByTagName`, and
  `getElementsByClassName` traverse the current detached tree.
- Element and projected-frame query methods see newly created/attached
  descendants in the same script turn rather than only the bootstrap snapshot.
- Local, process-backed HTTP(S), and same-origin frame realms retain
  `parentNode`, `parentElement`, owner-document, selector, and serialization
  behavior before and after fragment attachment.
- Oversized fragment markup fails before construction with the native storage
  bound.

## Implementation

- Added one bounded host tokenizer shared by local and frame fragment
  factories, including entity decoding, attribute extraction, nesting, and
  void-element handling.
- Added fragment `innerHTML` accessors that clear existing children and stage
  parsed nodes through the existing typed create/attribute commands.
- Added recursive live-tree selector/collection traversal to the shared node
  accessor layer and switched local/frame element queries to use it.
- Hardened element/text content synchronization when a detached fragment is
  the temporary parent, and corrected projected detached `parentNode`
  semantics.

## Tradeoffs and follow-up

The tokenizer deliberately owns only the bounded element/text construction
surface used by the native host. It does not claim comments, raw-text and
foreign-content parsing, complete malformed-HTML tree-builder recovery, or
full Web IDL descriptor parity. Fragment-staging MutationObserver records and
the broader browser-complete promotion gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine document_fragments -- --nocapture`
  (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation checkpoint: `6c7d49f1`
