# Glass native engine browser slice 314: Path2D and clipping

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Replace the page Canvas 2D `clip()` placeholder with a usable retained clip
contract and add reusable `Path2D` objects so ordinary drawing code can share
geometry between fill, stroke, hit queries, and clipping.

## Contract

- The page realm exposes a stable `Path2D` constructor and `instanceof`
  identity. A path can be empty, copied from another native `Path2D`, or
  created from a bounded SVG-style string containing `M`, `L`, `H`, `V`, `Q`,
  `C`, and `Z` commands.
- `Path2D` supports the common imperative geometry methods: `moveTo`,
  `lineTo`, `closePath`, `rect`, `arc`, `ellipse`, `quadraticCurveTo`,
  `bezierCurveTo`, and transformed `addPath`. Invalid sources, matrices, and
  non-finite coordinates fail explicitly instead of becoming silent geometry
  loss.
- `CanvasRenderingContext2D.fill`, `stroke`, `clip`, `isPointInPath`, and
  `isPointInStroke` accept reusable paths while retaining the current-path
  forms. The bounded `nonzero` and `evenodd` fill-rule names are validated.
- `clip()` retains an intersection of clip regions in drawing state. The
  software pixel gate applies those regions to fill, stroke, clear, text, and
  image writes; `putImageData` remains unaffected by clip, transform, and
  compositing as required by its separate pixel-write contract.
- `save()`/`restore()` copy clip regions and drawing properties but do not copy
  the current path. Clip state is re-established on every retained context
  operation and cannot leak between contexts.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The JavaScript Canvas owner now stores reusable paths as bounded point
subpaths, clones them on construction and clipping, and transforms them at
the context boundary. A small path-data parser handles the common absolute
and relative line/curve commands; unsupported grammar is rejected with an
explicit `SyntaxError`. The software raster's central blend gate checks every
clip region before mutating a pixel, so the behavior is shared by all drawing
owners rather than duplicated in each API method. Path overloads and query
methods use the same transformed representation, and clip snapshots are
deep-copied with the drawing-state stack.

## Tradeoffs and follow-up

This slice keeps the existing deterministic software raster and bounded point
flattening. It intentionally does not claim the complete SVG path grammar,
exact nonzero winding/hole behavior, browser-accurate stroke caps/joins,
subpixel antialiasing, DOMMatrix/Web IDL descriptor fidelity, worker-realm
Path2D installation, or WebGL/WebGPU paths. Those remain explicit issue #40
conformance and production-promotion work; the new APIs fail closed for
unsupported path syntax instead of silently accepting it.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused Canvas filter passes locally with 2 tests. The added witness
covers `Path2D` identity, copying, string parsing, transformed `addPath`, path
queries, clipping, and clip restoration alongside the prior Canvas and
OffscreenCanvas coverage. Remote CI, publication, release, and final
native/CDP parity or production-promotion claims remain pending the wider
issue #40 gates.
