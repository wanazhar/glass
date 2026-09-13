# Glass native engine browser slice 316: 2D DOM geometry objects

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Provide the standard 2D matrix and point objects used by Canvas and Path2D
callers. Replace private plain-object transform assumptions with stable
`DOMMatrix`/`DOMPoint` identity while keeping the software renderer's bounded
2D coordinate model explicit.

## Contract

- The page realm exposes `DOMMatrixReadOnly`, `DOMMatrix`, `DOMPointReadOnly`,
  and `DOMPoint` constructors with stable `instanceof` relationships and
  finite six-value 2D state.
- Matrix objects expose `a`/`b`/`c`/`d`/`e`/`f`, the common 2D `m*` aliases,
  `is2D`, `isIdentity`, JSON/typed-array projection, string projection, and
  immutable composition helpers (`multiply`, `translate`, `scale`, `rotate`,
  `skewX`, `skewY`, `inverse`).
- Mutable `DOMMatrix` objects additionally support `multiplySelf`,
  `preMultiplySelf`, translation/scale/rotation/skew self variants, and
  `invertSelf`, returning the receiver for chaining.
- `DOMPoint.matrixTransform()` returns a `DOMPoint`, preserving finite point
  coordinates and the existing bounded 2D transform math.
- Canvas object-form `setTransform()` and `transform()`, `getTransform()`,
  and `Path2D.addPath()` consume or return these objects through one matrix
  owner. Existing six-number Canvas calls remain supported.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The Canvas bootstrap owns a six-value matrix representation and constructs
the page geometry wrappers over it. Read-only methods return new
`DOMMatrixReadOnly` instances; mutable methods replace the receiver's
bounded state and return `this`. Point transforms use the same affine helper
as Canvas rasterization. Canvas transforms and `Path2D.addPath()` now call the
shared matrix parser, so DOMMatrix instances and compatible six-field
initializers follow one validation path.

## Tradeoffs and follow-up

This slice intentionally supports only the native renderer's finite 2D affine
matrix domain. It does not claim 3D/perspective matrices, sixteen-value
matrix state, complete CSS transform-list or matrix-string parsing, exact
DOMMatrix/Web IDL property descriptors, or full geometry API breadth. Invalid
or non-finite values fail explicitly, and no 3D value is silently flattened.
Those semantics, worker-realm geometry installation, complete Canvas/Web IDL,
and final native/CDP replacement remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused Canvas filter passes locally with 3 tests. The added witness
covers constructor identity, matrix composition, translation, point
transforms, typed-array export, object-form Canvas transforms, and raster
output. Remote CI, publication, release, and final native/CDP parity or
production-promotion claims remain pending the wider issue #40 gates.
