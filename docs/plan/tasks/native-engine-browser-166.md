# Native engine browser slice 166: attached text-node identity

Status: completed locally.

## Objective

Preserve the native DOM's attached text nodes in every script projection so
live tree collections and node relationships represent the parsed document,
not only script-created element children.

## Contract

- The bounded script snapshot includes attached document, element, and text
  nodes with raw parent and ordered child links.
- Local/content-worker and same-origin frame realms reconstruct text hosts in
  native child order and retain a text host for the same native node across
  evaluations when its generation remains current.
- Text hosts expose `nodeType === 3`, `nodeName === "#text"`, `nodeValue`,
  `data`, `textContent`, `parentNode`, `parentElement`, `ownerDocument`,
  sibling traversal, removal, and live collection participation.
- Existing element `children` filters text nodes while `childNodes` includes
  them; markup/text synchronization and typed Rust mutation remain unchanged.
- Detached-node filtering, frame origin validation, and content-worker
  transfer limits remain authoritative.

## Implementation

- Added `NativeScriptNodeSnapshot` records for attached node kind, value,
  parent, and ordered children, while retaining the element-specific form and
  validity projection.
- Added persistent local text hosts and frame text projections, including
  bounded text mutation, data aliases, parent relationships, and markup
  synchronization.
- Rebuilt local and frame child lists from ordered native node links and
  connected document root child lists to the live collection/tree helper.
- Extended HTTP content and same-origin frame integration coverage for parsed
  text identity, node values, sibling order, parent links, and text-vs-element
  collection filtering.

## Tradeoffs

The snapshot now carries one bounded record per attached text node, increasing
script-bootstrap payload size in proportion to real text-node count. This is
necessary for correct DOM order and identity; it avoids inventing text nodes
from serialized `innerHTML`, which would lose boundaries and references. The
projection still does not claim complete WHATWG text normalization, comments,
fragments, ranges, mutation observers, or full Web IDL descriptor parity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` (441 passed)
- `git diff --check`
