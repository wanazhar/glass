# Native engine browser slice 210: inline PNG images

Status: completed locally.

## Objective

Make image elements participate in the same native layout, display-list,
software-raster, hit-test, scroll, and capture path as the rest of the
rendered document for bounded inline PNG data URLs.

## Contract

- `<img src="data:image/png,...">` accepts bounded base64 or percent-encoded
  PNG payloads and decodes them to a validated RGBA pixel buffer.
- Decoding is bounded by the existing native image byte/pixel policy and fails
  closed for malformed data, unsupported media types, invalid percent escapes,
  oversized payloads, and unsupported decoder output.
- Intrinsic image dimensions participate in inline sizing. A declared width
  or height preserves the intrinsic aspect ratio when the other dimension is
  omitted; two declared dimensions take precedence.
- The immutable display list carries the decoded image dimensions and pixels
  in a typed `Image` command. Software replay uses nearest-neighbor sampling,
  source-over alpha blending, the shared half-open clip owner, root-scroll
  translation, and the existing logical RGBA capture surface.
- Image layout boxes remain ordinary native hit-test owners, and the PNG
  capture path remains the only public raster artifact path.
- No network fetch, decoded-image cache, CSS `background-image`, SVG image
  element, animated format, or browser-wide image/Web IDL conformance claim is
  implied by this slice. Those use the resource-transfer work in the next
  issue #40 browser slices.

## Implementation

`native_engine::image` now owns bounded data-URL PNG decoding and RGBA
normalization. Layout derives the used inline image size from declared and
intrinsic dimensions. Paint emits one typed image command per rendered image;
the software surface validates its payload and replays it through clipping,
scrolling, and source-over alpha composition.

## Tradeoffs and follow-up

Keeping the first image path inline and dependency-neutral reuses the existing
`png` and `base64` dependencies and keeps the content-worker wire schema
unchanged. Re-decoding during layout and paint is intentionally simple for the
bounded slice; production image loading needs shared resource ownership,
deduplication, invalidation, HTTP caching, media-type policy, and transfer of
decoded resources across the content-process boundary. External images,
responsive sources, CSS image paints, SVG image resources, animated formats,
color management, and browser-complete image behavior remain active issue #40
implementation work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_ -- --nocapture`
  (3 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `ac9645f9`.
