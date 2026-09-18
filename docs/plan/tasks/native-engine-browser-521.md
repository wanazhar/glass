# Native-engine browser slice 521: numeric font weights

Status: complete on the current source line.

## Objective

Admit the CSS `font-weight` numeric range `1` through `1000` throughout the
native style, font-face, local-font, selection, paint, CSSOM, and variable-axis
owners. `normal` and `bold` remain the canonical aliases for 400 and 700.

## Contract

- Parse integer numeric weights in the CSS range 1–1000; reject zero, values
  above 1000, decimals, ranges, and unsupported relative keywords.
- Preserve inherited/cascade/CSS-wide behavior and serialize numeric computed
  weights as their canonical number while retaining `normal`/`bold` aliases.
- Use numeric distance for font-face matching and local font lookup while
  preserving the existing style and stretch priorities.
- Feed the requested numeric weight to an advertised variable `wght` axis;
  explicit descriptor and authored variation coordinates still win.
- Keep the bounded fixed-cell bold paint dilation for weights at or above the
  bold threshold and avoid changing static-font behavior outside selection.

## Tradeoffs and explicit boundary

This removes a fundamental CSS grammar gap without adding dependencies or
changing the two-crate boundary. Installed-font metadata remains conservatively
normal/bold, relative `lighter`/`bolder`, numeric `@font-face` ranges, optical
sizing, custom axes, hinting, color tables, WOFF2, and complete FontFace/Web
IDL parity remain separate issue #40 gates.

## Verification

The final record will include scoped check, focused CSS/font/CSSOM tests, the
affected `font_` group, the full locked `glass-browser` library regression,
and documentation inventory/depth/TUI gates. No remote CI, push, release,
tag, registry publication, or native/CDP parity certification is implied by
this local slice.

## Results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 11.66
  seconds after the final test-fixture corrections.
- The focused `font_weight` group passed 3/3 tests in 38.72 seconds; the
  inherited-text regression group passed 6/6 after its formerly-invalid `500`
  fixtures were moved to `1001`.
- The affected `font_` group passed 86/86 tests in 30.22 seconds.
- The full locked `glass-browser` library gate passed 1,221 tests with 1
  ignored and 0 failures in 63.13 seconds under `RUST_MIN_STACK=8388608`.
  The default local 2 MiB test-thread stack reproduces the already-known
  large-Clap `agent_readiness_commands_are_explicit` harness overflow; the
  same test passes at the repository CI setting of 4 MiB.
- Documentation truth passed for 1,171 Markdown documents with 83 current
  documents, 63 previous-version hits, 1,347 semantic-audit hits, and 0
  current-claim failures. Documentation coverage passed with 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules; depth
  passed with 93 current guides and 19 substantive contracts; TUI inventory
  passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, or native/CDP
  parity certification is claimed by this local slice.
