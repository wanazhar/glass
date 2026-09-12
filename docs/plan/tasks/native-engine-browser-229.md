# Native engine browser slice 229: CSS background geometry

Status: completed locally.

## Objective

Make the existing single `background-image` URL behave like a real CSS
background layer instead of stretching one decoded image across the entire
layout box. The native CSS, display-list, raster, local-document, and
content-process owners must agree on repeat, position, size, and source
cropping.

## Contract

- The initial background repeat is `repeat`. `repeat-x`, `repeat-y`,
  `no-repeat`, and the two-value `repeat` syntax are normalized into the
  bounded native repeat model; unsupported repeat modes remain rejected by
  the existing diagnostic path.
- `background-position` accepts the bounded keyword, pixel, and percentage
  forms needed for horizontal/vertical placement. Percentages are stored as
  thousandths of the available leftover range, so `1000` means `100%`.
- `background-size` accepts `auto`, bounded pixel and percentage dimensions,
  and the aspect-ratio-preserving `cover` and `contain` modes. One automatic
  dimension derives from the decoded image ratio; a zero-sized result paints
  no image.
- A visible tile is emitted as an immutable `Image` command with a validated
  `source_rect`. The rasterizer maps the visible destination rectangle to
  that source crop and rejects source rectangles outside the full RGBA
  payload before sampling.
- Repeated tiles share one immutable `Arc<[u8]>` RGBA payload in the display
  list. The bounded tile budget prevents pathological repetition from
  creating an unbounded command list and returns a typed native limit error
  when exceeded.
- Inline declarations, stylesheet rules, CSSStyleDeclaration property
  mutation, local data images, and HTTP(S) content-process image resources
  all use the same computed-style and paint path. The typed computed-style
  record carries the three geometry values across the existing worker wire.

## Implementation

`css.rs` adds typed repeat, position, and size values, parsers, cascade
candidates, CSS-wide reset handling, computed-style accessors, and property
recognition. `paint.rs` resolves the image's intrinsic size and geometry,
derives mathematically aligned repeat starts, intersects partial edge tiles,
and emits source rectangles. `raster.rs` validates each source rectangle and
maps its pixels without changing the full-image path used by `<img>`.

The existing image loader and content-process resource transfer remain the
owners of URL policy, decoding, animation frames, and RGBA lifetime. No new
crate or alternate renderer was added.

## Tradeoffs and follow-up

The implementation keeps the one-background-layer boundary and represents
each visible repeated tile as a display command. This keeps clipping,
scrolling, capture, and hit-test ownership unchanged and makes partial source
crops explicit, at the cost of a bounded command budget for extremely small
images repeated over very large boxes. Tile payload memory is shared through
`Arc`, so repetition does not duplicate the decoded RGBA allocation.

Multiple background layers, gradients, `space`/`round` repetition, border-box
radius masking, and background shorthand remain separate CSS work items. They
must extend the same typed cascade and display-list owners rather than adding
an independent paint path.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_background -- --nocapture` (4 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_stylesheet_background_geometry_cascades_into_paint -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_png_background_images_share_css_paint_and_capture -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_background_png -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (17 passed, 0 failed)

Implementation checkpoint: `f97b20da`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
