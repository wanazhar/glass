---
id: native-engine-009
scope: glass-browser/native-engine/layout-hit-testing
status: done
depends-on: [native-engine-008]
---

This task records the initial layout seed. Its explicit no-implicit-scroll
boundary was later extended by the browser action contract and native slice
255's bounded semantic target scroll-into-view behavior.

# Native bounded layout and hit-testing seed

## Objective

Give the native engine its first deterministic geometry owner and connect that
geometry to native point clicks without turning the slice into a general CSS
or rendering engine:

- classify the existing bounded `display` values for normal-flow layout;
- accept only bounded integer-pixel `width` and `height` declarations;
- compute a viewport-bounded layout snapshot for visible elements using a
  deterministic block/inline flow;
- expose Rust-only rectangles and point hit testing for native callers; and
- accept a native `point=<x>,<y>` click target that resolves through the layout
  hit to the nearest actionable semantic ancestor before mutation.

This is a layout and input-ownership seed. It does not add screenshots,
painting, scrolling, text shaping, floating/flex/grid/absolute positioning,
transforms, fractional CSS units, JavaScript, network access, or browser
compatibility claims.

## Context

- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-008.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`Viewport` remains the only layout coordinate space. Coordinates are integer
CSS pixels with origin `(0, 0)` at the viewport's top-left. A rectangle uses
half-open bounds: `x <= point.x < x + width` and `y <= point.y < y + height`.
Viewport dimensions and layout dimensions remain bounded by the existing
native viewport limit.

The layout seed uses normal flow only. Block-level elements occupy the
available content width and stack vertically. Inline elements and supported
controls occupy one deterministic line box. A bounded integer `width: Npx` or
`height: Npx` declaration may override the default dimension; percentages,
`auto` values, margins, padding, borders, positioning, flex, grid, transforms,
and font metrics are not implemented and do not match. `display:none`, the
existing explicit hidden signals, and `visibility:hidden` remove a node and
its descendants from layout and hit testing. `display:contents` contributes no
box while its visible descendants remain eligible.

The layout snapshot is a Rust-native inspection surface containing the current
viewport and deterministic element rectangles. It is derived from the current
DOM and stylesheet and is never a second mutable state owner. The existing
transport-neutral evidence result remains URL/title/text only; no geometry or
pixel data is added to compact evidence by this task.

`NativeDocument::hit_test` rejects non-finite/negative/out-of-viewport points,
then selects the deepest visible layout box at the point. Ties are resolved by
document order, with the later box winning. A point click resolves the deepest
hit to its nearest actionable semantic ancestor, so text inside a supported
button or link can activate that control. A point that hits no actionable
ancestor fails before focus, control state, effects, or revision mutation.
There is no implicit scrolling or nearest-target adjustment.

The native action target grammar adds `point=<unsigned-x>,<unsigned-y>` only
inside the Glass-owned native engine. Semantic `ref=`, `id=`, `role=`, `name=`,
and `text=` locators remain unchanged. The default Chromium path and the
Firefox/Safari external runtime paths do not interpret this native extension.
Native CLI documentation may advertise the extension, but the existing
transport-neutral `SemanticAction` shape and default feature behavior remain
unchanged.

Point and semantic actions share the same pre-mutation actionability gate.
Accepted actions advance the current revision exactly once and invalidate
revision-bound references; rejected actions preserve document state, effects,
and revision. No default link navigation is introduced.

## Tradeoffs and what this misses

- Integer normal-flow geometry is deterministic and cheap, but it misses real
  browser line breaking, fonts, margins, padding, replaced elements, and most
  CSS layout behavior.
- Point clicks make hit-test ownership executable, but they cannot scroll,
  activate overlays/z-index behavior, or model pointer events until paint and
  stacking contexts exist.
- Rust-only layout inspection keeps the stable backend contract small, but
  CLI/MCP/TUI surfaces cannot consume rectangles or screenshots yet.
- Derived layout recomputation avoids cache invalidation bugs, but repeated
  inspection is more expensive than a future explicit style/layout cache.
- No new dependency preserves the edit/build path, at the cost of owning even
  the small geometry algorithm inside Glass.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo check -p glass-browser --no-default-features --locked
cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
