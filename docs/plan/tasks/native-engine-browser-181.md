# Native engine browser slice 181: reflected DOM attributes and live lookup

Status: completed locally.

## Objective

Make common DOM property writes behave like attribute-backed browser elements,
and keep document lookup methods authoritative during the current JavaScript
turn when newly created nodes are attached.

## Contract

- Element `id` and `className` getters reflect the current `id` and `class`
  attributes, returning the empty string when the attribute is absent.
- Assigning `element.id` or `element.className` performs the existing native
  attribute transaction; `setAttribute()` and `removeAttribute()` update the
  same reflected view without recursive commands.
- `getElementById()`, selector lookup, tag-name lookup, class-name lookup, and
  `document.activeElement` use the live attached tree rather than only the
  bootstrap snapshot. Detached nodes remain excluded until attached.
- Local, process-backed HTTP(S), and same-origin frame realms preserve the
  same property, lookup, command, and persistence behavior.

## Implementation

- Added shared reflected `id`/`className` accessors for local and projected
  elements, including newly created detached elements.
- Removed refresh-time property writes that could enqueue commands while
  hydrating a snapshot; attribute-backed getters now observe refreshed state.
- Added live document element traversal rooted at the document tree and routed
  document lookup/collection/active-element methods through it.
- Added local, HTTP(S) content-worker, and same-origin frame persistence tests.

## Tradeoffs and follow-up

The live lookup walk is intentionally bounded to the host-maintained attached
tree and uses identity deduplication; it does not broaden the existing parser
or invent a second node index. This fixes same-turn visibility without another
IPC path, but full Web IDL reflection, named properties, collection liveness,
namespace behavior, and the remaining browser-complete issue #40 gates remain.

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
- implementation checkpoint: `abbea205`
