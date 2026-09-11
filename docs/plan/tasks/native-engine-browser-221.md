# Native engine browser slice 221: picture source selection

Status: completed locally.

## Objective

Extend the responsive-image owner from `img[srcset]` to the common
`<picture>` art-direction structure. A matching `<source>` must be selected in
document order before the `<img>` fallback, while every request continues to
use the existing bounded candidate, security, cache, lifecycle, paint, and
event owners.

## Contract

- Only direct preceding `source` children of the image's `picture` parent are
  candidates. The first source with a valid `srcset`, a matching bounded media
  condition, and a supported type wins; later sources are not considered.
- A source without `media` matches. The supported media grammar is the
  existing simple viewport-width condition: `min-width`, `max-width`, or an
  exact `width` expressed in bounded `px` or `vw` terms. Unsupported or
  malformed conditions fail closed for that source.
- A source without `type` matches. The current decoder contract accepts
  `image/png` (including a parameter suffix); other declared media types are
  skipped so the native engine never claims to decode a format it cannot
  validate.
- A winning source's `srcset` and `sizes` use the slice-220 density/width
  selection algorithm. If no source wins, the `img` `srcset` and then `src`
  fallback are selected exactly as before.
- The selected raw source is the only external image identity discovered by
  the content process. It is resolved for `currentSrc`, transferred through
  the validated document wire, and used for intrinsic dimensions, paint,
  cache lookup, and terminal `load`/`error` state.
- Mutating a source's `media`, `type`, `srcset`, or `sizes` re-evaluates the
  picture set. If the selected identity changes, stale image state is cleared
  and the new source follows the existing policy/cache/load/event path.
- Wire validation recognizes candidates declared by preceding picture sources
  without allowing an unrelated node or undeclared URL to enter the parent
  document state.

## Implementation

`NativeDocument::selected_image_source` owns picture precedence and delegates
candidate selection to the shared bounded `srcset`/`sizes` selector.
`picture_image_source_set` walks only the image's preceding sibling sources and
applies media/type gates. The source-identity validator accepts a source
candidate only from the image's own `src`, `srcset`, or preceding picture
sources. The JavaScript bootstrap exposes reflected `srcset`, `sizes`, and
`media` properties for `IMG`/`SOURCE`, so existing mutation commands invalidate
and refresh image resources without introducing a second resource owner.

## Tradeoffs and follow-up

This slice deliberately implements the common PNG path with a small
deterministic media grammar. It does not claim full CSS media-list parsing,
`not`/`or`/comma-list semantics, MIME sniffing, WebP/JPEG/GIF/SVG/AVIF
decoders, preload or fetch-priority scheduling, source `width`/`height`
attributes, or complete `HTMLSourceElement` Web IDL/descriptors. Those are
visible promotion items rather than silent fallback to CDP.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_selects_picture_source_and_reloads_img_fallback -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_images_ -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_reloads_the_selected_srcset_candidate_after_sizes_mutation -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_image_element_exposes_complete_intrinsic_dimensions_and_current_src -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_png_through_document_wire -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_external_image_error_event -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `fd8541e7`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
