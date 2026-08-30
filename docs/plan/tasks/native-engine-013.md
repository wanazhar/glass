---
id: native-engine-013
scope: glass-browser/native-engine/paint-clipping
status: done
depends-on: [native-engine-012]
---

# Native bounded paint clipping

## Objective

Add the first explicit clipping boundary to the native paint pipeline:

- recognize the narrow `overflow:hidden` presentation value;
- derive ancestor clip rectangles from the current layout revision;
- attach bounded logical clips to fill and text display commands; and
- enforce those clips during deterministic software replay.

This is a paint-clipping slice, not scrolling or general CSS. It does not add
scroll containers, scroll offsets, stacking contexts, borders, padding,
positioning, transforms, masks, images, fonts, screenshots, capture transport,
JavaScript, network access, GPU/window APIs, or new dependencies.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-012.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`

## Contract

The CSS presentation grammar accepts `overflow:hidden` as the only clipping
value. `overflow:auto`, `scroll`, `visible`, and all other overflow syntax
remain unsupported. The property does not create scrolling or change layout
size; it only clips descendant paint to the element's integer layout rectangle.

For each fill or text command, `NativeDisplayList::build` derives the
intersection of all `overflow:hidden` ancestor rectangles, including the
current element when applicable. A missing clip means that the command is
clipped only by the surface bounds. Empty intersections produce harmless
no-op paint. Clear remains a full-surface operation and is never clipped.
Commands retain their current deterministic order and matching document
revision. The clip rectangle is logical, half-open, and never scaled by the
viewport device-scale metadata.

`NativeSurface` intersects command clips with its own bounds before writing
pixels. Fill and text replay must never write outside the effective clip, and
out-of-bounds/overflowing rectangles remain safe. The clip is a renderer
artifact only: backend profiles, evidence, CLI, MCP, TUI, screenshot/capture
capabilities, and document revision behavior remain unchanged.

## Tradeoffs and what this misses

- Carrying a precomputed clip on each command keeps replay stateless and
  deterministic, but a retained renderer will eventually want a clip stack or
  shared clip IDs to reduce repeated metadata.
- Clipping only `overflow:hidden` gives one useful CSS boundary without
  inventing scroll behavior, but visible overflow, scrolling, padding, borders,
  and stacking order remain incomplete.
- Integer rectangle intersections are portable and cheap, but they do not
  model fractional geometry, transforms, masks, or antialiasing.
- Deriving clips by ancestor walk is easy to audit under current node limits,
  but style/layout invalidation and retained clip caches are future work.

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
- Native feature check, 21 native integration tests, the clipping unit test,
  and strict native-feature Clippy passed.
- No-default-feature check and strict Clippy passed.
- The full `RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets
  --all-features --locked` matrix passed, including the updated native
  integration suite and all browser-free targets.
- No dependency, transport capability, screenshot path, or third-crate
  boundary changed.
