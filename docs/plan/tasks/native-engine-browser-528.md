# Native-engine browser slice 528: validate COLR clip provenance

Status: complete on the current source line.

## Objective

Close a correctness hole in the bounded COLRv1 solid-paint path: a current
outline clip must still describe the outline being painted. A later outline
must not be treated as if it were already clipped merely because the clip
stack is balanced.

## Contract

- Assign each admitted outline a bounded generation and record that generation
  when `push_clip` accepts the current outline.
- Admit the no-mask current-outline fast path only when every active clip
  generation matches the outline painted by the corresponding `paint` call.
- Reject clip creation without an outline, replaced-outline paint, unbalanced
  clip/layer/transform state, and all existing unsupported paint forms through
  the recoverable monochrome fallback.
- Preserve the 16-level transform/clip bounds, 32-layer color bound, native
  glyph compositing, shaping/spacing, hit testing, wire shape, two-crate
  boundary, and explicit CDP migration backend.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- Locked scoped `glass-browser` library check passed in 12.99 seconds.
- Color-specific font tests passed `5/5`, including the replaced-outline clip
  regression, in 0.03 seconds after the locked build.
- No remote CI, push, release, tag, registry publication, or native/CDP
  parity certification is implied by this local fix.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`
