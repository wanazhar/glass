# Native engine browser slice 198: bounded CSS grid layout

Status: completed locally.

## Objective

Make `display: grid` a real block-level layout mode in the Glass-owned native
renderer so ordinary two-dimensional card and panel layouts can share one
geometry contract across layout, paint, and hit testing.

## Contract

- `display: grid` is a distinct block-level mode rather than an inline or
  unsupported display value.
- `grid-template-columns` and `grid-template-rows` accept the bounded track
  grammar: fixed integer-pixel lengths, integer `fr` tracks, `auto`, the
  bounded `repeat()` form, and the existing `min-content`/`max-content`
  compatibility mapping.
- Track lists are bounded to eight tracks. Unsupported grammar remains
  observable through the existing CSS diagnostics surface.
- Visible element children are auto-placed in source order, row major, across
  the resolved columns. Fixed rows, gaps, and the bounded start/center/end and
  stretch alignment behavior produce deterministic integer geometry.
- The resulting layout boxes, descendants, text runs, display-list paint, and
  hit testing use the same translated coordinates.
- Stylesheet and inline declarations participate in the existing cascade;
  native mode owns the behavior without CDP or a compatibility fallback.

## Implementation

The CSS model now has typed bounded grid track lists and a distinct `Grid`
display value. Parsing, diagnostics, stylesheet declarations, inline style,
and computed-style resolution share the same bounded representation.

The layout engine resolves fixed, `fr`, and auto tracks, places eligible
children row-major, applies gaps and bounded alignment, and translates the
complete child artifact range before publishing layout, paint, and hit-test
results.

## Tradeoffs and follow-up

This slice deliberately keeps the grammar and implementation bounded. Explicit
`grid-column`/`grid-row` placement, named lines, `minmax()`, percentages,
auto-repeat, implicit track growth, full intrinsic track sizing, and anonymous
text grid items remain follow-up work. Those omissions are diagnosed or retain
normal-flow behavior; they are not silently represented as complete CSS Grid
support.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- focused CSS track parsing and cascade tests
- focused grid layout/paint/hit-test integration test
- focused unsupported-CSS diagnostics regression test
- full native integration target: 453/454 in the broad run; the lone
  abort-signal timing witness passed on an immediate exact rerun and is
  unrelated to grid

Implementation checkpoint: `bcc1d038`.

Remote CI, push, release, registry publication, and browser-parity
certification remain pending. Native mode remains non-promoted until the
issue-level production gates pass.
