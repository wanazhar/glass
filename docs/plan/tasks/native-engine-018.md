---
id: native-engine-018
scope: glass-browser/native-engine/side-specific-borders
status: done
depends-on: [native-engine-017]
---

# Native side-specific solid borders

## Objective

Extend the bounded native presentation and paint path from one uniform border
to four independently cascaded solid border sides:

- preserve the existing `border: Npx solid <color>` shorthand;
- accept `border-top`, `border-right`, `border-bottom`, and `border-left` in
  the same bounded integer-pixel solid grammar;
- let each side contribute its own layout inset and content origin;
- carry side widths and colors through the immutable display list; and
- replay the four side regions deterministically on the logical surface and
  after root viewport scrolling.

This is a side-specific presentation slice, not general CSS border support. It
does not add non-solid styles, border radius, border images, gradients,
corner-join fidelity, logical writing-mode sides, CSS-wide keywords, or
browser-parity claims.

## Contract

The existing uniform `border:` declaration sets all four physical sides. A
valid side declaration overrides only its named side according to the existing
specificity, source-order, and inline precedence rules. Invalid, negative,
non-pixel, unbounded, or non-`solid` values are ignored as unsupported
declarations. A side not supplied by any supported declaration has zero width
and no paint.

`NativeLayoutBox::rect` remains the outer border box. Layout derives the
content rectangle by subtracting the top/right/bottom/left border widths and
the existing uniform padding. `content-box` declarations add those insets to
the declared content dimensions; `border-box` declarations retain the outer
dimensions and reduce the content rectangle. Normal-flow margins and root
viewport scrolling retain their 017 behavior.

`NativeDisplayCommand::BorderRect` carries bounded physical side paint data in
document coordinates. Software replay translates the command through the
existing root scroll offset, clips it to the viewport/ancestor clip, and uses
a deterministic corner precedence. It does not expose a screenshot evidence
level or mutate document state.

## Tradeoffs

- Four physical sides improve common card/control fixtures while leaving
  logical properties and writing modes for a later CSS wave.
- Independent side cascade is more useful than treating the declaration as a
  single value, but it still omits CSS-wide reset keywords and full shorthand
  expansion rules.
- Side-region replay is deterministic and dependency-free, but it does not
  reproduce browser corner joins or anti-aliased border geometry.
- Per-side geometry reuses the existing integer box model, so fractional
  metrics, margin collapsing, positioning, flex/grid, and transforms remain
  outside the claim.
- No new dependency, stable evidence field, third crate, or automatic runtime
  path is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- native-engine unit/integration tests and capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused CSS, layout, display-list, raster, scroll, and backend tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. The focused and integration suites cover
independent physical-side cascade, shorthand preservation, invalid-side
rejection, content-box/border-box geometry, deterministic display commands,
clipped/scrolled software replay, and the default-feature boundary.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Native unit tests: 26 passed.
- Native integration tests: 27 passed.
- Strict default-feature and `native-engine` Clippy gates pass.
- Full locked `glass-browser` all-target/all-feature matrix: 808 passed, 1
  ignored.
- Documentation coverage: 432 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- Issue #40 records the exact local checkpoint commit and these verification
  results.
