# Native engine browser slice 168: style and dataset DOM surface

Status: completed locally.

## Objective

Make common application-level attribute and inline-style code observable and
mutable in the native script projection without introducing a second style or
attribute owner.

## Contract

- `Element.style` exposes a live, CSSStyleDeclaration-like bounded surface for
  declaration parsing, `cssText`, indexed property names, camelCase and
  dashed property access, priority reads, `setProperty()`, and
  `removeProperty()`.
- Style writes serialize through the existing `setAttribute("style", ...)`
  and `removeAttribute("style")` command path, so Rust layout remains the
  authority for committed presentation.
- `Element.dataset` exposes a live DOMStringMap-like surface with camelCase
  `data-*` mapping, property reads/writes, enumeration, `in`, deletion, and
  `Object.defineProperty()` writes.
- Existing and detached elements in local/content-worker realms and
  same-origin frame projections install both surfaces and preserve object
  identity during an evaluation.
- Attribute names remain the existing native snapshot/command state; no
  independent JavaScript-only style or dataset state is authoritative.

## Implementation

- Added bounded declaration parsing/serialization with duplicate replacement,
  `!important` priority, live length/index/item access, property-name
  normalization, and CSS-style proxy assignment.
- Added a live dataset proxy backed by `getAttributeNames()`, including
  camelCase conversion, enumeration descriptors, deletion, and
  define-property writes.
- Installed the surfaces on local attached/detached elements and existing or
  detached same-origin frame-projected elements.
- Added local and frame integration witnesses for persistence, detached-node
  installation, style ordering/priority/removal, and dataset enumeration and
  deletion.

## Tradeoffs

The style parser is intentionally bounded to declaration-level syntax and
does not claim full CSS value validation, shorthand expansion, custom
property grammar, CSSOM descriptor parity, or computed-style behavior.
Dataset mapping covers the ordinary ASCII `data-*` application surface and
reuses native attribute validation rather than creating a separate DOMStringMap
implementation. These choices keep same-evaluation behavior useful while
preserving one Rust-owned mutation and layout path.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture`
  (441 passed, 0 failed, 0 ignored, 123.13s)
