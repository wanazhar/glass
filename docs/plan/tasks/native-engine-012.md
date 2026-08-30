---
id: native-engine-012
scope: glass-browser/native-engine/style-inheritance
status: done
depends-on: [native-engine-011]
---

# Native inherited text color

## Objective

Extend the bounded CSS presentation seed with one real inherited property:

- resolve `color` through the DOM ancestor chain;
- preserve the existing cascade and inline-declaration precedence;
- feed the resolved color into display-list text runs and software replay; and
- keep the inheritance walk deterministic, bounded, and Rust-only.

This is a style-resolution slice, not general CSS. It does not add inherited
layout properties, selectors beyond the existing grammar, font metrics,
Unicode shaping, opacity groups, images, borders, clipping regions, scrolling,
screenshots, capture transport, JavaScript, network access, or new
dependencies.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-011.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`

## Contract

The initial computed `color` is opaque black. A matching explicit `color`
declaration on an element wins the existing specificity/source-order/inline
cascade; when no declaration wins, the element inherits the resolved color of
its nearest DOM ancestor. `transparent` remains an explicit color and must not
be confused with an absent declaration. The document root and nodes without a
colored ancestor use the black initial value.

Only `color` inherits in this slice. `background-color`, dimensions, display,
and visibility retain their existing non-inherited resolution and ancestor
visibility gate. Inheritance follows the parsed DOM parent links, including
through `display:contents`; it does not inspect rendered boxes or mutate the
document. The existing depth/node/document limits bound the walk.

`NativeDisplayList::build` consumes the resolved style for each layout box, so
direct text uses inherited color when appropriate. `NativeSurface` behavior
does not change beyond consuming that command color. The display list and
surface remain Rust-only inspection artifacts; backend profiles, evidence,
CLI, MCP, TUI, and screenshot/capture capabilities remain unchanged.

## Tradeoffs and what this misses

- Inheriting one high-value property improves nested content without a full
  cascade engine, but every other CSS inherited property remains unsupported.
- Walking ancestors during derived style lookup is simple and bounded, but a
  retained style cache will be needed for large documents and invalidation.
- Keeping the initial color opaque black makes text deterministic, but it does
  not model user-agent styles, visited-link colors, system colors, or color
  management.
- Treating `transparent` as an ordinary resolved RGBA value exposes alpha in
  the software surface, but it does not implement CSS opacity/compositing
  groups or blending contexts.

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
- Native feature check, 20 native integration tests, and strict
  native-feature Clippy passed.
- No-default-feature check and strict Clippy passed.
- The full `RUST_MIN_STACK=8388608 cargo test -p glass-browser --all-targets
  --all-features --locked` matrix passed, including the updated native
  integration suite and all browser-free targets.
- No dependency, transport capability, screenshot path, or third-crate
  boundary changed.
