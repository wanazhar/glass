# Glass native engine browser slice 282: computed style and media queries

Status: completed locally.

## Objective

Expose the native cascade and layout result to page JavaScript through the
common `getComputedStyle` and `matchMedia` surfaces required by ordinary web
application code.

## Contract

- Each element snapshot carries the computed style associated with the same
  native cascade/layout revision as its geometry.
- `getComputedStyle(element)` returns a bounded read-only CSSOM-like object
  with CSS property lookup, camelCase/kebab-case access, `length`, `item`,
  `getPropertyValue`, and `getPropertyPriority`.
- Common display, positioning, visibility, opacity, dimensions, box-model,
  color, border, overflow, text, font, flex, and gap values are projected into
  CSS-style strings or bounded used values.
- Inline style declarations take precedence for reads made during the same
  script turn, so a script can observe its own style mutation.
- `matchMedia` supports bounded `all`/`screen`, `not`/`only`, compound `and`,
  width/height, and orientation queries. Invalid dimensions do not match.
- The slice remains inside `glass-browser`'s native engine and does not add a
  third crate, invoke CDP, or alter backend selection.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

`NativeScriptElementSnapshot` now includes `NativeComputedStyle`. The page
realm keeps that snapshot with each element, projects its bounded fields
through a read-only Proxy, and refreshes the projection through the existing
`__glassRefresh` path. Geometry supplies used width and height while inline
declarations provide immediate same-turn overrides. Media-query evaluation is
derived from the configured viewport and rejects unsupported features rather
than treating them as matching.

## Tradeoffs and follow-up

Serializing the computed style with every element increases bootstrap payload
and memory cost, but gives scripts a coherent cascade/layout revision without
introducing a second style-recalculation protocol. The CSSOM projection is
deliberately bounded: pseudo-elements, complete property enumeration, source
URLs for background images, full background geometry, dynamic viewport-change
media events, and standards-complete CSSOM/Web IDL behavior remain open issue
#40 work. The native engine is not certified production-parity complete by
this slice.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_computed_style_and_media_queries --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script --locked -- --nocapture` — 17 passed

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
