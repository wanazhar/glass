# Native engine browser slice 182: form and HTML attribute reflection

Status: completed locally.

## Objective

Extend the attribute-backed DOM surface beyond identity fields so common form
and HTML properties behave as native browser properties across every supported
script realm.

## Contract

- Boolean properties including `disabled`, `hidden`, `multiple`, `required`,
  `readOnly`, `autofocus`, `open`, `controls`, `loop`, `muted`, `autoplay`,
  `reversed`, `formNoValidate`, and `noValidate` map true to a present
  attribute and false to an absent attribute.
- Common string properties including `name`, `title`, `lang`, `dir`, `slot`,
  `htmlFor`, `accept`, `alt`, `placeholder`, `pattern`, `min`, `max`, `step`,
  `action`, `method`, `target`, `rel`, and `download` reflect their attributes.
- `type` returns a bounded lower-case attribute value, with `text` and
  `submit` defaults for input and button elements respectively.
- Snapshot refresh preserves actionability state without invoking property
  setters; local, process-backed HTTP(S), and same-origin frame realms share
  the command and persistence contract.

## Implementation

- Added common string and boolean attribute accessor installation for local and
  projected elements, including detached nodes.
- Moved disabled/hidden/multiple state into scoped refresh-safe variables so
  attribute methods and property setters update one state source.
- Added bounded `type` default/normalization behavior and kept existing value,
  checked, and selected state channels separate.
- Added local, HTTP(S) content-worker, and same-origin frame assertions for
  property writes, attributes, command persistence, and refresh behavior.

## Tradeoffs and follow-up

The property table is deliberately bounded and exposed on the existing broad
host element surface; it does not claim per-interface Web IDL availability,
full URL reflection, enumerated-value validation, default-value/live-value
separation, or namespace-aware attributes. The existing native command and
Rust-owned state remain authoritative, avoiding an additional IPC protocol,
while complete Web IDL reflection and browser-wide parity remain issue #40
work.

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
- implementation checkpoint: `7ef89b95`
