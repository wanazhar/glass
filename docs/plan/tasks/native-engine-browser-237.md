# Native engine browser slice 237: nested scrolling and scroll events

Status: completed locally.

## Objective

Make element-owned overflow usable through the same native layout, script,
paint, raster, hit-test, and content-process owners as root scrolling. A page
must be able to scroll a bounded nested `auto`/`scroll` container, observe the
correct non-bubbling event target, and retain the resulting state when the
native DOM projection is refreshed.

## Contract

- The typed CSS cascade distinguishes `visible`, `hidden`, `clip`, `auto`, and
  `scroll` for `overflow-x` and `overflow-y`, with independent axis behavior
  and the existing CSS-wide cascade/reset forms preserved.
- Layout derives bounded `clientWidth`/`clientHeight`, `scrollWidth`/
  `scrollHeight`, and maximum offsets for each element-owned overflow
  container. Explicit wide content is retained inside an `auto`/`scroll`
  ancestor so it can contribute to that container's scroll range.
- Nested offsets are independently validated and clamped. Descendant boxes,
  text, CSS overflow clips, SVG viewport clips, hit testing, display-list
  replay, software rasterization, and PNG capture consume the same projected
  geometry; root scrolling remains a separate viewport offset.
- `Element.scrollLeft`/`scrollTop`, `scrollTo`, `scrollBy`, and `scroll` update
  the owning container. `window.scrollX`/`scrollY`, the document root scroll
  aliases, and the existing root commands continue to use the window owner.
- Element scroll events target the scroller, do not bubble to `document`, and
  preserve `currentTarget` and `bubbles` values. Root scroll events target the
  window realm rather than the document node. Local and content-process event
  bridges deliver handler-generated scroll commands without replaying or
  duplicating the same event.
- Page scripts executed during initial navigation can establish root or nested
  scroll state before the first commit. The command queue crosses the
  content-process wire, is applied by the native owner, and is reflected by
  the first committed script snapshot.
- Local and HTTP(S) content-process documents share the same scroll metrics,
  geometry refresh, event ordering, and failure-atomic bounds. No CDP path,
  second renderer, or third crate is introduced.

## Implementation

`css.rs` adds typed `auto`/`scroll` overflow values and preserves the resolved
axis values through inheritance. `layout.rs` derives scroll-container metrics,
axis-aware root overflow, nested projection, projected clips, and hit-test
geometry. `paint.rs` and `raster.rs` carry nested offsets through one display
list replay path. `javascript.rs` adds the element/window scroll APIs,
snapshot state, and bounded non-bubbling event dispatch. `content_process.rs`
transfers scroll commands and applies the same event bridge in the isolated
worker. `dom.rs` and `engine.rs` carry scroll state through script geometry,
navigation commits, refresh, and publication.

The test witness also corrected a stale responsive-image expectation: WebP is
already supported by the native decoder, so the unsupported `<picture>` source
case now uses AVIF.

## Tradeoffs and follow-up

This slice uses bounded integer-pixel scroll metrics and rectangular overflow
projection. It does not add scrollbar painting, wheel physics, momentum,
scroll snapping, smooth-scroll timing, scroll-linked animation timelines,
visual viewport pinch zoom, or complete `body`/`document.scrollingElement`
alias behavior. Nested scroll restoration in history entries and cross-frame
scroll propagation remain later issue #40 work. Those features must extend the
same scroll-container and event owners rather than adding a parallel path.

The implementation keeps ordinary clipped-layout sizing compatible with the
existing native profile while allowing explicit overflow content where an
`auto`/`scroll` ancestor owns the range. This avoids changing unrelated
box-model witnesses while making the new nested-scroll contract observable.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_scrolls_root_and_nested_overflow_containers -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_scrolls_root_and_nested_overflow_containers -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_initial_page_script_scrolls_and_delivers_window_event -- --nocapture` (1 passed, 0 failed)
- Affected vertical-scroll, horizontal-scroll, root-overflow, fixed-position,
  overflow, box-model, dimension, logical-edge, paint-clipping, and responsive
  image witnesses passed after the shared projection repair.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` (516 passed, 0 failed, 338.55s)

Implementation checkpoint: `10b207ac`.

Remote CI, push, release, tag, registry publication, browser-parity, and
production-promotion claims are not made by this local checkpoint.
