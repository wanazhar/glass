# Native engine browser slice 185: hardened fragment HTML parsing

Status: completed locally.

## Objective

Make the shared detached-fragment parser preserve ordinary HTML token
boundaries for quoted attributes and special text elements across local,
process-backed HTTP(S), and same-origin frame realms.

## Contract

- Tag scanning honors single- and double-quoted attribute values, including
  `>` characters inside those values.
- HTML declarations, processing instructions, and comments do not become
  synthetic semantic nodes in the bounded fragment tree.
- `script` and `style` contents are consumed as literal raw text until their
  matching end tag and are never parsed as nested elements or executed.
- `textarea` and `title` contents are consumed as RCDATA until their matching
  end tag and decode supported entities into one text child.
- Existing nested element/text ownership, entity decoding, serialization,
  limits, and one authoritative host mutation transaction remain intact.

## Implementation

- Replaced the regex token pass in `populateDetachedFragment` with a bounded
  cursor scanner and quote-aware tag-end finder.
- Added case-insensitive special-element end-tag detection and separate
  raw-text/RCDATA materialization paths.
- Kept declaration/comment nodes out of the existing semantic element/text
  model and preserved malformed trailing input as bounded text where the
  current parser already does so.
- Added local, HTTP(S) content-worker, and same-origin frame witnesses for
  quoted attributes, declarations/comments, raw text, and RCDATA.

## Tradeoffs and follow-up

The scanner remains a bounded host parser rather than a complete WHATWG HTML
tree builder. Foreign-content integration, full malformed-input recovery,
template/comment node identity, adoption agency rules, scripting insertion
semantics, and complete Web IDL parity remain issue #40 completion work. The
parser is shared across all three realms, so this slice avoids divergent
behavior without creating a second staging tree.

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
- implementation checkpoint: `94dcd95f`
