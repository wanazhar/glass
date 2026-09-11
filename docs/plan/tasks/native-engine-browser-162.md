# Native engine browser slice 162: structural DOM mutation

Status: completed locally.

## Objective

Make the most common structural DOM writes usable in the native page realm:
replace an element's child markup through `innerHTML`, remove an existing
element through `remove()`/`removeChild()`, and observe the committed subtree
through the same host objects on the next evaluation.

## Contract

- Setting `Element.innerHTML` replaces the attached child subtree with the
  bounded fragment parsed by the native HTML tokenizer.
- Existing descendants removed by replacement or `remove()` become detached
  from selectors, semantic projections, layout, visible-text evidence, and
  embedded-frame discovery.
- The serializer exposes deterministic lower-case element names, sorted
  attributes, double-quoted escaped values, escaped ordinary text, and raw
  script/style text within the bounded result.
- `removeChild()` validates the current parent relationship, queues the same
  typed removal command as `remove()`, and returns the removed host object.
- Fragment parsing observes the document node and DOM-depth quotas. Inserted
  scripts are represented in the tree but are not executed as a side effect of
  `innerHTML`, matching the ordinary DOM insertion rule used by this host.
- The command crosses local, content-worker, and same-origin frame owners only
  as bounded typed data. No JavaScript object or native engine pointer crosses
  a realm or process boundary.

## Implementation

- Added `SetInnerHtml` and `RemoveNode` to the typed script command protocol and
  the same-origin frame command allow-list.
- Added bounded fragment replacement, depth/node enforcement, subtree
  detachment, and canonical bounded markup serialization to `NativeDocument`.
- Added `innerHTML`, `remove()`, and `removeChild()` to local host elements and
  projected same-origin frame elements. The internal frame command dispatcher
  accepts both new operations.
- Added the snapshot field needed to refresh markup and preserve existing host
  element identities for unaffected arena nodes.
- Kept the content-process wire representation data-only; detached arena
  entries remain bounded tombstones and snapshots expose only attached nodes.

## Tradeoffs

The fragment parser reuses the existing bounded tokenizer, which keeps limits,
raw-text handling, and security validation in one place but does not claim full
WHATWG fragment insertion-mode conformance. The arena retains detached nodes
until the next document generation, preserving stable indices for unaffected
objects at the cost of finite mutation headroom. `createElement`,
`appendChild`, `insertBefore`, live `childNodes`/`children`, mutation-observer
delivery, and cross-realm listener identity remain separate browser-complete
gates.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --lib script_inner_html_replaces_subtree_and_serializes_markup -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
