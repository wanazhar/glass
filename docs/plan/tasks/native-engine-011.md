---
id: native-engine-011
scope: glass-browser/native-engine/software-surface
status: done
depends-on: [native-engine-010]
---

# Native bounded software surface

## Objective

Consume the 010 display-list seed with a deterministic, dependency-free
software surface:

- allocate a bounded logical RGBA pixel buffer for the display-list viewport;
- apply clear and solid-rectangle commands with deterministic source-over
  alpha compositing;
- draw a small built-in 5x7 ASCII glyph subset for text-run commands; and
- expose the surface only through the Rust-native inspection API.

This is a renderer kernel slice, not a screenshot feature. It does not add
PNG encoding, image files, capture transport, font loading or shaping, images,
borders, clipping regions, scrolling, stacking contexts, GPU/window APIs,
JavaScript, network access, or browser compatibility claims.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-010.md`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`

## Contract

`NativeSurface::from_display_list` allocates the display list's logical
`viewport.width * viewport.height` pixels, retaining the viewport's scale
factor as metadata only; it does not silently multiply the allocation by the
device scale. Allocation fails before memory growth when the surface exceeds
`MAX_NATIVE_SURFACE_PIXELS` or when the RGBA byte count cannot be computed.

Commands are consumed in list order. `Clear` writes every pixel exactly.
`FillRect` clips only to the surface bounds and applies integer source-over
RGBA compositing. Empty or fully out-of-bounds rectangles are harmless. Text
runs use a dependency-free 5x7 ASCII bitmap subset with a deterministic
six-pixel advance; unsupported glyphs advance without drawing. Text is clipped
to the logical surface and is rejected if a caller-constructed list contains a
text payload above the existing native text limit. The rasterizer rejects a
caller-constructed list above `MAX_NATIVE_DISPLAY_COMMANDS` before execution.

The resulting surface exposes width, height, immutable RGBA bytes, and bounded
point sampling through Rust. `NativeDocument::rasterize` and
`NativeEngine::rasterize` derive the display list from the current revision
first; they do not cache a mutable surface or advance document state. The
backend profile, compact evidence, CLI, MCP, TUI, and capture capability remain
unchanged.

## Tradeoffs and what this misses

- A fixed small glyph table keeps builds fast and behavior reproducible, but it
  misses Unicode shaping, kerning, font fallback, and accurate text metrics.
- Logical pixels avoid an unbounded device-scale allocation, but the surface is
  not a physical screenshot and cannot prove platform display fidelity.
- Integer source-over blending makes alpha colors observable without a
  graphics dependency, but it does not model CSS compositing, opacity groups,
  filters, or color management.
- Replaying the immutable list is simple and safe for the experiment, but a
  production renderer will need retained surfaces and invalidation scheduling.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo check -p glass-browser --no-default-features --locked
cargo clippy -p glass-browser --no-default-features --all-targets --locked -- -D warnings
RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets --all-features --locked
```

The increased test-thread stack is required by the existing large CLI parser
test on the current ARM Linux host; the unmodified command aborts there before
any native failure is reported.

## Completion evidence

- `cargo fmt --all -- --check` passed.
- Native feature check, 19 native integration tests, 3 raster unit tests, and
  strict native-feature Clippy passed.
- No-default-feature check and strict Clippy passed.
- The full `RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets
  --all-features --locked` matrix passed, including the native integration
  suite and all browser-free targets.
- The surface remains Rust-only: no CLI/MCP/TUI capture path, dependency, or
  backend capability was added.
