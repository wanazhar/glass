# Native engine browser slice 209: SVG viewport clipping

Status: completed locally.

## Objective

Keep SVG content inside its declared viewport so overflow from mapped
user-space geometry cannot leak into screenshots or receive false hit-test
ownership.

## Contract

- Every rendered supported SVG descendant is intersected with each laid-out
  ancestor SVG viewport rectangle in document space.
- The existing clip owner carries that intersection to projected bounds,
  software fill/stroke replay, scroll translation, capture, and hit testing.
  CSS overflow clips and SVG viewport clips compose as one half-open rectangle.
- Geometry and layout bounds remain unmodified document-space bounds; clipping
  is applied at the shared consumer boundary so scroll and capture still apply
  exactly once.
- Shapes outside or fully beyond a viewport produce no visible pixels and do
  not win hit testing, while an eligible containing HTML box may remain the
  target outside the clipped shape.
- Empty or non-overlapping intersections fail closed without invalid display
  commands; no separate SVG raster or hit-test implementation is introduced.

## Implementation

The native layout clip walk now intersects CSS overflow clips with the laid-out
SVG ancestor boxes. `NativeLayoutSnapshot`, display-list construction,
software raster replay, viewport projection, and hit testing all consume the
same combined clip result.

## Tradeoffs and follow-up

The rectangle model is deterministic and composes with root scrolling, but it
does not yet model rounded SVG clip paths, nested SVG viewport placement, or
clip-path/mask semantics. CSS sizing/percentages, dash arrays, explicit
cap/join styles, gradients, markers, filters, text layout, and external
SVG/image resources remain separate issue #40 browser-completeness work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (11 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `6c87efa2`.
