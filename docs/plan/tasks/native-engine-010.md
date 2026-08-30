---
id: native-engine-010
scope: glass-browser/native-engine/display-list
status: done
depends-on: [native-engine-009]
---

# Native deterministic display-list seed

## Objective

Create the first renderer-owned artifact without adding a pixel renderer or
heavy graphics dependency:

- parse a bounded solid-color subset for `background-color` and `color`;
- derive a revisioned display list from the current layout snapshot;
- emit deterministic clear, fill-rectangle, and direct-text commands; and
- expose the display list only through the Rust-native inspection surface.

This is a display-list seed, not a software renderer. It does not add PNGs,
screenshots, rasterization, font shaping, images, borders, clipping, scrolling,
stacking contexts, GPU/window dependencies, JavaScript, network access, or
browser compatibility claims.

## Context

- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-009.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`NativeDisplayList` is derived from one `NativeDocument` revision and one
`NativeLayoutSnapshot` with the same revision and viewport. A mismatched
snapshot is rejected before commands are emitted. The list is immutable
inspection data; it is never a second mutable page-state owner and is never
included in transport-neutral compact evidence.

Commands are emitted in deterministic order: one white viewport clear, then
layout-box order. An element with a supported explicit `background-color`
emits one solid fill command. An element with direct visible text emits one
bounded text-run command at the top-left of its layout box. Text in nested
elements is owned by the deepest element with that direct text, so a parent
does not duplicate a child run. Explicit `color` controls text-run color;
otherwise text is opaque black. `display:none`, explicit hidden signals,
`visibility:hidden`, and non-rendered document elements emit no command; a
`display:contents` element emits no own command while visible descendants may
emit commands.

Colors are opaque RGBA values from the bounded named set `black`, `white`,
`red`, `green`, `blue`, `transparent`, three/six/eight-digit hexadecimal
notation, or `rgb(r,g,b)`. Unsupported functions, percentages, malformed
values, and all other paint properties are ignored. Color text and direct text
payloads remain bounded by the existing native text limits. The command count
is bounded by the native DOM-node limit and fails explicitly if that bound is
exceeded.

The existing backend profile and evidence contract do not gain capture or
paint capability from this task. Native layout and display-list methods are
Rust-only; no CLI/MCP/TUI operation silently returns pixels or display-list
payloads. Later rasterization must consume this list without mutating DOM,
layout, or revision state.

## Tradeoffs and what this misses

- A small color grammar makes the display artifact deterministic without
  pulling a CSS/color or graphics dependency, but it misses inheritance,
  alpha compositing, gradients, images, borders, and modern color spaces.
- Direct text runs give the future renderer a stable command boundary, but
  their positions and widths still use fallback integer geometry rather than
  shaping or font metrics.
- A derived list avoids invalidation bugs while the engine is small, but a
  future renderer will need explicit retained-list invalidation for throughput.
- Keeping the list Rust-only preserves the stable backend contract, but there
  is no screenshot or visual evidence until a later software-paint task.

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
