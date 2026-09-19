# Native-engine browser slice 568: CSSOM variable recascade

Status: implementation checkpoint; issue #40 remains open.

## Objective

Keep computed native styles coherent after page CSSOM mutations. Setting or
removing an element attribute, including the `style` attribute used by
`CSSStyleDeclaration`, must invalidate the cached style vector before the next
layout or snapshot observes inherited custom-property values.

## Scope

- Invalidate cached computed styles after validated script-driven attribute
  writes and removals.
- Preserve the existing native CSSOM proxy, attribute command boundary,
  cascade precedence, custom-property inheritance, and direct/composite
  `font-size` variable resolution.
- Prove an inline custom property update changes a descendant's inherited
  `font-size` and removing the override recascades the stylesheet value.
- Keep broader CSSOM mutation semantics, registered custom properties, full
  CSS variable grammar, and complete CSS/Web IDL parity as explicit issue #40
  gates.

## Verification

Focused check passed:

- `cssom_custom_property_mutations_recompute_inherited_font_size`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1296 tests passed across two suites, one ignored, and 1295 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed:

- `scripts/check-rust-workspace.sh fast-check`
- `cargo build -p glass-browser --bin glass-browser --locked`
- `cargo build -p glass-dev --bin glass --locked`
- `cargo metadata --no-deps --format-version 1 --locked`

Documentation depth, coverage, and release-truth checks passed with the exact
counts recorded below after this task document was added:

- depth: 93 current guides and 19 substantive contracts
- coverage: 1218 Markdown files, 346 full-product MCP tools, 101 browser-only
  tools, 17 examples, and 22 public modules
- release truth: 83 current documents, 63 previous-version hits, 1367
  semantic audit hits, and zero current-claim failures

`cargo fmt --all -- --check` and `git diff --check` passed.
