# Native-engine browser slice 565: dynamic viewport recomputation

Status: implementation checkpoint; issue #40 remains open.

## Objective

Make the live native viewport mutable without navigation so viewport-dependent
CSS, layout state, and page viewport globals observe the updated dimensions on
both the in-process and content-process native paths.

## Scope

- Validate and apply a bounded `Viewport` update through the native engine,
  backend, and `BrowserRuntime` native seam.
- Synchronize the content-process owner before replacing the engine's live
  document viewport.
- Recompute viewport-dependent computed styles, advance the document revision,
  and reset root and nested scroll offsets to a safe origin after a resize.
- Initialize newly prepared native documents with the configured viewport rather
  than the parser default.
- Expose updated `innerWidth` and `innerHeight` values on the next page script
  evaluation.
- Keep resize-event dispatch, complete media-query re-selection, dynamic image
  resource timing, and complete CSS viewport-unit/API parity as explicit issue
  #40 gates.

## Verification

Focused checks passed:

- `viewport_font_sizes_use_configured_dimensions_and_bounds`
- `viewport_updates_recompute_css_and_script_dimensions`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1293 tests passed across two suites, one ignored, and 1292 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed:

- `scripts/check-rust-workspace.sh fast-check`
- `cargo build -p glass-browser --bin glass-browser --locked`
- `cargo build -p glass-dev --bin glass --locked`
- `cargo metadata --no-deps --format-version 1 --locked`

Documentation depth passed with 93 current guides and 19 substantive
contracts; coverage passed with 1215 Markdown files, 346 full-product MCP
tools, 101 browser-only tools, 17 examples, and 22 public modules; release
truth passed with 83 current documents, 63 previous-version hits, 1367
semantic audit hits, and zero current-claim failures.

`cargo fmt --all -- --check` and `git diff --check` passed.
