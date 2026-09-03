---
id: native-engine-098
scope: glass-browser/native-engine/flex-wrapped-auto-margins
status: complete
depends-on: [native-engine-097]
---

# Native bounded wrapped flex auto margins

## Objective

Extend the existing wrapped flex line owners so eligible `margin:auto` edges
resolve independently inside each formed row or column line. Preserve the
single layout/artifact owner across forward and reverse directions,
`wrap-reverse`, line distribution, and every descendant consumer without
expanding into general Flexbox sizing.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-097.md`
- [CSS Flexible Box Layout Module Level 1: flex item margins and paddings](https://www.w3.org/TR/css-flexbox-1/#item-margins)
- [CSS Flexible Box Layout Module Level 1: aligning with auto margins](https://www.w3.org/TR/css-flexbox-1/#auto-margins)
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

This slice consumes the auto-edge state and deterministic share helper from
097. It adds no CSS grammar, dependency, crate, runtime, network, JavaScript,
storage, or artifact-pipeline surface.

Layout consumes wrapped auto margins only for the existing eligible flex
owners:

- the container is `display:flex` with `flex-wrap:wrap|wrap-reverse`; row and
  row-reverse use the existing bounded row owner, while column and
  column-reverse require the existing finite explicit-height column owner;
- visible direct element children retain the current eligibility, integer
  fixed-pixel/intrinsic item measurement, order sorting, gap, flex grow/shrink,
  basis, and min/max rules. Unsupported, hidden, anonymous-text, and fallback
  children retain the established normal-flow boundary;
- auto margins are zero while line membership, line cross-size formation, and
  the existing per-line flex sizing run. Auto edges therefore do not consume
  wrap capacity or inflate a line's provisional cross size;
- after each line has its final bounded size, including the existing
  `align-content` or stretch adjustments, positive main-axis remainder is
  divided equally across that line's auto main-axis edges. A line whose auto
  margins consume positive remainder receives no additional
  `justify-content` main-axis offset. With no positive remainder, the existing
  justify and overflow behavior remains unchanged;
- positive cross-axis remainder is divided equally across each item's auto
  cross-axis edges within its final line. Cross-axis auto margins suppress that
  item's normal `align-items`/`align-self` placement. If the item overflows its
  line, the auto margins resolve to zero and the existing bounded alignment and
  overflow path is retained;
- row and column reverse directions preserve source, semantic, and keyboard
  order while applying resolved margins to their physical edges. `wrap-reverse`
  reflects line placement through the existing physical cross-end mapping; it
  does not create a second line or margin coordinate system;
- every line-local resolved edge remains part of item outer geometry, gaps,
  line extent, `align-content`, root overflow, scrolling, point hit testing,
  display-list translation, software rasterization, viewport projection,
  capture, and semantic/source-order consumers;
- auto-height columns, new intrinsic or percentage sizing, fractional lengths,
  negative margins, logical writing modes, baseline alignment, grid, normal
  flow auto-margin centering, and browser-wide Flexbox remain outside this
  bounded extension and retain their existing fallback behavior.

## Tradeoffs

- Resolving auto margins per formed line makes the common wrapped toolbar,
  card-grid, and fixed-height column patterns observable while keeping line
  formation and cross-line distribution owned by the already-tested 070-096
  paths. It does not claim a general multi-line Flexbox implementation.
- Auto margins remain zero during wrapping and provisional line sizing. This
  matches the important Flexbox ordering and prevents a margin declaration from
  changing which line receives an item, but it means line-size contributions
  stay limited to the existing numeric/content measurements.
- Cross-axis resolution runs after `align-content` has chosen each final line
  size. This preserves the existing line-distribution owner and makes
  `wrap-reverse` a physical reflection of the same resolved line geometry.
- Integer quotient-plus-prefix-remainder allocation is deterministic and
  preserves exact bounded totals, at the cost of subpixel and fractional
  distribution semantics that remain explicitly unsupported.
- Removing the blanket wrapped-auto fallback is safe only behind the existing
  eligibility gates. Any unsupported child or sizing shape must continue to
  use normal flow rather than partially entering the wrapped flex owner.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation and documentation closeout are complete. The design
checkpoint is `a4b05f07`, the implementation checkpoint is `77a4b629`, and the
final test-only checkpoint is `866a8862`.

Passed locally:

- `cargo fmt --all -- --check` and `git diff --check`;
- focused `auto_margins` native integration: 6/6 passed;
- full `cargo test -p glass-browser --features native-engine --test
  native_engine --no-fail-fast`: 131/131 passed;
- `RUST_MIN_STACK=8388608 cargo test -p glass-browser --features
  native-engine --lib --quiet --no-fail-fast`: 888 passed, 1 ignored, 0
  failed;
- the default 2 MiB test-thread stack issue remains isolated to the existing
  `cli::args::tests::agent_readiness_commands_are_explicit` test; the feature
  library suite passes with the documented explicit 8 MiB stack;
- lockfile-pinned all-feature workspace Clippy and no-default-feature
  `glass-browser` Clippy passed with warnings denied;
- warning-denied locked workspace rustdoc passed;
- locked `glass-dev --bins` compilation passed and produced both expected
  development binaries;
- locked `cargo package` passed for both `glass-browser` and `glass-dev`, and
  the packaged dependency validator confirmed the exact `glass-browser`
  `0.3.14` dependency;
- locked offline fuzz all-target checking passed;
- the static version, feature-parity, release-documentation, TUI-shortcut,
  documentation-depth, documentation-coverage, reliability, public-adapter,
  and Web IR validators passed against the current checkout. Documentation
  coverage reports 512 Markdown files, 345 full-product MCP tools (100
  browser-only), 17 examples, and 22 public modules; the release-documentation
  audit reports 83 current documents, 57 previous-version hits, 574 semantic
  hits, and 0 current-claim failures;
- the row/column forward/reverse placements, `wrap-reverse`, per-line integer
  remainder allocation, `justify-content`, `align-content`, overflow and
  auto-height fallback behavior, nested geometry, display-list paint, software
  raster output, and hit testing are covered by the native integration tests.

Cleanup is recorded as an exact, post-validation operation: only the
regenerable `/tmp/glass-098-target` tree and
`/tmp/glass-098-release-documentation.json` report are eligible for removal
after process/open-file checks. The repository `target/` and `fuzz/target`
remain untouched; shared registries/toolchains, source, durable data, and
long-lived Glass processes are retained.

Remote CI remains pending because `main` is local-only and has not been
pushed. No browser-parity, release, registry-publication, or remote-
certification claim is part of this task.
