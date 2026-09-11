# Native engine browser slice 186: expose character-data nodes

Status: completed locally.

## Objective

Close the text-node Web IDL identity and live character-data gap in every
native JavaScript realm used by Glass.

## Contract

- Text nodes expose the `Text` → `CharacterData` → `Node` prototype chain.
- `nodeValue`, `data`, and `textContent` read and write the same live value.
- `length` and `substringData()` use the text node's current UTF-16 string.
- `appendData()`, `insertData()`, `deleteData()`, and `replaceData()` update
  the live node through the existing bounded native command path.
- Character-data writes remain visible to the existing mutation shadow and
  `MutationObserver` machinery, with existing storage and command limits.
- The contract is shared by local, HTTP(S) content-worker, and same-origin
  frame projections; no second DOM implementation is introduced.

## Implementation

- Registered `CharacterData` and `Text` native constructors in the shared
  Web IDL identity setup.
- Added bounded CharacterData accessors and mutation methods on the shared
  prototype, including `IndexSizeError` range checks.
- Converted projected text `nodeValue` properties to accessors backed by the
  same closure value as `textContent`, and installed the `Text` prototype on
  initial, detached, content-worker, and frame text nodes.
- Extended local, HTTP(S), and same-origin frame identity coverage; the local
  witness also exercises all CharacterData mutation methods.

## Tradeoffs and follow-up

The implementation covers the text-node contract needed by the current native
DOM model, but it does not claim every Web IDL descriptor detail, full
CharacterData error/conversion parity, or complete HTML parser behavior.
Those remain part of the issue #40 browser-complete conformance work. The
methods reuse the existing single-string text representation and command
queue, which preserves bounded resource behavior and cross-realm consistency
at the cost of not yet modeling richer browser text-node internals.

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
- implementation checkpoint: `192624ea`
