# Native engine browser slice 230: CSS background shorthand

Status: completed locally.

## Objective

Support the common CSS `background` shorthand by expanding it into the
already shared color, image, repeat, position, and size owners. A shorthand
must load and paint the same bounded image resource as equivalent longhands in
local documents, stylesheets, CSSOM mutation, and HTTP(S) content-process
pages.

## Contract

- A bounded one-layer shorthand accepts the supported color, one `url()` or
  `none` image, repeat mode, position, and `/`-separated size components in
  the common order-independent form. The parser also accepts repeat after the
  size, as in `url(...) center/cover no-repeat`.
- Omitted shorthand components reset to their CSS initial values: transparent
  color, no image, repeating background, `0% 0%` position, and automatic image
  size. Standalone `inherit`, `initial`, `unset`, `revert`, and `revert-layer`
  expand to the existing typed local cascade states for every component.
- The shorthand's image URL is added to the same bounded source registry used
  by `background-image`, so external HTTP(S) loading, CSP/mixed-content and
  referrer policy, cache/resource hydration, typed document transfer, and
  broken-image handling remain single-owner behavior.
- Stylesheet and inline declaration order continues to control shorthand versus
  longhand precedence. `!important` is carried independently into each
  expanded component's existing paint cascade.
- Unsupported layers, gradients, attachment/origin/clip keywords, and other
  shorthand forms remain diagnostic failures rather than being partially
  interpreted as a different background.

## Implementation

`css.rs` adds a quote- and parenthesis-aware `/` separator, bounded shorthand
component recognition, CSS-wide expansion, property validation, and source
collection from valid shorthand URLs. The existing declaration record expands
one parsed shorthand into the five component candidates, so no parallel
computed-style or paint path is introduced.

The external background fixture and script mutation fixture now use the
shorthand form. Local coverage verifies color/image expansion and the
common `center/cover no-repeat` ordering; stylesheet coverage continues to
exercise the longhand cascade and the image filter covers every supported
image format and lifecycle path.

## Tradeoffs and follow-up

Keeping the shorthand intentionally one-layer and rejecting unsupported
subproperties makes invalid author input observable and preserves the bounded
resource/source model. It does not claim CSS-wide multi-layer shorthand
conformance. Multiple background layers, gradients, attachment/origin/clip,
and full CSS shorthand grammar should extend the same per-layer typed model
before being advertised as supported.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_css_background -- --nocapture` (5 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_stylesheet_background_geometry_cascades_into_paint -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_background_png -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_background_png_after_style_mutation -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (17 passed, 0 failed)

Implementation checkpoint: `0627c76b`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
