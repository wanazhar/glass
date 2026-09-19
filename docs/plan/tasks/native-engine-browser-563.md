# Native engine browser task 563: bounded font-size custom properties

## Objective

Extend the native stylesheet cascade so `font-size: var(--name)` can consume a
bounded custom property value inherited through matched ancestors or supplied
by an inline style.

## Scope

- Accept ASCII custom-property names beginning with `--`, up to 64 bytes.
- Retain at most 16 custom-property declarations per style block or inline
  declaration list, with values bounded to 512 bytes.
- Apply important, inline, specificity, and source-order precedence.
- Resolve direct `var(--name)` font-size declarations through inherited custom
  properties, including bounded recursive references with a depth limit.
- Preserve invalid or missing references as ignored declarations so the normal
  inherited or initial font-size behavior remains available.

This is intentionally a bounded implementation checkpoint, not the complete
CSS Variables feature. Fallback syntax, composite `var()` expressions inside
`calc()`, CSSOM mutation, registered custom properties, and full CSS variable
grammar/parity remain open issue #40 gates.

## Verification

Focused native tests passed:

- `font_size_parser_accepts_direct_custom_property_reference`
- `inherited_font_size_custom_properties_resolve_with_inline_override`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1292 tests passed across two suites, one ignored, and 1291 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed.

Documentation depth passed with 93 current guides and 19 substantive
contracts; coverage passed with 1213 Markdown files, 346 full-product MCP
tools, 101 browser-only tools, 17 examples, and 22 public modules; release
truth passed with 83 current documents, 63 previous-version hits, 1367
semantic audit hits, and zero current-claim failures.

`cargo fmt --all -- --check` and `git diff --check` passed.
