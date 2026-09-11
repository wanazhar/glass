# Native engine browser slice 215: CSS background image paint

Status: completed locally.

## Objective

Make a single CSS `background-image` URL a real native resource and paint,
including initial HTML, external HTTP(S) loading, document-wire transfer, and
page-script style mutation.

## Contract

- The bounded CSS cascade accepts `background-image: none`, CSS-wide values,
  and one `url(...)` image source. Quoted URLs, parenthesized semicolons, and
  the existing `!important`/layer/inline precedence path remain intact.
- Background image sources are represented by a bounded stable source ID and a
  validated source table. Content-process snapshots reject unknown, duplicate,
  malformed, or mismatched source/resource entries before parent publication.
- Inline data-URL PNG backgrounds decode through the existing image decoder and
  paint into the element's border box through the typed display list, software
  rasterizer, clipping, scrolling, and capture owners.
- External PNG backgrounds use the same HTTP(S) URL resolution, credential,
  redirect, referrer, cookie, mixed-content, CSP image policy, decoded cache,
  size limits, and broken-resource behavior as external `<img>` sources.
- Page-script `style.backgroundImage` mutation refreshes source identity,
  invalidates stale background resources, loads the new source, and publishes
  the updated paint-bearing document snapshot without a CDP or renderer path.
- Background image loading is visual resource work; unlike `<img>`, it does
  not dispatch an element `load` event.

## Implementation

`css.rs` adds a typed background-image cascade value, source identity, robust
declaration splitting for quoted/parenthesized values, and source collection
for stylesheet and inline declarations. `dom.rs` carries source IDs and
decoded background resources through `NativeDocumentWire`, validates their
ownership, retains only resources matching the current computed style, and
exposes source/resource lookup to the shared painter. The content process
hydrates external background images during initial loading and after script
mutations. `paint.rs` emits the same bounded `Image` command used by `<img>`
for the CSS background layer before borders.

## Tradeoffs and follow-up

This slice intentionally implements one URL layer stretched to the border box;
CSS repeat, position, size, multiple layers, gradients, masks, filters,
responsive image selection, SVG image resources, animation, and additional
image formats remain separate browser-completeness work. Source IDs are
bounded 32-bit hashes backed by source-table validation, which keeps computed
styles `Copy` and the wire compact while retaining a collision check during
source registration. Background loading is serialized with the existing
content mutation response for deterministic publication, so a slow image can
extend that response until the broader resource scheduler is implemented.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_png_background_images_share_css_paint_and_capture -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_background_png_through_document_wire -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_background_png_after_style_mutation -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `4b2a0d06`.
