# Native visual capture options (330)

Status: implemented locally in the native runtime.

This slice completes the native screenshot geometry contract used by the
shared CLI surface. Native PNG capture now honors the existing validation and
metadata shape for viewport, clip, scale, full-page, and semantic element
captures. Cropping happens after child-frame composition, element captures use
a temporary document-sized raster viewport when necessary, and full-page
captures render the document from an unscrolled origin without mutating the
live engine viewport or revision. Scaling uses deterministic nearest-neighbor
sampling in the software renderer.

Native JPEG and WebP encoding remain typed format denials; the native visual
path never writes PNG bytes under another extension or silently ignores a
requested option. The shared transport-neutral `CaptureFormat` path continues
to provide PNG and bounded PDF bytes. Capture geometry remains bounded by the
native software surface and axis limits.

## Tradeoffs

- Cropping is performed after nested-frame composition so iframe pixels remain
  aligned with their owner box.
- Off-screen element and full-page capture use a temporary layout/raster
  viewport; the live scroll position, script-visible viewport, and revision do
  not change.
- Nearest-neighbor scaling is deterministic and dependency-free, but it is not
  a browser GPU compositor filter. A later encoder/compositor slice can add
  higher-fidelity filtering without changing the capture option contract.
- Native screenshot output is PNG until native JPEG/WebP encoders are owned by
  the renderer; unsupported formats fail explicitly.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --locked native_visual_capture_honors_clip_scale_full_page_and_element_options -- --exact --nocapture`
- `cargo test --quiet -p glass-browser --locked native_backend_ -- --nocapture`
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

All evidence is local; this checkout has not been pushed and has no remote CI
result.
