# Native engine Phase 1/Phase 2, initial Phase 3, and runtime integration analysis

Status: Active implementation analysis for issue #40; Phase 0/1 and the first
Phase 2 semantic/action/form-control/parser slices, the initial Phase 3
presentation/layout slices, and the 008/009 runtime and input checkpoints are
committed locally, including the bounded 010 display-list seed and the bounded
011 software-surface seed, 012 style-inheritance seed, 013 paint-clipping
seed, 014 uniform solid-border paint, 015 bounded PNG capture, 017
viewport-scroll, and 018 side-specific-border slices. The 018 slice is
complete locally and its checkpoint evidence is recorded on issue #40. The
019 bounded dashed/dotted-border slice is complete locally and its checkpoint
evidence is recorded on issue #40. The 020 bounded physical border-radius
slice is complete locally and its checkpoint evidence is recorded on issue #40.
The 021 bounded inline line-placement slice is complete locally and its
checkpoint evidence is recorded on issue #40.
The 022 bounded fixed line-height slice is complete locally and its checkpoint
evidence is recorded on issue #40.
The 023 bounded direct-text-flow slice is complete locally and its checkpoint
evidence is recorded on issue #40.
The 024 bounded word-aware text-wrapping slice is complete locally and its
checkpoint evidence is recorded on issue #40.
The 025 bounded physical box-edges slice is complete locally; its complete gate
evidence is recorded below and its issue #40 checkpoint is updated from the
same commit.
The 026 bounded whitespace-boundary slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit.
The 027 bounded overflow hit-test/projection slice is complete locally; its
complete gate evidence is recorded below and its issue #40 checkpoint is
updated from the same commit.
The 028 bounded unsupported-CSS-diagnostics slice is complete locally; its
complete gate evidence is recorded below and its issue #40 checkpoint is
updated from the same commit. It makes ignored selectors, properties, values,
and malformed CSS observable through a bounded revisioned Rust surface without
changing stable backend evidence or the existing deterministic fallback
behavior.
The 029 bounded pixel-golden-capture slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit. It certifies the existing logical surface and PNG path with
one complete fixed fixture golden without expanding screenshot evidence or
renderer scope.
The 030 bounded descendant-selector slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit. It extends the narrow compound-selector grammar through
bounded descendant ancestor matching without claiming general CSS selector
conformance.
The 031 bounded-overflow-clip slice is complete locally; its complete gate
evidence is recorded below and its issue #40 checkpoint is updated from the
same commit. It accepts the non-scrolling `overflow: clip` value through the
existing bounded rectangular clip path without adding nested scrolling or
general CSS overflow conformance.
The 032 bounded-hard-line-breaks slice is complete locally: visible `<br>`
elements advance the existing integer inline-flow cursor without creating
synthetic semantic/layout/paint nodes or claiming general inline formatting
conformance. Its complete gate evidence is recorded below and its issue #40
checkpoint is updated from the same commit.
The 033 bounded-pre-line-breaks slice is complete locally: inherited
`white-space: pre-line` turns bounded source line-feed and carriage-return
boundaries into the same hard flow breaks while retaining space collapsing and
the no-general-CSS boundary. Its complete gate evidence is recorded below and
its issue #40 checkpoint is updated from the same commit.
The 034 bounded-preformatted-whitespace slice is complete locally: inherited
`white-space: pre` retains literal fixed-cell source whitespace, turns bounded
LF/CR/CRLF boundaries into the same hard flow breaks, and does not soft-wrap
preformatted segments. Its contract explicitly leaves tab stops, wide-line
reflow/scrolling, text alignment, font metrics, and general CSS whitespace
conformance outside the native claim; its complete gate evidence is recorded
in `docs/plan/tasks/native-engine-034.md` and its issue #40 checkpoint is
updated from the same commit.
The 035 bounded-pre-wrap-whitespace slice is complete locally: inherited
`white-space: pre-wrap` retains literal fixed-cell source whitespace and hard
LF/CR/CRLF breaks while splitting source runs at deterministic fixed-cell
soft-wrap boundaries. Its contract leaves browser line breaking, tab stops,
font metrics, shaping, baselines, bidi, justification, `break-spaces`,
`nowrap`, and general CSS whitespace conformance outside the native claim; its
complete gate evidence is recorded in `docs/plan/tasks/native-engine-035.md`
and its issue #40 checkpoint is updated from the same commit.
The 036 bounded-base64-data-url slice is complete locally: standard padded
base64 `data:text/html` payloads decode through the existing local resource
loader and real navigation/dispatcher path under a derived encoded bound and
the existing decoded UTF-8 document limit. Its contract keeps URL-safe or
whitespace-tolerant encodings, percent-encoded base64, subresources, network,
filesystem, script, storage, and arbitrary schemes outside the native claim;
its complete gate evidence is recorded in
`docs/plan/tasks/native-engine-036.md` and its issue #40 checkpoint is updated
from the same commit.
The 037 bounded-fragment-navigation-history slice is complete locally: raw
fragments are separated from local resource lookup, same-resource navigation
is document-preserving and revisioned, and explicit Rust back/forward traversal
is implemented without inventing a transport-level history operation. Its
completion evidence is recorded in `docs/plan/tasks/native-engine-037.md` and
issue #40.
The 038 bounded-local-link-activation slice is complete locally: semantic local
anchor clicks route through the existing action and navigation owner while
relative, remote, and other unsupported href destinations remain rejected. Its
completion evidence is recorded in `docs/plan/tasks/native-engine-038.md` and
issue #40.
The 039 bounded-fragment-target-scroll slice is complete locally: exact visible
local `id` targets position the root viewport at a bounded document-space top,
and each history entry retains its saved root-scroll offset for same- and
different-resource traversal restoration. Its implementation and validation
evidence are recorded in `docs/plan/tasks/native-engine-039.md` and issue #40.
The 040 bounded-relative-local-links slice is complete: semantic links now
resolve same-host fixture-relative URLs while preserving the existing
parse-before-commit, anchor-scroll, and failure-atomicity boundaries. Its
contract and validation evidence are recorded in
`docs/plan/tasks/native-engine-040.md`.
The 041 bounded-percent-decoded-fragment-target slice is complete locally:
local fragment navigation decodes bounded UTF-8 percent escapes before exact
visible `id` matching while preserving the existing duplicate-safe scroll,
history, and failure behavior. Its contract and validation evidence are
recorded in `docs/plan/tasks/native-engine-041.md` and issue #40.
The 042 bounded-legacy-name-fragment-target slice is complete locally: it adds
a unique exact `<a name>` fallback only when no matching `id` exists, while
preserving the existing decoded fragment, layout, scroll, history, and
duplicate-safe boundaries. Its contract and validation evidence are recorded
in `docs/plan/tasks/native-engine-042.md` and issue #40.
The 043 bounded-text-fragment-target slice is complete locally: it matches a
simple one-run `#:~:text=start` or `#:~:text=start,end` request after per-term
bounded UTF-8 decoding while preserving the existing ID/name precedence,
layout, scroll, and history boundaries. Its complete gate evidence is recorded
below, in `docs/plan/tasks/native-engine-043.md`, and on issue #40.
The 044 bounded-text-fragment-affix slice is complete locally: it adds exact
same-run prefix/suffix affixes around the existing matcher, with the same
raw-comma parsing, per-term UTF-8 decoding, scroll, history, and fail-closed
boundaries. Its complete gate evidence is recorded below, in
`docs/plan/tasks/native-engine-044.md`, and on issue #40. The 045 bounded
root-horizontal-scroll slice is complete locally: it derives measured overflow
width, independently clamps x/y, and reuses the existing viewport, hit-test,
display-list, raster, and history owners. Its complete gate evidence is
recorded below, in `docs/plan/tasks/native-engine-045.md`, and on issue #40.
The 046 bounded-nowrap-whitespace slice is complete locally: it accepts
inherited and inline `white-space: nowrap`, collapses supported whitespace
through the existing fixed-cell policy, disables soft wrapping, and reuses the
measured root horizontal scroll/projection path. Its complete gate evidence is
recorded below, in `docs/plan/tasks/native-engine-046.md`, and on issue #40.
The 047 bounded-inherited-line-height slice is complete locally: it propagates
the existing positive-pixel `line-height` through the DOM style walk while
preserving explicit child declarations and height precedence. Its complete
gate evidence is recorded in `docs/plan/tasks/native-engine-047.md` and issue
#40. The 048 bounded-clip-aware-root-overflow slice is complete locally: it
measures text contribution through the existing paint/projection clip owner so
fully clipped text cannot create a false root horizontal scroll range. Its
complete gate evidence is recorded in `docs/plan/tasks/native-engine-048.md`
and issue #40. The 049 bounded-axis-specific-overflow slice is complete
locally: it cascades bounded `overflow-x`/`overflow-y` clips independently
through the same owner. Its complete gate evidence is recorded in
`docs/plan/tasks/native-engine-049.md` and issue #40. The completed
dependency-ordered task is `native-engine-050`: bounded physical min/max
width/height constraints through the existing box-model owner. The completed
`native-engine-051` task adds bounded CSS opacity groups through transparent
display-list layers and software compositing. No general stacking, transform,
filter, animation, or browser compositor parity is implied. The completed
`native-engine-052` task adds bounded inherited `text-align` for fixed-cell
direct text and supported inline flow, with no logical-direction or
justification parity. The completed dependency-ordered `native-engine-053`
task adds bounded functional `rgba(R, G, B, A)` alpha parsing for background,
border, and text colors through the existing `NativeColor` and software
source-over owners. It does not imply CSS Color 4, color-space, or
color-management parity. The completed dependency-ordered `native-engine-054`
task adds
bounded inherited `text-decoration: none|underline` through immutable text
commands and fixed-cell software replay. It does not imply font metrics,
decoration propagation, or browser text-paint parity. Its implementation and
validation evidence are recorded in the task file and issue #40. The completed
dependency-ordered `native-engine-055` task adds bounded inherited ASCII
`text-transform: none|uppercase|lowercase` during fixed-cell layout so
wrapping, text-fragment matching, display-list projection, and root-overflow
measurement consume the same presentation text. Semantic source text remains
unchanged; Unicode case mapping, locale behavior, and font-specific glyph
metrics remain outside the boundary. Its implementation and validation
evidence are recorded in the task file and issue #40. The completed
dependency-ordered `native-engine-056` task adds bounded non-negative
fixed-pixel `text-indent` to the first line of block containers, clamps the
effective value to retain one fixed cell, and keeps inline/`display:contents`
text on the containing block's flow. Its implementation and local validation
evidence are recorded in the task file and issue #40; remote CI remains
pending until this branch is pushed.
The completed dependency-ordered `native-engine-057` task adds bounded inherited
non-negative fixed-pixel `word-spacing` to the existing fixed-cell text-flow
owner. Collapsed ASCII separator spaces and literal ASCII spaces in the
supported preformatted modes receive the extra advance before wrapping,
fragment projection, alignment, display-list generation, raster replay, hit
testing, and root-overflow measurement. Negative, relative, percentage,
keyword, Unicode-whitespace, and browser word-boundary behavior remain
outside the boundary. Its implementation and local validation evidence are
recorded in the task file and issue #40; remote CI remains pending until this
branch is pushed.
The completed dependency-ordered `native-engine-058` task adds bounded inherited
non-negative fixed-pixel `letter-spacing` to every rendered fixed-cell
character in each emitted fragment. Its measured advance composes with
`word-spacing` on ASCII spaces before wrapping, preformatted chunking,
fragment projection, alignment, display-list generation, raster replay, hit
testing, and root-overflow measurement. Fragment and line boundaries remain
hard boundaries; browser pair-boundary, Unicode, font-metric, negative,
relative, percentage, `normal`, and conformance semantics remain outside the
boundary. Its implementation is committed locally as `cb191a3`; the task file
records the local validation evidence, and remote CI remains pending until this
branch is pushed.

The completed dependency-ordered `native-engine-059` slice adds inherited
`font-weight: normal|bold|400|700` to the fixed-cell text presentation path.
`normal`/`400` retain the current glyph replay and `bold`/`700` apply a
deterministic one-pixel horizontal glyph dilation without changing advances,
layout, semantics, hit testing, overflow, or text-fragment coordinates.
Real font selection, metrics, shaping, variable weights, and browser text
rendering parity remain outside the boundary. The implementation is committed
locally as `21fcff5`; the task file records the local validation evidence, and
remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-060` slice adds inherited
`font-style: normal|italic` to the fixed-cell text presentation path.
`normal` retains the current glyph replay and `italic` applies a deterministic
bounded row-dependent horizontal shear through the existing clipped software
rasterizer. Bold dilation, underline, spacing, opacity, scrolling, and capture
compose through the same immutable text command; advances, layout, semantic
text, hit testing, overflow, and text-fragment coordinates remain unchanged.
Oblique forms, angles, font selection/loading/metrics, shaping, anti-aliasing,
and browser text-rendering parity remain outside the boundary. The
implementation is committed locally as `2992eb8`; the task file records the
local validation evidence, and remote CI remains pending until this branch is
pushed.

The completed dependency-ordered `native-engine-061` slice adds inherited
`word-break: normal|break-all` to the bounded collapsed fixed-cell flow path.
`normal` retains word-aware wrapping; `break-all` permits deterministic
character-boundary splitting for every collapsed word while preserving the
existing separator, spacing, fragment, overflow, and semantic owners. `pre`,
`pre-wrap`, and `nowrap` retain their established behavior. Unicode
line-breaking, grapheme policy, hyphenation, `overflow-wrap`, bidi, writing
modes, font metrics, and browser conformance remain outside the boundary. The
design is `14d7fc4`, the implementation is `479f3a3`, and the documentation
closeout is `433d6fd`; local validation evidence is recorded in the task file
and remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-062` slice adds local
`text-overflow: clip|ellipsis` to the bounded single-line fixed-cell path.
`clip` retains the existing full visual run under a horizontal overflow clip;
eligible `ellipsis` blocks replace an overflowing suffix with a
spacing-aware fixed-cell ASCII `...` marker while preserving the full semantic
source text. The implementation is restricted to one direct text child in a
rendered `nowrap` block with finite horizontal clipping; multi-line
truncation, nested inline formatting, Unicode ellipsis behavior, and browser
conformance remain outside the boundary. The design is `7e488aa`, the
implementation is `e4c5bb1`, and local validation evidence is recorded in the
task file; remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-063` slice adds inherited
`vertical-align: baseline|top|middle|bottom` to the existing fixed-cell
inline-flow line-item owner. `baseline` preserves the current top-origin
behavior; `top`, `middle`, and `bottom` apply bounded integer offsets within
the existing line box and move an inline item's boxes and text artifacts
together. Font metrics, typographic baselines, lengths, percentages, bidi,
writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. The design is `7721df2`, the implementation is
`facd2f6`, and local validation evidence is recorded in
`docs/plan/tasks/native-engine-063.md`; remote CI remains pending until this
branch is pushed.

The completed dependency-ordered `native-engine-064` slice adds bounded
`display: flex` single-row placement for eligible direct element children. The
container keeps the existing block box model; items use explicit or intrinsic
fixed widths, retain source order and margins, and do not grow, shrink, wrap,
reverse, or distribute free space. Containers with meaningful direct text,
`display: contents`, or visible `<br>` children fall back to the existing
normal flow so content is not dropped. The contract is recorded in
`docs/plan/tasks/native-engine-064.md`; the design is `a05bdd6`, the
implementation is `7c38354`, and local validation evidence is recorded there.
Remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-065` slice adds one non-negative
fixed-pixel `gap` between visible direct element items in an eligible bounded
flex row. Hidden and `display:none` items do not consume a gap position, while
ineligible containers retain the existing normal-flow fallback. Multi-value or
percentage gap grammar, `row-gap`, `column-gap`, flex distribution, wrapping,
and general Flexbox remain outside the boundary. The contract is recorded in
`docs/plan/tasks/native-engine-065.md`; the design is `062998c`, the
implementation is `28735c4`, and local validation evidence is recorded there.
Remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-066` slice adds bounded
`justify-content:flex-start|center|flex-end|space-between` to eligible
fixed-width flex rows. Positive free space is placed before the row or
distributed across existing gaps with deterministic integer rounding; an
overflowing row keeps a zero leading offset and remains root-scrollable.
Flex growth/shrink, wrapping, direction, cross-axis alignment, `space-around`,
`space-evenly`, and general Flexbox remain outside the boundary. The contract
is recorded in `docs/plan/tasks/native-engine-066.md`; the design is
`a53b10f`, the implementation is `3fe5306`, and local validation evidence is
recorded in the task file. Remote CI remains pending until this branch is
pushed.

The completed dependency-ordered `native-engine-067` slice adds bounded
non-inherited signed `order` values in `-1024..=1024` to eligible fixed-width
flex rows. Visual items sort by `(order, source_index)` before existing gap and
`justify-content` distribution, while semantic DOM/source order stays stable.
The contract is recorded in `docs/plan/tasks/native-engine-067.md`; design is
`09f3b00`, implementation is `a713b6e`, and local validation evidence is
recorded in the task file. Remote CI remains pending until this branch is
pushed.

The completed dependency-ordered `native-engine-068` slice adds bounded
non-inherited `align-items:flex-start|center|flex-end` to eligible fixed-width
single-row flex rows. It uses an explicit resolved content height when present,
otherwise the maximum visible item outer height, then shifts complete item
artifact ranges by deterministic integer cross-axis offsets. The contract is
recorded in `docs/plan/tasks/native-engine-068.md`; design is `a0488ef`,
implementation is `6b55b9c`, and current-source documentation closeout is
recorded in this checkpoint. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-069` slice adds bounded
non-inherited `flex-direction:row|row-reverse` to eligible fixed-width
single-row flex rows. `row` remains equivalent to 068; `row-reverse` performs a
margin-aware physical right-to-left walk of the order-sorted visual sequence,
maps bounded justification to the physical edges, and preserves cross-axis
alignment, shared subtree artifacts, non-negative coordinates, root scrolling,
and semantic/source order. The contract is recorded in
`docs/plan/tasks/native-engine-069.md`; design is `7fea901`, implementation is
`be11f49`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-070` slice adds bounded
non-inherited `flex-wrap:nowrap|wrap` to eligible fixed-width flex rows.
`nowrap` remains equivalent to 069; `wrap` partitions the order-sorted visible
items by measured outer width and the existing gap, then reuses per-line
justification, physical row/reverse placement, cross-axis alignment, shared
subtree artifacts, and root overflow consumers. Lines stack top-to-bottom with
maximum-item outer heights; `align-content`, cross-axis gaps, flex sizing, and
`wrap-reverse` remain excluded. The contract is recorded in
`docs/plan/tasks/native-engine-070.md`; design is `8772a6a`, implementation is
`5c16185`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-071` slice adds bounded
non-inherited `align-content:flex-start|center|flex-end|space-between` to
wrapped flex rows. It distributes only positive cross-axis free space from an
explicit content height after 070 line formation, translates complete line
artifact ranges after per-line `align-items` placement, and preserves the
default `flex-start`/`nowrap` geometry. The contract is recorded in
`docs/plan/tasks/native-engine-071.md`; design is `71bd380`, implementation is
`cc1d602`, the final single-line coverage test is `050d41b`, and local
validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-072` slice adds bounded
non-inherited `align-content:space-around` to wrapped flex rows. It uses the
completed 071 line-record owner and places each line at a saturating integer
slot center from positive explicit-content-height remainder, preserving line
membership, per-line alignment, complete artifact translation, and
`nowrap`/auto-height behavior. The contract is recorded in
`docs/plan/tasks/native-engine-072.md`; design is `c8e5170`, implementation is
`a1c8b56`, and local validation and cleanup evidence are recorded in the task
file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-073` slice adds bounded
non-inherited `align-content:space-evenly` to the same wrapped flex rows. It
uses equal leading, inter-line, and trailing integer slots from positive
explicit-content-height remainder while preserving the 072 line-record,
artifact, overflow, and semantic owners. The contract is recorded in
`docs/plan/tasks/native-engine-073.md`; design is `23b62a3`, implementation is
`b4833a9`, and local validation and cleanup evidence are recorded in the task
file. The default-stack overflow in one existing large-Clap parser test is
documented there; the full native library suite passes with an explicit 8 MiB
test-thread stack. Remote CI remains pending because the branch is local-only.
The completed dependency-ordered `native-engine-074` slice adds bounded
non-inherited `flex-wrap:wrap-reverse` to the same eligible fixed-width rows.
It preserves source-order line formation, reverses physical cross-axis line
stacking, reuses every bounded `align-content` value, and requires signed
complete-artifact translation from provisional line origins. Its contract is
recorded in `docs/plan/tasks/native-engine-074.md`; design is `5f6c61a`,
implementation is `96fd15c`, and local validation and cleanup evidence are
recorded in the task file. The previously documented default-stack issue in
one large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending because the branch is local-only.
The completed dependency-ordered `native-engine-075` slice adds bounded
explicit non-inherited `align-content:stretch` to wrapped fixed-width rows. It
expands formed line heights by deterministic integer shares of positive
explicit content-box remainder, rebuilds provisional line origins, and reuses
the existing per-line alignment and signed wrap-reverse artifact pass. The
design is `4637863`, implementation is `e26c0a4` with the diagnostics-fixture
correction in `cc8b538`, and local validation and cleanup evidence are recorded
in `docs/plan/tasks/native-engine-075.md`. The default-stack issue in one
existing large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-076` slice adds explicit bounded
non-inherited `align-content:normal` to wrapped fixed-width rows. It aliases
the completed 075 line-box stretch owner for positive explicit cross-axis
remainder, preserving normal and wrap-reverse artifact consumers while
keeping the established omitted-value `flex-start` fallback unchanged. The
design is `b6c647e`, implementation is `e3933b3`, and local validation and
cleanup evidence are recorded in `docs/plan/tasks/native-engine-076.md`. The
default-stack issue in one existing large-Clap parser test remains a harness
follow-up; the full native library suite passes with an explicit 8 MiB
test-thread stack. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-077` slice adds explicit
bounded non-inherited `row-gap` spacing between adjacent formed lines in
eligible wrapped fixed-width flex rows. It includes that spacing once in the
existing cross-line occupied-size/free-space owner while preserving the
current main-axis-only `gap` behavior. The design is `99d70ef`, implementation
is `4b27b15`, and local validation and cleanup evidence are recorded in
`docs/plan/tasks/native-engine-077.md`. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-078` slice completes the
bounded flex gap family with one- and two-value integer-pixel `gap`, explicit
`row-gap`/`column-gap` longhands, and declaration-order-aware shorthand and
longhand cascade. One-value `gap` intentionally supplies both axes in this
new boundary, so wrapped-row goldens that previously relied on the 077
main-axis-only compatibility behavior now reflect the correct cross-axis
spacing. The design is `c6ebecd`, implementation is `1bca33f`, and local
validation and cleanup evidence are recorded in
`docs/plan/tasks/native-engine-078.md`. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-079` slice adds bounded
non-inherited integer `flex-grow` weights to the existing row-flex owner.
Positive line free space is allocated before `justify-content` with a
deterministic prefix-floor policy; existing `max-width` caps freeze items and
redistribute remainder among the remaining positive weights. Negative free
space remains a no-shrink overflow case, and the slice does not add
`flex-shrink`, `flex-basis`, fractional factors, columns, or a second geometry
owner. Its design contract is in `docs/plan/tasks/native-engine-079.md`;
design is `a45fb01`, implementation is `1a930a3`, and local validation and
cleanup evidence are recorded in that task file. Remote CI remains pending
because the branch is local-only.

The completed dependency-ordered `native-engine-080` slice adds bounded
non-inherited integer `flex-shrink` weights to the same row-flex owner. Negative
line free space is allocated using original-base-width weighted prefix-floor
shares; effective outer `min-width` floors freeze items and redistribute the
remaining deficit. Zero factors and exhausted minimums retain explicit
overflow, while wrapping remains a base-size line-formation decision. Its
design contract is in `docs/plan/tasks/native-engine-080.md`; design is
`46227de5`, implementation is `b1414931`, and local validation and cleanup
evidence are recorded in that task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-081` slice defines bounded
`flex-basis:auto|Npx` sizing for the same row-flex owner. Explicit pixel bases
override item `width`, convert through the existing box-sizing and min/max
helpers, remain unclamped before line formation, and feed the completed
grow/shrink allocation with the original base width. `auto` delegates to the
existing width/intrinsic compatibility path. Its design is `04cc4a3e`,
implementation is `299c93f9`, and local validation and cleanup evidence are
recorded in `docs/plan/tasks/native-engine-081.md`. Remote CI remains pending
because the branch is local-only.

The completed dependency-ordered `native-engine-082` slice defines bounded
`flex` shorthand expansion into the existing grow, shrink, and basis
components. It accepts `none`, `auto`, bounded integer factor forms, and
bounded pixel/`auto` bases; valid longhands after a shorthand override only
their own component, while a later shorthand resets all three. Unsupported
CSS-wide, percentage, fractional, and ambiguous forms remain typed
diagnostics. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-082.md`; design is `71d1060d`, implementation is
`40f6fb1c`, and local validation and cleanup evidence are recorded there.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-083` slice defines bounded
`flex-flow` shorthand expansion into the existing direction and wrap
components. It accepts one direction token, one wrap token, or one of each in
either order; omitted components reset to their initial row or nowrap value.
Unsupported columns, duplicates, CSS-wide, logical-direction, and ambiguous
forms remain typed diagnostics. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-083.md`; design is `80c6836e`, implementation is
`0291bf90`, and the strict-Clippy fix is `16e6d9aa`. Focused/full validation and
cleanup evidence are recorded there. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-084` slice defines bounded
non-inherited `align-self:auto|flex-start|center|flex-end` for eligible direct
flex items. `auto` resolves to the parent `align-items` value at the existing
cross-axis placement decision, while explicit values override only that item.
Stretch, baseline, logical, CSS-wide, and ambiguous forms remain typed
diagnostics. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-084.md`; design is `36085e9c`, implementation is
`f2f99f66`, and focused/full/strict/documentation/cleanup evidence is recorded
there. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-085` slice defines bounded
`place-content` shorthand expansion into the existing `align-content` and
`justify-content` components. One shared token covers their common bounded
values; two tokens use explicit cross-axis/main-axis order. Unsupported
CSS-wide, logical, safe/unsafe, ambiguous, and unsupported justify forms
remain typed diagnostics. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-085.md`; design is `adc61a1f`, implementation is
`02f866e6`, and focused/full/strict/documentation/cleanup evidence is recorded
there. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-086` slice extends bounded
non-inherited `align-self` with `stretch` for eligible direct flex items.
Auto-height items fill the existing line cross size; explicit heights remain
unchanged and use the bounded flex-start fallback. The complete contract and
tradeoffs are in `docs/plan/tasks/native-engine-086.md`; design is `f92b7b9a`,
implementation is `5ea2c8d1`, and strict layout lint cleanup is `2c07c749`.
Focused, full, strict, documentation, release-certification, and exact
target-cleanup evidence are recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered `native-engine-087` slice extends the parent
`align-items` grammar with explicit `stretch`. Children whose `align-self`
remains `auto` reuse the 086 used-size and complete-artifact owner; explicit
child overrides remain authoritative and explicit heights stay fixed under the
bounded flex-start fallback. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-087.md`; design is `4708f663` and
implementation is `e0d051ce`. Focused/full/strict/release-certificate,
documentation, and exact-target evidence are recorded in the task file. The
first all-in-one certification run exposed one environment-sensitive Rust
Analyzer probe; its exact retry passed. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-088` slice adds explicit parent
`align-items:normal`. In the supported row/row-reverse flex context, `normal`
resolves `align-self:auto` through the completed stretch used-size owner while
the computed keyword remains distinct and the omitted native fallback stays
`flex-start`. The complete contract and tradeoffs are in
`docs/plan/tasks/native-engine-088.md`; design is `162543f1` and implementation
is `19d8d3f6`. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-089` slice adds explicit child
`align-self:normal`. In the supported row/row-reverse flex context, explicit
`normal` reuses the completed stretch used-size owner regardless of the
parent's `align-items` value, while `align-self:auto` remains
parent-controlled and the computed keyword remains distinct. The complete
contract and tradeoffs are in
`docs/plan/tasks/native-engine-089.md`; design is `95caf4a9` and
implementation is `1ee55c43`. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-090` slice adds explicit
`justify-content:space-around` to the bounded fixed-width row/row-reverse
flex-line owner. Positive main-axis free space is distributed with
deterministic integer cumulative offsets around existing item/gap/margin and
flex-sizing geometry, while row-reverse mirrors the offsets and all downstream
artifacts remain shared. The complete contract and evidence are in
`docs/plan/tasks/native-engine-090.md`; design is `97c69d9d` and
implementation is `d814784f`. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-091` slice adds explicit
`justify-content:space-evenly` to the same bounded fixed-width row/row-reverse
flex-line owner. Positive main-axis free space is distributed into equal
deterministic integer slots after existing item/gap/margin and flex-sizing
geometry, while row-reverse mirrors the offsets and all downstream artifacts
remain shared. The complete contract and evidence are in
`docs/plan/tasks/native-engine-091.md`; design is `be1a8e20` and
implementation is `118590f7`. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-092` slice adds explicit
`justify-content:normal` to the same bounded fixed-width row/row-reverse
flex-line owner. The computed keyword remains distinct while used placement
reuses the completed `flex-start` path, including explicit gaps, margins,
wrapping, reverse placement, and shared downstream artifacts. Its contract and
evidence are in `docs/plan/tasks/native-engine-092.md`; design is `4cbaf338`
and implementation is `ce19db39`. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-093` slice adds explicit
`justify-content:stretch` to the same bounded fixed-width row/row-reverse
flex-line owner. The computed keyword remains distinct while used placement
reuses the completed `flex-start` path, including explicit gaps, margins,
wrapping, reverse placement, and shared downstream artifacts. The shared
one-token and two-token `place-content:stretch` forms are valid through the
existing axis parser composition. Its contract and evidence are in
`docs/plan/tasks/native-engine-093.md`; design is `273b31db` and
implementation is `653f025e`. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-094` slice extends the bounded
Flexbox owner to explicit `flex-direction:column|column-reverse` for eligible
fixed-height, no-wrap containers. It maps the existing vertical main-axis
justification and row-gap, bounded grow/shrink/basis, horizontal item
alignment, reverse placement, and complete descendant/artifact consumers
without adding a second geometry owner. Auto-height columns, wrapping,
column-gap line distribution, logical writing modes, and browser-wide Flexbox
remain outside the contract in `docs/plan/tasks/native-engine-094.md`. Design
is `aea47b17`, implementation checkpoints are `7dc92517` and `4e212151`, and
local certification/cleanup evidence is recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-095` slice extends the completed
column owner to fixed-height `flex-wrap:wrap` containers. It forms vertical
main-axis lines, consumes `column-gap` across the horizontal cross axis,
reuses per-line integer flex sizing/justification and existing `align-content`,
and keeps complete descendant/artifact consumers on the shared geometry owner.
`wrap-reverse`, auto-height columns, and browser-wide Flexbox remain outside
the contract in `docs/plan/tasks/native-engine-095.md`. Design is `598228a7`,
implementation is `96ea62a3`, and local certification evidence is recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-096` slice extends the same
fixed-height column/column-reverse owner to `flex-wrap:wrap-reverse`. It keeps
095's order-sorted vertical line formation, per-line sizing and justification,
row/column gap mapping, and complete artifact ownership, then reflects line
boxes and cross-axis item alignment from the physical horizontal cross-end.
Source, semantic, and keyboard order remain unchanged. The bounded contract,
tradeoffs, exclusions, and local evidence are in
`docs/plan/tasks/native-engine-096.md`; design is `f5026f3c` and implementation
is `6862aff6`. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-097` slice extends the no-wrap
row, row-reverse, column, and column-reverse owners to bounded `margin:auto`
edges. Auto margins remain distinct from numeric box edges, resolve as zero
during flex sizing, absorb positive main-axis remainder before
`justify-content`, and absorb positive cross-axis remainder before
`align-items`/`align-self`, with deterministic integer remainder allocation.
Wrapped lines, auto-height columns, intrinsic or percentage sizing, logical
writing modes, and normal-flow auto margins remain outside the contract in
`docs/plan/tasks/native-engine-097.md`. Design is `2901c830`, implementation is
`815794ce`, and local certification/cleanup evidence is recorded in that task
file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-098` slice extends that auto-edge
owner into eligible wrapped row/row-reverse and fixed-height
column/column-reverse lines. Auto edges remain zero during line formation and
per-line sizing, then absorb positive main-axis remainder before justification
and positive cross-axis remainder before alignment after the existing final
line and `align-content` owners settle. `wrap-reverse`, reverse physical-edge
mapping, deterministic integer shares, and complete artifact consumers remain
on the shared geometry path. Auto-height columns, new intrinsic/percentage
sizing, normal-flow auto margins, logical writing modes, baseline alignment,
grid, and browser-wide Flexbox remain outside the bounded contract in
`docs/plan/tasks/native-engine-098.md`. Design is `a4b05f07`, implementation is
`77a4b629`, and the final test-only checkpoint is `866a8862`; remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-099` slice adds inherited
`direction:ltr|rtl` to the bounded Flexbox axis mapping. Rows use the inline
direction for physical main-start placement; columns preserve their vertical
main axis while reflecting horizontal cross-axis alignment and wrapped line
stacking. Reverse and wrap-reverse combinations, auto margins, and complete
artifact consumers remain on the shared owner. Non-flex text bidi, logical
properties, vertical writing modes, and browser-wide directionality remain
outside the bounded contract in `docs/plan/tasks/native-engine-099.md`. The
design is `2f18abb4`, the implementation is `3bf658e8`, and local gate
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-100` slice adds bounded
inherited `text-align:start|end` to the fixed-cell inline-flow owner. Logical
start/end resolve through inherited `direction:ltr|rtl`; physical
`left|right`, center, source order, and the existing no-bidi/shaping boundary
remain unchanged. Wrapped lines, inline boxes, hard breaks, whitespace,
spacing, indentation, overflow, and capture continue through the shared
line-flush owner. The design is `2dae80fc`, implementation is `3380978c`, and
local gate and paired-package evidence is recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-101` slice adds inherited
`text-align:justify` to the same fixed-cell inline-flow owner. It stretches
only emitted collapsed ASCII separators on soft-wrapped non-final lines with
positive free space, using deterministic source-order integer remainder
allocation and carrying the extra advance through text runs, display-list,
raster, overflow, scrolling, capture, and existing inline-item translations.
Preformatted flow, bidi/shaping, language-specific line breaking, logical
properties, vertical writing, and full text conformance remain outside the
bounded contract. The design is `959cbbc9`, implementation is `8ff29aa1`, and
the explicit word-spacing acceptance test is `15cf0c85`; complete local gate
evidence is recorded in the task file, with exact isolated-target cleanup
recorded in the final cleanup checkpoint. Remote CI remains pending because
the branch is local-only.

The completed dependency-ordered `native-engine-102` slice adds inherited
`text-align-last:auto|left|center|right|start|end` to the same fixed-cell
inline-flow owner. Only the final non-empty line flushed by a block's normal
completion path uses the explicit value; soft-wrap justification remains
owned by 101 for non-final lines, while forced-break paths, bidi/shaping,
language-specific line breaking, logical properties, vertical writing, and
full text conformance remain outside the bounded contract. The contract is
`docs/plan/tasks/native-engine-102.md`; design is `fc396200`, implementation
is `1157bf49`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-103` slice adds explicit
inherited `text-align-last:justify` to the same fixed-cell inline-flow owner.
Only the final non-empty line reaching the normal block completion flush may
distribute positive free space across eligible collapsed ASCII separators,
reusing 101's deterministic integer spacing through 102's final-line owner.
Ordinary soft-wrap justification, preformatted/pre-wrap, break-all, truncated,
forced-break and intermediate-boundary paths, bidi/shaping, logical
properties, vertical writing, and full text conformance remain outside the
bounded contract. The completed task is
`docs/plan/tasks/native-engine-103.md`; design is `f261773f`, implementation is
`be5757ae`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-104` slice adds inherited
`text-justify:auto|none|inter-word` to the existing fixed-cell text-spacing
owner. `none` suppresses positive separator expansion for both 101 ordinary
soft-wrap justification and 103 explicit final-line justification, while
`auto` and `inter-word` retain the bounded ASCII-space algorithm. The property
does not enable either alignment mode, change authored `word-spacing`, or
alter preformatted, break-all, truncated, forced-break, bidi/shaping, logical,
vertical-writing, or full text-conformance paths. The completed task is
`docs/plan/tasks/native-engine-104.md`; design is `6ca523f1`, implementation is
`d83b24e4`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-105` slice extends the existing
inherited fixed-cell `text-decoration` owner from `none|underline` to the
single bounded values `none|underline|overline|line-through`. The line state
reaches immutable text commands, clipping, scrolling, opacity replay, and
software raster without changing layout, source order, or semantic
projections. Decoration colors, thickness, style, offsets, combinations, font
metrics, shaping, bidi, vertical writing, and browser-wide text conformance
remain outside the contract. The completed task is
`docs/plan/tasks/native-engine-105.md`; design is `9002ae13`, implementation is
`ebfefefa`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered `native-engine-106` slice extends the
inherited fixed-cell `text-decoration` owner to distinct multi-token
combinations in the `text-decoration` shorthand. One immutable three-bit line
set reaches display commands, clipping, scrolling, opacity replay, capture,
and software raster without changing layout, overflow, hit testing, or
semantic/source order. `text-decoration-line` longhand semantics, decoration
colors, thickness, style, offsets, font metrics, shaping, bidi, vertical
writing, and browser-wide text conformance remain outside the contract. The
completed task is `docs/plan/tasks/native-engine-106.md`; design is
`c0525afb`, implementation is `baf680ee`, and complete local gate plus
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered `native-engine-107` slice adds a bounded
local `text-decoration-color` value to the same fixed-cell decoration owner.
The existing `NativeColor` parser supplies a separate decoration color beside
glyph color in one immutable text command, preserving shared origin, width,
clipping, scrolling, opacity, capture, and software-raster consumption while
keeping glyph and line pixels distinct. Decoration-origin propagation,
`currentColor` syntax, style/thickness/offset, and complete color/text
conformance remain outside the bounded contract. The completed task is
`docs/plan/tasks/native-engine-107.md`; design is `c5177215`, implementation is
`2474efe6`, and current local gate evidence includes the 144/144 native suite
plus the full workspace package tests, lint, docs, packaging, fuzz, and static
validators. The ancillary bounded diagnostics-wait fix is `64247bd4`. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-108` slice exposes the same
fixed-cell line bitset through the bounded `text-decoration-line` longhand.
`none`, `underline`, `overline`, and `line-through` combinations reuse the
shorthand's declaration-order-aware slot and one immutable text command,
preserving the 107 glyph/decoration color path and shared geometry, clipping,
scrolling, opacity, capture, hit-test, and semantic consumers. Full CSS
longhand inheritance/decoration propagation, style/thickness/offset, and text
conformance remain outside the bounded contract. The completed task is
`docs/plan/tasks/native-engine-108.md`; design is `ea1bf881`, implementation is
`4981ff82`, and local evidence includes 145/145 native integration tests,
898 feature-library tests plus one expected ignored test, the complete
browser/dev suites, strict lint and rustdoc, locked packaging, nightly/offline
fuzz, and static documentation validators. Remote CI remains pending because
the branch is local-only.

The completed dependency-ordered `native-engine-109` slice adds bounded
inherited `text-decoration-style:solid|dashed|dotted` to the existing
fixed-cell decoration owner. Solid remains the default; dashed and dotted
reuse the existing integer `NativeBorderStyle` pattern helper with one-pixel
lines anchored at each emitted run origin, preserving the 108 longhand/color
path and all shared artifact consumers. Wavy/double styles, thickness,
offsets, decoration-origin propagation, and full CSS conformance remain
outside the contract. Design `93034cbf`, implementation `81069084`,
inherited-style coverage `b8ae87dc`, and local gate/cleanup evidence are
recorded in `docs/plan/tasks/native-engine-109.md`. Remote CI remains pending
because the branch is local-only.

The completed dependency-ordered `native-engine-110` slice adds bounded
inherited `text-decoration-thickness:1px|2px|3px|4px` to the existing
fixed-cell decoration owner. Each selected line preserves its current line-y
origin and paints a positive-y pixel band; dashed and dotted periods reuse the
existing integer pattern helper scaled by thickness, while the value travels
through one immutable text command and replay clamps externally constructed
values to 4px. Arbitrary/font-derived/fractional values, centering, offsets,
baseline metrics, fragment continuity, and full CSS conformance remain outside
the contract. Design `1623c5f1`, implementation `157da4ad`, and complete local
gate evidence are recorded in `docs/plan/tasks/native-engine-110.md`. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-111` slice adds bounded
inherited signed fixed-pixel `text-underline-offset` from `-4px` through `4px`
to the existing fixed-cell underline owner. Negative values move only the
underline toward decreasing y and positive values toward increasing y;
overline and line-through keep their existing origins, while the 110
thickness band and style helper remain shared. `auto`, percentages, fractional
and font-derived metrics, decoration-origin propagation, fragment continuity,
and full CSS conformance remain outside the contract. Design `e2651dfd`,
implementation `215b02a8`, current-claim docs `e76a732a`, and complete local
gate and cleanup evidence are recorded in
`docs/plan/tasks/native-engine-111.md`. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered `native-engine-112` slice is recorded in
`docs/plan/tasks/native-engine-112.md`. It adds inherited
`text-decoration-style:double` through a dedicated text-decoration style type,
painting two solid bands with the existing resolved thickness and one
transparent separator pixel. The 111 underline offset, line origins, x
anchoring, clipping, scrolling, opacity, capture, and immutable text command
remain shared; border styling, layout, fragment continuity, and browser-wide
conformance remain outside the boundary. Implementation is `3bdd3b54`; final
local gate and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-113` boundary is recorded in
`docs/plan/tasks/native-engine-113.md`. It adds inherited
`text-decoration-style:wavy` through the dedicated text-decoration style type,
painting a continuous eight-pixel fixed-cell wave with phase
`[0,1,2,1,0,-1,-2,-1]` and the resolved thickness at each x column. Phase
resets at each immutable text run; the 112 double behavior, 111 underline
offset, line origins, clipping, scrolling, opacity, capture, and semantic
consumers remain shared. CSS metric centering, fragment continuity,
antialiasing, and browser-wide conformance remain outside the boundary.
Implementation is `0c6a9ddc`; complete local gate and cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The dependency-ordered `native-engine-114` implementation is complete and
recorded in `docs/plan/tasks/native-engine-114.md`. It adds inherited
`text-decoration-skip-ink:auto|none` through a dedicated value in the existing
immutable text command. `auto` suppresses underline and overline pixels only
where the same fixed-cell text run emits glyph ink; `none` preserves existing
replay and line-through remains unchanged. Wavy, thickness, offset, clipping,
scroll, opacity, capture, hit testing, semantics, source order, and layout
remain shared. Font metrics, shaping, fragment continuity, and browser-wide
conformance remain outside the design. Implementation is checkpointed at
`ceedf1d8` and synchronized documentation at `d276d7b1`; all required local
certification gates pass. Exact cleanup evidence and the local-only remote-CI
boundary are recorded in the task file.

The dependency-ordered `native-engine-115` implementation is complete and
recorded in `docs/plan/tasks/native-engine-115.md`. It adds inherited
`text-decoration-skip-spaces:none|all` through a dedicated value in the
existing immutable text command. `all` skips decoration pixels over same-run
ASCII-space intervals, including the existing word, letter, and final-line
justification spacing, for underline, overline, and line-through; `none`
preserves replay. `start`/`end`, Unicode whitespace, line-boundary semantics,
fragment continuity, and browser-wide text conformance remain outside the
bounded design. Implementation is checkpointed at `1c0bd484`; local native/dev,
strict, package, fuzz, security, formatting, and static certification is
complete and exact cleanup is recorded in the task file. The browser
registry-backed publish dry-run passed; the dev registry-backed verification is
blocked by the immutable public `glass-browser 0.3.14` API surface, while the
local dev no-verify packaging dry-run passed. No upload was attempted. Remote
CI remains pending because the branch is local-only.

The dependency-ordered `native-engine-116` implementation is complete and
recorded in `docs/plan/tasks/native-engine-116.md`. It extends inherited
`text-decoration-skip-spaces` with explicit `start`, `end`, and unordered
`start end` line-edge modes. The authoritative block-owned flow flush marks
the first and last text items and carries immutable provenance beside
display-list text commands, so raster replay skips only leading/trailing
fixed-cell ASCII-space intervals while the 115 `none|all` behavior remains
unchanged; nested inline temporary flows cannot claim a line edge. Implementation
is checkpointed at `a671a559` and `db7585f7`; local native/dev, strict, package,
fuzz, security, formatting, and static certification is recorded in the task
file. Remote CI remains pending because the branch is local-only.

The dependency-ordered `native-engine-117` implementation is complete at
`5e65aadf` and recorded in `docs/plan/tasks/native-engine-117.md`. It extends
the 116 replay classifier from literal ASCII spaces to Rust's bounded Unicode
`char::is_whitespace()` property for whitespace that survives the
literal/preformatted path. The existing `none|all|start|end|start end` values,
line-edge provenance, normal collapsing, fixed-cell geometry, and explicit
omission fallback remain unchanged. Focused, full-native, two-crate, package,
fuzz, security, and static local certification passed; exact evidence and
cleanup are recorded in the task.

The dependency-ordered `native-engine-118` implementation is complete at
`6f8e89fc` and recorded in `docs/plan/tasks/native-engine-118.md`. It accepts
only the explicit, case-insensitive `text-decoration-skip-spaces: initial`
keyword and maps it to the existing `StartAndEnd` computed value while
preserving the deliberate omitted-property `None` fallback. Other CSS-wide
keywords, layout/raster changes, new dependencies, default-feature changes,
and crate-boundary changes remain outside this slice. Focused, full-native,
two-crate, strict, package, fuzz, security, formatting, and static local
certification passed; exact evidence and cleanup are recorded in the task.
The dependency-ordered `native-engine-119` implementation is complete at
`1118bf2b` and recorded in `docs/plan/tasks/native-engine-119.md`. It accepts
explicit, case-insensitive `text-decoration-skip-spaces: inherit` through a
private declaration-only representation and resolves it against the existing
`NativeInheritedStyle` at computed-style construction. The resolved
`NativeTextDecorationSkipSpaces` enum, display-list, raster, layout, and
omitted fallback remain unchanged. Focused, full-native, two-crate, strict,
package, fuzz, security, formatting, and static local certification passed;
exact evidence and cleanup are recorded in the task.
The dependency-ordered `native-engine-120` implementation is complete at
`897bd648` and recorded in `docs/plan/tasks/native-engine-120.md`. It accepts
explicit, case-insensitive `text-decoration-skip-spaces: unset` through the
same private declaration-only representation and resolves it as inherited
parent state for this inherited property. The resolved paint enum, display-list,
raster, layout, and omitted fallback remain unchanged. Focused, full-native,
two-crate, strict, package, fuzz, security, formatting, and static local
certification passed; exact evidence and cleanup are recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-121` implementation is complete at
`f361415a` and recorded in `docs/plan/tasks/native-engine-121.md`. It accepts
explicit, case-insensitive `text-decoration-skip-spaces: revert` through a
distinct private declaration-only representation and resolves it at the
existing one-author-origin inherited fallback boundary. The resolved paint
enum, display-list, raster, layout, and omitted fallback remain unchanged;
`revert-layer`, cascade layers, multiple style origins, and general CSS-wide
keyword machinery remain outside this slice. Focused, full-native, two-crate,
strict, package, fuzz, security, formatting, and static local certification
passed; exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered `native-engine-124` implementation is complete at
`d6c8bc70` and recorded in `docs/plan/tasks/native-engine-124.md`. It reuses
bounded cascade layers and the private rollback representation for inherited
`text-decoration-style: revert-layer`, preserving finite
`Solid|Dashed|Dotted|Double|Wavy` paint replay and the existing
display-list/raster owner. Other CSS-wide keywords, `all`, multiple origins,
`!important` inversion, layer statements, and unsupported values remain
excluded. Exact local gate, documentation-audit, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered `native-engine-125` implementation is complete at
`271702ae` and recorded in `docs/plan/tasks/native-engine-125.md`. It reuses
bounded cascade layers and the private rollback representation for inherited
`text-decoration-thickness: revert-layer`, preserving finite `1px` through
`4px` decoration geometry and the existing display-list/raster owner. Parser,
cascade, display-list, command, and decoded-raster regressions passed for all
three line owners. Other CSS-wide keywords, `all`, multiple origins,
`!important` inversion, layer statements, and unsupported values remain
excluded. Exact local gate, issue-sync, and regenerable-output cleanup evidence
is recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-126` implementation is complete at
`3ffe86f8` and recorded in `docs/plan/tasks/native-engine-126.md`. It reuses
bounded cascade layers and the private rollback representation for inherited
`text-underline-offset: revert-layer`, preserving finite signed `-4px` through
`4px` underline translation, the existing display-list/raster owner, and the
unlayered/inline layer boundary. Parser, cascade, display-list, command, and
decoded-raster regressions passed; other CSS-wide keywords, `all`, multiple
origins, `!important` inversion, layer statements, and unsupported values
remain excluded. Exact local gate, issue-sync, and regenerable-output cleanup
evidence is recorded in the task. The dependency-ordered `native-engine-127`
implementation is complete at `50a36545` and recorded in
`docs/plan/tasks/native-engine-127.md`. It reuses bounded cascade layers and a
private rollback representation for local `text-decoration-color:
revert-layer`, preserving the existing `Option<NativeColor>` no-candidate
fallback and separate glyph/decoration paint owner. Parser, cascade,
display-list, command, and decoded-raster regressions passed; other CSS-wide
keywords, `currentColor`, multiple origins, `!important` inversion, layer
statements, and unsupported values remain excluded. Exact evidence,
issue-sync, and regenerable-output cleanup evidence is recorded in the task.
The dependency-ordered `native-engine-128` implementation is complete at
`15fc761c` and recorded in `docs/plan/tasks/native-engine-128.md`. It reuses
bounded cascade layers and a private rollback representation for
`text-decoration-line: revert-layer` and `text-decoration: revert-layer`,
preserving the shared inherited three-bit line-state owner,
declaration-order interaction, unlayered/inline bucket, and existing
display-list/command/raster consumers. Parser, cascade, inherited fallback,
and decoded-raster regressions passed; other CSS-wide keywords, multiple
origins, `!important` inversion, layer statements, and unsupported values
remain excluded. Exact evidence, issue-sync, and regenerable-output cleanup
evidence is recorded in the task. Remote CI remains pending because the
checkout is local-only.
The dependency-ordered `native-engine-129` implementation is complete in
`d26033af`, with its fixture assertion correction in `c32aeafe` and
diagnostic-classifier fix in `36a0f68`, and is recorded in
`docs/plan/tasks/native-engine-129.md`. It reuses the bounded cascade layer
registry for private `revert-layer` declarations on the inherited `text-align`,
`text-align-last`, and `text-justify` owners without changing the finite
alignment values or fixed-cell line/artifact path; valid rollback declarations
also remain absent from unsupported-value diagnostics.
The dependency-ordered `native-engine-130` implementation is complete in
`d7f4d7ca` and is recorded in `docs/plan/tasks/native-engine-130.md`. It reuses
the bounded cascade layer registry for private `white-space: revert-layer`
declarations while preserving the five finite whitespace modes,
inherited/root fallback, and the existing hard-break and fixed-cell wrapping
owners. Focused and complete affected-package local gates passed; the package
library gate used an explicit 32 MiB test-thread stack for one pre-existing
CLI stack-overflow test. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-131` implementation is complete in
`e35a13fd` and is recorded in `docs/plan/tasks/native-engine-131.md`. It reuses
the bounded cascade layer registry for private `line-height: revert-layer`
declarations while preserving the positive-pixel grammar, inherited/root
`Option<u32>` fallback, inline auto-height and explicit-height precedence, and
the existing flow/artifact owners. Focused and complete affected-package local
gates passed; the package library gate used an explicit 32 MiB test-thread
stack for one pre-existing CLI stack-overflow test. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered `native-engine-132` implementation is complete in
`4a46862f` and recorded in `docs/plan/tasks/native-engine-132.md`. It extends
the same private bounded layer resolver to inherited `direction: revert-layer`,
retaining the finite `ltr|rtl` value and the existing logical text, flex,
wrapping, hit-test, display-list, and raster owners. Focused, full-native,
affected-library, and strict affected-package local gates passed; exact test
and cleanup evidence is recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered `native-engine-133` implementation is complete in
`56944c83` and recorded in `docs/plan/tasks/native-engine-133.md`. It reuses the
private bounded layer resolver for non-inherited `flex-direction: revert-layer`,
keeping finite row/row-reverse/column/column-reverse computed values, the
local `row` fallback, finite `flex-flow` expansion, and all existing
flex/artifact owners. Focused, full-native, affected-library, and strict
affected-package local gates passed; exact test and cleanup evidence is
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-134` implementation is complete in
`f2e20121` and recorded in `docs/plan/tasks/native-engine-134.md`. It reuses the
private bounded layer resolver for the non-inherited `flex-wrap`,
`justify-content`, `align-items`, `align-self`, and `align-content` owners,
keeping their finite values, native fallbacks, finite `flex-flow`/`place-content`
expansion, and all existing flex/artifact consumers. Focused, full-native,
affected-library, and strict affected-package local gates passed; exact test
and cleanup evidence is recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered `native-engine-135` implementation is complete in
`74195032` and recorded in `docs/plan/tasks/native-engine-135.md`. It reuses
the private bounded layer resolver for the non-inherited `order`, `flex-grow`,
`flex-shrink`, and `flex-basis` owners, keeping finite values, native
fallbacks, finite `flex` expansion, stable visual order, grow/shrink
allocation, base-size selection, min/max constraints, and existing
layout/artifact consumers. Focused, full-native, affected-library, and
strict affected-package local gates passed; exact test and cleanup evidence
is recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-122` implementation is complete at
`1291fc2c`, with the strict-Clippy parser-context follow-up at `efb5bdfc`, and
is recorded in `docs/plan/tasks/native-engine-122.md`. It adds bounded
top-level named cascade layers and explicit `text-decoration-skip-spaces:
revert-layer` rollback with a private 15-layer priority bound, preserving the
unlayered/inline bucket and the existing finite public artifact contract.
Focused, full-native, two-crate, strict, package, fuzz, security, formatting,
and static local certification passed; exact evidence and cleanup are recorded
in the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered `native-engine-123` implementation is complete at
`af644112` and recorded in `docs/plan/tasks/native-engine-123.md`. It reuses
bounded cascade layers for inherited `text-decoration-skip-ink:revert-layer`,
with private candidate rollback and no public artifact/schema change.
Existing `Auto|None` glyph-intersection replay remains the owner; general
CSS-wide keyword machinery, multiple origins, layer statements, and
unsupported values remain excluded. Focused, full-native, two-crate, strict,
package, fuzz, security, formatting, and static local certification passed;
exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.

Issue [#40](https://github.com/wanazhar/glass/issues/40) is the authority. The
current delivery is a Phase 0/Phase 1 kernel plus bounded Phase 2 semantic
DOM/interaction slices and initial Phase 3 presentation slices, not an attempt
to implement a complete browser in one change.

## Baseline and constraints

The current checkout has exactly two installable crates. The native engine
must stay inside `glass-browser`, remain default-off, and add no dependency in
these slices. Chromium/CDP remains the production path; native selection is
explicit-only.

The normal browser-free platform matrix uses `--no-default-features`; a
dedicated Linux `Native engine core` job owns the explicit `native-engine`
feature checks, tests, and strict Clippy. The two jobs use separate cache keys
so experimental compilation does not become an unobserved default-build cost.

Baseline captured on the working machine before the change:

```text
command: /usr/bin/time -f 'elapsed_seconds=%e peak_rss_kb=%M' \
  cargo check -p glass-browser --lib --locked
result: exit 0
elapsed_seconds: 11.22
peak_rss_kb: 1,132,708
```

This is a local observation, not a cross-platform budget. The post-change
verification records default and native-feature checks separately.

Post-change warm checks after the updated target was built:

| Check | Elapsed | Peak RSS | Scope |
|---|---:|---:|---|
| `cargo check -p glass-browser --lib --locked` | 0.26 s | 81,004 KiB | default features |
| `cargo check -p glass-browser --features native-engine --locked` | 0.61 s | 81,056 KiB | native feature |

These are warm incremental checks on this Linux host, not clean-build or
cross-platform claims. The native feature remains heavier to compile than the
default path only because it adds the Phase 1/Phase 2 module set; it adds no
external dependency. A future performance task should record clean builds, a
real edit touching the native module, and target-directory growth separately.

## Module decomposition

| Module | Owns | Inputs | Outputs | Dependencies |
|---|---|---|---|---|
| `native_engine::config` | public startup configuration, hard limits, and bounded local URL policy | URLs, viewport, fixtures, limits | validated `NativeEngineConfig` and local URL helpers | `url`, typed native error |
| `native_engine::lifecycle` | lifecycle state | transitions | `New`, `Running`, `Closed` | none |
| `native_engine::scheduler` | logical clock and bounded ordered tasks | task kind, delay | deterministic task IDs/order | native limits |
| `native_engine::history` | current local history and per-entry root scroll state | committed URL/revision/scroll offset | bounded entries/current index | native limits + layout point |
| `native_engine::origin` | Phase 1 origin placeholder | loaded URL | opaque origin | none |
| `native_engine::resource_loader` | fixture/data/about resource boundary | validated URL | bounded local HTML resource | `url`, config fixtures |
| `native_engine::css` | bounded selector/rule parsing, display/visibility presentation, inherited color, positive-pixel line-height, pixel dimensions, physical solid/dashed/dotted borders, circular border radii, physical padding/margin edges, local opacity alpha, inherited `font-weight:normal|bold|400|700`, inherited `font-style:normal|italic`, inherited `word-break:normal|break-all`, inherited `vertical-align:baseline|top|middle|bottom`, and bounded non-inherited flex-row `justify-content:normal|flex-start|center|flex-end|space-between|space-around|space-evenly|stretch`, flex-item `order`, flex cross-axis `align-items`, `align-self:auto|flex-start|center|flex-end`, `flex-direction`, `flex-wrap`, `flex-flow`, `align-content:flex-start|center|flex-end|space-between|space-around|space-evenly|stretch|normal`, integer `flex-grow`, integer `flex-shrink`, `flex-basis:auto|Npx`, and `flex` shorthand | style text, inline style, native element attributes, ancestor styles | deterministic computed presentation values | native DOM element surface |
| `native_engine::layout` | viewport-bounded block/inline normal-flow geometry, bounded outer/content box model, side-specific border insets, rounded-box metadata, preflight inline line placement, inherited fixed line-height floors, direct-text fragments, whitespace-boundary flow, source-order paint entries, aligned line-item ranges with bounded vertical offsets, opacity group boundaries, root scroll projection, rounded point hit testing, bounded inherited word-break wrapping, bounded fixed-width flex-row free-space placement, stable visual flex-item order sorting, complete flex cross-axis alignment with explicit/auto line sizing and per-item `align-self` overrides, bounded physical flex wrapping and wrap-reverse line stacking, bounded cross-line alignment, `justify-content:normal|stretch|space-around|space-evenly` through the flex-start placement owner, explicit `align-content:normal` line distribution, bounded positive flex-grow allocation with max-width freeze/redistribution, base-width-weighted flex-shrink allocation with min-width freezing, explicit flex-basis base sizing, and flex shorthand/flow component reuse | DOM, computed presentation, viewport, scroll offset | document-space layout boxes/text fragments, paint order, scroll metadata, and deterministic hit target | native DOM + CSS presentation |
| `native_engine::paint` | revisioned clear/fill/text-fragment/physical-border display-list derivation, bounded rounded paint masks, source-order entries, opacity group markers, ancestor clips, and scroll metadata | current layout, bounded computed colors/text/borders/radii/opacity/font presentation, and overflow presentation | immutable document-space display-list commands | native DOM + layout |
| `native_engine::raster` | bounded logical RGBA surface replay for fills, text, rounded solid/dashed/dotted borders, nested opacity layers, PNG encoding, and viewport translation | immutable display-list commands and scroll offset | immutable software surface or bounded PNG bytes | native display list + existing `png` dependency |
| `native_engine::dom` | arena DOM, semantic projection, and bounded control/form mutation | HTML source, locators, and limits | generational nodes/document evidence/effects | native limits |
| `native_engine::interaction` | action/effect types and bounded effect records | semantic action and event kind | revisioned interaction metadata | native DOM IDs |
| `native_engine::engine` | sole mutable page-state coordinator | lifecycle/navigation/action requests | snapshots/context/history/effects | all engine modules |
| `browser::native_backend` | semantic adapter/profile | backend requests | typed backend responses/errors | engine + `browser_backend` |
| `browser::runtime` | explicit native session construction | `NativeEngineConfig`, runtime choice | initialized `BrowserRuntimeSession` | backend factory + dispatcher |
| `cli::runner` | feature-gated native one-shot dispatch | local command and semantic target | bounded CLI result or typed denial | runtime session + policy boundary |

The completed 094 extension makes the `css` direction state explicit as
`row|row-reverse|column|column-reverse`. The `layout` owner now consumes
column directions only for fixed-height, no-wrap flex containers, maps
vertical justification and `row-gap`, applies bounded vertical flexible
sizing, resolves horizontal item alignment, and sends the resulting complete
subtree ranges through the existing paint, raster, overflow, scroll, hit-test,
and capture consumers. Unsupported auto-height and wrapped-column contexts
retain the established fallback boundary.

The completed 085 shorthand is intentionally represented by the existing CSS
component fields and consumed by the existing layout owners: `align-content`
continues to distribute wrapped lines, while `justify-content` continues to
distribute items on each line. No standalone shorthand state or second
geometry representation is introduced.

## Integration enumeration

The first slice must prove these real call chains:

1. `NativeEngineConfig` creates a bounded `NativeResourceLoader` and a
   deterministic scheduler.
2. `NativeEngineBackend::new` creates one `NativeEngine` and validates its
   experimental `BackendProfile`.
3. `BackendFactory::native` registers the backend without adding it to
   automatic selection candidates.
4. `BackendFactory::start` permits the native candidate only when the request's
   preferred backend ID is `native-engine`.
5. `BrowserBackendDispatcher::initialize` reaches the engine lifecycle state.
6. `BrowserBackendDispatcher::navigate` reaches resource loading, DOM parsing,
   scheduler commit, history, and revision generation.
7. `BrowserBackendDispatcher::contexts` projects the sole engine context into
   the transport-neutral `BrowsingContext` type.
8. `BrowserBackendDispatcher::evidence` projects the bounded document snapshot
   and rejects screenshot-containing levels.
9. A failed resource or parse path returns before document/history/revision
   mutation.
10. Dispatcher action/effects calls reach the native mutation and revision
    owner; script, storage, prompts, and download calls fail through the
    profile's typed capability gate, while the explicit PNG capture path reads
    the renderer surface without mutation.

## Non-goals for this checkpoint

- network, filesystem navigation, redirects, HTTP semantics, or cookies;
- general CSS parsing/cascade, general raster painting, screenshot semantics,
  or fonts;
  the bounded 009 layout, 010 display-list, 011 software-surface, 013
  clipping, 014 border, and 015 PNG seeds do not imply general layout or
  rendering;
- JavaScript, event loops, timers, storage, workers, Web APIs, or downloads;
- MCP/TUI integration, external browser lifecycle, or platform windows;
- claiming standards compatibility, browser parity, or remote-content safety;
- adding a third crate or a complete-engine dependency.

## Phase 2 and initial Phase 3 slice decomposition

The Phase 2 work is deliberately split into dependency-ordered slices so that
semantic identity is established before mutation and parser state consume it:

| Task | Owns | Depends on | Does not claim |
|---|---|---|---|
| `native-engine-002` | supported role/name projection, bounded attributes, explicit semantic locators, revision-bound references | `native-engine-001` | CSS selectors, layout, hit testing, raw form values |
| `native-engine-003` | click/type/focus mutation for supported controls, checkbox/radio state, revisioned native effects, action/effects backend dispatch | `native-engine-002` | JavaScript, default navigation, coordinate input, full event loop |
| `native-engine-004` | deterministic single-select/option state and semantic option clicks | `native-engine-003` | keyboard navigation, multi-select, submission, network, layout hit testing |
| `native-engine-005` | bounded hidden-state projection, hidden-subtree text exclusion, and pre-mutation visibility gating | `native-engine-004` | CSS selectors/cascade, layout, hit testing, opacity, paint |
| `native-engine-006` | raw-text/RCDATA tokenizer state for script, style, title, and textarea content | `native-engine-005` | HTML5 insertion modes, foreign content, CSS, JavaScript execution |
| `native-engine-007` | bounded compound selectors and display/visibility cascade feeding text/actionability | `native-engine-006` | general CSS, inheritance, layout, hit testing, paint |
| `native-engine-008` | feature-gated `BrowserRuntime::Native`, explicit Rust session construction, local CLI dispatch, and fail-closed runtime validation | `native-engine-007` | remote endpoints, external lifecycle, script/evaluate, MCP/TUI, browser parity |
| `native-engine-009` | bounded integer-pixel normal-flow geometry, Rust layout inspection, deterministic point hit testing, and native `point=x,y` click resolution | `native-engine-008` | general CSS/layout, scrolling, paint, screenshots, stacking contexts, fractional units, browser parity |
| `native-engine-010` | bounded solid-color computed values and revisioned clear/fill/text display-list derivation | `native-engine-009` | rasterization, screenshots, fonts, images, borders, clipping, scrolling, stacking contexts, browser parity |
| `native-engine-011` | bounded logical RGBA software-surface replay with fixed ASCII glyphs and alpha compositing | `native-engine-010` | PNG/screenshots, font shaping, images, borders, clipping, scrolling, stacking contexts, browser parity |
| `native-engine-012` | bounded inherited `color` resolution through DOM ancestors feeding text runs | `native-engine-011` | general CSS inheritance/cascade, inherited layout, fonts, images, screenshots, browser parity |
| `native-engine-013` | bounded `overflow:hidden` ancestor clips on fill/text commands and software replay | `native-engine-012` | scrolling, visible overflow, stacking contexts, borders, transforms, screenshots, browser parity |
| `native-engine-014` | bounded uniform `border:Npx solid <color>` parsing, `BorderRect` display commands, and inside-the-box software replay | `native-engine-013` | padding, box sizing, individual sides, non-solid styles, scrolling, transforms, screenshots, browser parity |
| `native-engine-015` | bounded logical RGBA-to-PNG encoding and real native `CaptureFormat::Png` dispatch | `native-engine-014` | screenshot-containing evidence, JPEG/PDF, physical pixels, viewport/element/full-page modes, fonts, images, browser parity |
| `native-engine-016` | bounded uniform padding/margin, explicit box sizing, outer/content layout rectangles, and content-origin flow/paint | `native-engine-015` | side-specific/negative/percentage/auto values, margin collapsing, positioned/flex/grid layout, scrolling, browser parity |
| `native-engine-017` | bounded root vertical viewport scrolling, content-height/max-offset metadata, coordinate mapping, translated replay/capture, and revisioned scroll action | `native-engine-016` | horizontal/nested/smooth scrolling, scroll anchoring/snap, keyboard scrolling, general overflow/stacking layout, browser parity |
| `native-engine-018` | independently cascaded physical side-specific solid borders, side-aware box insets, display-list paint data, and clipped/scrolled replay | `native-engine-017` | non-solid styles, radius/images/gradients, logical sides/writing modes, corner joins, general CSS, browser parity |
| `native-engine-019` | bounded physical `solid`/`dashed`/`dotted` border styles, typed paint data, deterministic integer patterns, and clipped/scrolled replay | `native-engine-018` | other border styles, radius/images/gradients, standalone style properties, logical sides/writing modes, browser corner metrics, general CSS, browser parity |
| `native-engine-020` | bounded one-to-four-value physical `border-radius`, conservative corner normalization, rounded fill/border replay, and rounded point hit testing | `native-engine-019` | percentages, elliptical radii, corner longhands, rounded descendant clips, transforms, anti-aliasing, browser corner fidelity, general CSS |
| `native-engine-021` | bounded inline-box line placement with preflight width/margin checks and deterministic line-height flow | `native-engine-020` | font shaping/metrics, general word-aware text fragments, CSS whitespace modes, CSS word-breaking variants, baselines, bidi, floats, replaced elements, flex/grid, browser parity |
| `native-engine-022` | bounded positive-pixel `line-height` parsing/cascade, flow minimum line-height, and inline auto-height precedence | `native-engine-021` | `normal`, unitless/relative/percentage values, general inheritance, font metrics, baselines, vertical-align, browser parity |
| `native-engine-023` | bounded direct-text fragments at actual flow origins, collapsed bounded text width, and source-order display-list paint | `native-engine-022` | font metrics/shaping, CSS whitespace modes, general word-aware wrapping, baselines, bidi, cross-node whitespace joining, browser parity |
| `native-engine-024` | bounded word-aware direct-text wrapping with fixed-width fallback for over-wide words | `native-engine-023` | CSS `white-space`/`word-break`/`overflow-wrap`, hyphenation, font metrics/shaping, baselines, bidi, cross-node whitespace joining, browser parity |
| `native-engine-025` | bounded one-to-four-value physical `padding`/`margin` expansion, physical longhands, independent side cascade, and side-aware content/flow geometry | `native-engine-024` | negative/percentage/auto values, logical sides/writing modes, margin collapsing, min/max constraints, positioning, flex/grid, browser parity |
| `native-engine-026` | bounded source-whitespace boundaries across sibling direct text, `display:contents`, and supported inline flow items | `native-engine-025` | CSS `white-space` modes, preserved tabs/newlines, word spacing, Unicode line breaking, bidi, font metrics/shaping, anonymous inline boxes, and browser parity |
| `native-engine-027` | bounded rectangular `overflow:hidden` ancestor clips shared by paint, viewport rectangle projection, and point hit-testing | `native-engine-026` | visible overflow, axis-specific or nested scrolling, rounded descendant clips, stacking contexts, transforms, and browser parity |
| `native-engine-028` | bounded revisioned diagnostics for unsupported selectors, properties, values, and malformed CSS in stylesheet and inline-style sources | `native-engine-027` | general CSS parsing/conformance, raw source echo, stable transport diagnostics, and browser parity |
| `native-engine-029` | complete logical-pixel golden for the bounded native surface and decoded PNG capture path | `native-engine-028` | screenshot compatibility, physical pixels, font/image fidelity, anti-aliasing, color management, stable evidence schema, and browser parity |
| `native-engine-030` | bounded descendant selector chains across the owned DOM ancestry with summed specificity and existing cascade precedence | `native-engine-029` | direct-child/sibling/pseudo/functional/namespace selectors, general CSS conformance, style caching, and browser parity |
| `native-engine-031` | bounded non-scrolling `overflow: clip` through the existing rectangular clip, viewport projection, and point hit-testing path | `native-engine-030` | `visible`/`auto`/`scroll`, axis-specific overflow, nested scrolling, scrollbars, rounded descendant clips, general CSS conformance, and browser parity |
| `native-engine-032` | bounded visible `<br>` hard line breaks through the existing integer inline-flow cursor and fixed line-height floor | `native-engine-031` | `<wbr>`, preserved source newlines, CSS `white-space`, font metrics, Unicode line breaking, bidi, general inline formatting, and browser parity |
| `native-engine-033` | bounded inherited `white-space: pre-line` source newline breaks through the existing hard-break cursor while collapsing other whitespace | `native-engine-032` | `pre`/`pre-wrap`/`break-spaces`/`nowrap`, preserved tabs or arbitrary whitespace, text alignment, font metrics, Unicode line breaking, bidi, and browser parity |
| `native-engine-034` | bounded inherited `white-space: pre` with literal fixed-cell source whitespace, LF/CR/CRLF hard breaks, and no soft wrapping | `native-engine-033` | `pre-wrap`/`break-spaces`/`nowrap`, browser tab stops, wide-line reflow or horizontal scrolling, text alignment, word spacing, Unicode line breaking, bidi, font metrics, and browser parity |
| `native-engine-035` | bounded inherited `white-space: pre-wrap` with literal fixed-cell source whitespace, LF/CR/CRLF hard breaks, and deterministic fixed-cell soft wrapping | `native-engine-034` | `break-spaces`/`nowrap`, browser line-breaking opportunities, tab stops, wide-line scrolling, text alignment/justification, word spacing, Unicode line breaking, bidi, font metrics/shaping, and browser parity |
| `native-engine-036` | bounded standard padded base64 `data:text/html` loading through the existing local resource and dispatcher path with pre-decode and decoded-body bounds | `native-engine-035` | URL-safe or whitespace-tolerant base64, percent-encoded payloads, non-HTML media types, alternate charsets, subresources, network/filesystem loading, cancellation, and browser data-URL parity |
| `native-engine-037` | bounded raw-fragment same-document navigation plus explicit Rust back/forward traversal for local resources, preserving document state and parse-before-commit failure atomicity | `native-engine-036` | anchor scrolling, document snapshots for mutable history state, redirects, HTTP/network history, credentials/cookies, transport-level history operations, and browser navigation parity |
| `native-engine-038` | bounded semantic local anchor activation through the existing click and navigation path for fragment-only and absolute local hrefs, with empty-href click behavior and parse-before-commit failure atomicity | `native-engine-037` | relative URL resolution, remote/network navigation, downloads, target contexts, event propagation/default-action ordering, redirects, and browser navigation parity |
| `native-engine-039` | exact visible local fragment `id` target scrolling plus bounded per-history-entry root-scroll save/restore across same- and different-resource traversal | `native-engine-038` | percent-decoded fragment matching, `name`/text fragments, duplicate-id recovery, smooth/nested/horizontal/keyboard/snap scrolling, sticky layout, mutable DOM snapshots, and browser scrolling parity |
| `native-engine-040` | bounded same-host fixture-relative link resolution through the existing semantic navigation owner, with deterministic URL normalization and existing fragment/history behavior | `native-engine-039` | relative resolution from opaque data/about resources, host-changing references, remote/network navigation, credentials, redirects, downloads, and browser URL-parsing parity |
| `native-engine-041` | bounded UTF-8 percent-decoding of local fragment identifiers before exact visible `id` lookup and existing root-scroll/history restoration | `native-engine-040` | `name`/text fragments, duplicate-id recovery beyond decoded equality, malformed-fragment error policy, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser URL-parsing parity |
| `native-engine-042` | bounded exact legacy `<a name>` fragment-target fallback after decoded `id` lookup, with ID precedence and duplicate-safe root-scroll/history behavior | `native-engine-041` | arbitrary `name` attributes, text fragments, duplicate-id recovery, malformed-fragment error policy, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser URL/scrolling parity |
| `native-engine-043` | bounded `#:~:text=start[,end]` matching against the first visible non-truncated text run after per-term UTF-8 decoding | `native-engine-042` | prefix/suffix syntax, multiple directives, cross-run ranges, highlights, duplicate-text disambiguation, Unicode normalization, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser text-fragment parity |
| `native-engine-044` | bounded exact prefix/suffix text-fragment affixes around a same-run `start[,end]` match | `native-engine-043` | cross-run ranges, multiple directives, highlights, Unicode normalization, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser text-fragment parity |
| `native-engine-045` | bounded root horizontal scrolling from measured document overflow width with independent x/y clamping and shared projection | `native-engine-044` | nested scrolling, scrollbars, smooth/keyboard/snap scrolling, axis-specific CSS overflow, scroll anchoring, and browser parity |
| `native-engine-046` | bounded inherited `white-space: nowrap` with collapsed fixed-cell one-line flow and measured root overflow | `native-engine-045` | preserved whitespace modes beyond the existing subset, nested scrolling, scrollbars, Unicode line breaking, font metrics, and browser parity |
| `native-engine-047` | bounded inherited positive-pixel `line-height` through the DOM style walk and existing flow minimum/inline auto-height owners | `native-engine-046` | font-relative, unitless, percentage, CSS-wide keyword, baseline, `vertical-align`, general inheritance, and browser line metrics |
| `native-engine-048` | bounded clip-aware root overflow measurement from the existing document-space overflow intersection | `native-engine-047` | browser overflow propagation, nested scrolling, scrollbars, axis-specific overflow, visible overflow parity, and font metrics |
| `native-engine-049` | bounded independent `overflow-x`/`overflow-y` `hidden`/`clip` rectangles with shorthand/longhand cascade and shared consumers | `native-engine-048` | nested scrolling, scrollbars, mixed visible/auto/scroll used values, rounded descendant clips, and browser overflow parity |
| `native-engine-050` | bounded physical `min-width`/`max-width`/`min-height`/`max-height` constraints through the existing content-box or border-box owner | `native-engine-049` | negative/percentage/auto values, intrinsic sizing, aspect ratio, margin collapsing, positioning, flex/grid, and browser sizing parity |
| `native-engine-051` | bounded local CSS opacity alpha with display-list subtree markers and inside-out transparent-layer software compositing | `native-engine-050` | stacking contexts, transforms, filters, blend modes, animation, compositor parity, and unbounded layer allocation |
| `native-engine-052` | bounded inherited physical `text-align:left|center|right` with complete fixed-cell line-item offsets across text and supported inline boxes | `native-engine-051` | `justify`, logical `start`/`end`, direction/writing modes, vertical alignment, bidi, font metrics, and browser inline-formatting parity |
| `native-engine-053` | bounded functional `rgba(R, G, B, A)` alpha colors for background, border, and text through the existing fixed-point/source-over path | `native-engine-052` | CSS Color 4 syntax, color spaces, interpolation, wide gamut, and color-management parity |
| `native-engine-054` | bounded inherited fixed-cell `text-decoration:none|underline` carried through immutable text commands and clipped software replay | `native-engine-053` | font metrics, decoration propagation, other decoration styles, and browser text-paint parity |
| `native-engine-055` | bounded inherited ASCII `text-transform:none|uppercase|lowercase` applied during fixed-cell text layout and consumed by wrapping, text fragments, paint, and overflow measurement | `native-engine-054` | Unicode case mapping/expansion, locale behavior, `capitalize`/other transforms, font shaping/metrics, and browser text-rendering parity |
| `native-engine-056` | bounded non-negative fixed-pixel `text-indent` applied to the first line of block containers, with one-cell clamping and shared flow consumers | `native-engine-055` | negative/hanging indentation, each-line/hanging keywords, percentages and font-relative units, bidi/logical writing modes, inline-formatting parity, and browser CSS conformance |
| `native-engine-057` | bounded inherited non-negative fixed-pixel `word-spacing` applied to rendered ASCII spaces across collapsed and supported preformatted flow | `native-engine-056` | negative spacing, `letter-spacing`, relative/percentage units, Unicode whitespace and word-boundary policy, browser tab stops, font metrics, bidi, and browser CSS parity |
| `native-engine-058` | bounded inherited non-negative fixed-pixel `letter-spacing` applied after every rendered fixed-cell character in each emitted fragment, composed with word spacing | `native-engine-057` | negative/relative/percentage values, `normal`, pair-boundary and cross-fragment semantics, Unicode shaping/metrics, grapheme clusters, bidi, and browser CSS parity |
| `native-engine-059` | bounded inherited `font-weight: normal|bold|400|700` with deterministic fixed-cell normal/bold raster replay | `native-engine-058` | real font selection/loading/metrics, numeric interpolation, variable fonts, synthetic-bold policy, Unicode shaping, anti-aliasing, and browser text-rendering parity |
| `native-engine-060` | bounded inherited `font-style:normal|italic` with deterministic fixed-cell normal/italic raster replay composed with bold | `native-engine-059` | oblique angles, font selection/loading/metrics, real italic faces, variable fonts, Unicode shaping, anti-aliasing, and browser text-rendering parity |
| `native-engine-061` | bounded inherited `word-break:normal|break-all` with deterministic fixed-cell word-aware or character-boundary wrapping in collapsed flow | `native-engine-060` | `keep-all`, `break-word`, `overflow-wrap`, Unicode/CJK line breaking, grapheme policy, hyphenation, bidi, writing modes, font metrics, and browser CSS parity |
| `native-engine-062` | bounded local `text-overflow:clip|ellipsis` for eligible single-line clipped direct text with fixed-cell ASCII marker presentation | `native-engine-061` | multi-line ellipsis/line-clamp, nested inline formatting, multiple text nodes, visible overflow, vertical/RTL behavior, Unicode ellipsis, grapheme policy, font metrics, shaping, and browser CSS parity |
| `native-engine-063` | bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for fixed-cell inline/inline-block line items | `native-engine-062` | font baselines/metrics, `text-top`/`text-bottom`, sub/super, lengths, percentages, bidi, writing modes, ruby, table-cell/replaced-element alignment, and browser CSS parity |
| `native-engine-064` | bounded block-level `display:flex` single-row placement for eligible direct element children with fixed item widths and existing box-model consumers | `native-engine-063` | flex grow/shrink/basis/order, columns/reverse directions, wrapping, gaps, justify/align distribution, anonymous text items, `display:contents` flattening, and browser Flexbox parity |
| `native-engine-065` | bounded one-value non-negative fixed-pixel `gap` between visible direct element items in an eligible fixed-width flex row | `native-engine-064` | multi-value/percentage gap grammar, row/column gap, free-space distribution, wrapping, direction, cross-axis alignment, and browser Flexbox parity |
| `native-engine-066` | bounded `justify-content:flex-start|center|flex-end|space-between` free-space placement for eligible fixed-width flex rows with deterministic integer rounding | `native-engine-065` | flex growth/shrink/basis, wrapping, reverse/column direction, `space-around`/`space-evenly`, cross-axis alignment, anonymous items, and browser Flexbox parity |
| `native-engine-067` | bounded non-inherited signed `order` values in `-1024..=1024` for eligible fixed-width flex rows, sorted by order with stable source-order ties before gap/justification | `native-engine-066` | flex growth/shrink/basis, wrapping, reverse/column direction, cross-axis alignment, anonymous items, semantic/accessibility/keyboard reordering, and browser Flexbox parity |
| `native-engine-068` | bounded non-inherited `align-items:flex-start|center|flex-end` for eligible fixed-width single-row flex rows, using explicit or auto line cross size and shared subtree offsets | `native-engine-067` | stretch, baseline/normal/logical alignment, align-content, auto margins, wrapping, reverse/column direction, multiple lines, and browser Flexbox parity |
| `native-engine-069` | bounded non-inherited `flex-direction:row|row-reverse` for eligible fixed-width single-row flex rows, with margin-aware physical reverse placement and shared consumers | `native-engine-068` | column directions, wrapping, multiple lines, logical direction/RTL, auto margins, flex growth/shrink/basis, and browser Flexbox parity |
| `native-engine-070` | bounded non-inherited `flex-wrap:nowrap|wrap` for eligible fixed-width flex rows, with deterministic line formation and per-line existing consumers | `native-engine-069` | `wrap-reverse`, `flex-flow`, row/column gaps, multi-value/percentage gap, `align-content`, `place-content`, flex growth/shrink/basis, auto margins, logical direction/RTL, and browser Flexbox parity |
| `native-engine-071` | bounded non-inherited `align-content:flex-start|center|flex-end|space-between` for wrapped fixed-width flex rows, with explicit-height positive free-space distribution and complete line artifact translation | `native-engine-070` | `stretch`, `space-around`, `space-evenly`, `place-content`, cross-axis gaps, flex sizing, auto margins, column directions, `wrap-reverse`, logical direction/RTL, and browser Flexbox parity |
| `native-engine-072` | bounded non-inherited `align-content:space-around` for wrapped fixed-width flex rows, with saturating integer slot-center offsets and complete line artifact translation | `native-engine-071` | `space-evenly`, `stretch`, `place-content`, cross-axis gaps, flex sizing, auto margins, column directions, `wrap-reverse`, logical direction/RTL, and browser Flexbox parity |
| `native-engine-073` | bounded non-inherited `align-content:space-evenly` for wrapped fixed-width flex rows, with equal leading/inter-line/trailing integer slots and complete line artifact translation | `native-engine-072` | `stretch`, `place-content`, cross-axis gaps, flex sizing, auto margins, column directions, `wrap-reverse`, logical direction/RTL, and browser Flexbox parity |
| `native-engine-074` | bounded non-inherited `flex-wrap:wrap-reverse` for eligible fixed-width flex rows, preserving source-order line formation while reversing physical cross-axis stacking through signed complete-artifact translation | `native-engine-073` | `flex-flow`, cross-axis gaps, flex sizing, auto margins, `stretch`, `place-content`, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity |
| `native-engine-075` | bounded explicit non-inherited `align-content:stretch` for wrapped fixed-width flex rows, expanding line heights by deterministic integer shares and preserving normal/wrap-reverse artifact consumers | `native-engine-074` | implicit `normal`, `place-content`, cross-axis gaps, flex sizing, auto margins, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity |
| `native-engine-076` | bounded explicit non-inherited `align-content:normal` aliasing the completed stretch line-box owner for wrapped fixed-width flex rows while preserving the omitted-value fallback | `native-engine-075` | implicit initial-value change, `place-content`, cross-axis gaps, flex sizing, auto margins, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity |
| `native-engine-077` | bounded explicit non-inherited `row-gap` between adjacent formed lines in eligible wrapped fixed-width flex rows, included once before `align-content` free-space distribution | `native-engine-076` | `column-gap`, multi-value or percentage gap, implicit `gap` expansion, flex sizing, auto margins, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity |

| `native-engine-078` | bounded flex gap family with one- and two-value integer-pixel `gap`, explicit row/column longhands, declaration-order-aware cascade, and shared item/line geometry | `native-engine-077` | percentage/fractional lengths, negative values, column-direction flex, grid, flex sizing, auto margins, logical direction/RTL, and browser Flexbox parity |
| `native-engine-079` | bounded non-inherited integer `flex-grow` weights for eligible fixed-width row-flex items, with deterministic prefix-floor allocation and max-width freeze/redistribution | `native-engine-078` | flex shrink/basis/shorthand, fractional factors, base-size reflow, auto margins, columns, percentage/intrinsic sizing, and browser Flexbox parity |
| `native-engine-080` | bounded non-inherited integer `flex-shrink` weights for eligible fixed-width row-flex items, weighted by original base width with min-width freeze/redistribution | `native-engine-079` | flex basis/shorthand, fractional factors, base-size reflow, auto margins, columns, percentage/intrinsic sizing, and browser Flexbox parity |
| `native-engine-081` | bounded non-inherited `flex-basis:auto|Npx` for eligible row-flex items, with explicit bases overriding width before the shared grow/shrink and line-formation owners | `native-engine-080` | flex shorthand, percentages, fractional lengths, `calc()`, `content`, intrinsic sizing changes, auto margins, columns, and browser Flexbox parity |
| `native-engine-082` | bounded `flex` shorthand expansion into non-inherited grow, shrink, and basis components with declaration-order-aware longhand overrides | `native-engine-081` | CSS-wide reset keywords, percentage/fractional bases, fractional factors, ambiguous or unsupported token forms, auto margins, columns, and browser Flexbox parity |
| `native-engine-083` | bounded `flex-flow` shorthand expansion into non-inherited direction and wrap components with omitted-component reset and declaration-order-aware longhand overrides | `native-engine-082` | columns, column-reverse, logical direction/RTL, duplicate tokens, CSS-wide reset keywords, ambiguous forms, and browser Flexbox parity |
| `native-engine-084` | bounded non-inherited `align-self:auto|flex-start|center|flex-end` item override resolved through the existing flex line cross-axis and complete-artifact translation owner | `native-engine-083` | stretch, baseline metrics, normal, logical start/end, safe/unsafe alignment, auto margins, column directions, fractional/intrinsic sizing, and browser Flexbox parity |
| `native-engine-085` | bounded `place-content` shorthand expansion into the existing `align-content` and `justify-content` components with one-token shared values and explicit two-token axis order | `native-engine-084` | full CSS shorthand grammar, unsupported justify values, logical direction/writing modes, safe/unsafe alignment, CSS-wide reset semantics, grid, fractional/intrinsic sizing, and browser Flexbox parity |
| `native-engine-086` | bounded non-inherited `align-self:stretch` for eligible direct flex items, filling auto-height items from the existing line cross size while preserving explicit heights | `native-engine-085` | `align-items:stretch`, auto margins, baseline/normal/logical alignment, column directions, fractional/intrinsic sizing, and browser Flexbox parity |
| `native-engine-087` | bounded explicit parent `align-items:stretch` resolving `align-self:auto` through the completed used-size path while preserving explicit child overrides and heights | `native-engine-086` | auto margins, baseline/normal/logical alignment, column directions, fractional/percentage/intrinsic sizing, changed omitted-value defaults, and browser Flexbox parity |
| `native-engine-088` | bounded explicit parent `align-items:normal` resolving `align-self:auto` as the existing stretch path in row/row-reverse flex while retaining a distinct computed keyword | `native-engine-087` | `align-self:normal`, auto margins, baseline/logical alignment, block/grid/absolute layout modes, column directions, fractional/percentage/intrinsic sizing, changed omitted-value defaults, and browser Flexbox parity |
| `native-engine-089` | bounded explicit non-inherited `align-self:normal` resolving as the existing stretch path for eligible row/row-reverse flex items while retaining a distinct computed keyword | `native-engine-088` | auto margins, baseline/logical alignment, block/grid/absolute layout modes, column directions, fractional/percentage/intrinsic sizing, changed omitted-value defaults, and browser Flexbox parity |
| `native-engine-090` | bounded explicit non-inherited `justify-content:space-around` using deterministic cumulative integer main-axis offsets for eligible row/row-reverse flex lines | `native-engine-089` | auto margins, negative-free-space fallback, column directions, logical direction/RTL, percentage/fractional/intrinsic sizing, grid, and browser Flexbox parity |
| `native-engine-091` | bounded explicit non-inherited `justify-content:space-evenly` using deterministic cumulative integer main-axis offsets for eligible row/row-reverse flex lines | `native-engine-090` | auto margins, negative-free-space fallback, column directions, logical direction/RTL, percentage/fractional/intrinsic sizing, grid, and browser Flexbox parity |
| `native-engine-092` | bounded explicit non-inherited `justify-content:normal` retaining a distinct computed keyword while reusing the flex-start used-placement owner for eligible row/row-reverse flex lines | `native-engine-091` | auto margins, negative-free-space fallback, column directions, logical direction/RTL, percentage/fractional/intrinsic sizing, grid, and browser Flexbox parity |
| `native-engine-093` | bounded explicit non-inherited `justify-content:stretch` retaining a distinct computed keyword while reusing the flex-start used-placement owner for eligible row/row-reverse flex lines | `native-engine-092` | auto margins, negative-free-space fallback, column directions, logical direction/RTL, percentage/fractional/intrinsic sizing, grid, and browser Flexbox parity |
| `native-engine-094` | bounded explicit `flex-direction:column|column-reverse` for fixed-height no-wrap flex containers, reusing vertical main-axis justification, row-gap, flexible lengths, cross-axis alignment, reverse placement, and complete artifacts | `native-engine-093` | auto-height columns, wrapping, column-gap line distribution, auto margins, logical direction/writing modes, percentage/fractional/intrinsic sizing, baseline alignment, grid, and browser Flexbox parity |
| `native-engine-095` | bounded `flex-wrap:wrap` for fixed-height column/column-reverse flex containers, with vertical line formation, column-gap, per-line flexible sizing/justification, cross-axis alignment, align-content, and complete artifacts | `native-engine-094` | wrap-reverse, auto-height columns, percentage/fractional/intrinsic main sizes, auto margins, logical direction/writing modes, baseline alignment, grid, and browser Flexbox parity |
| `native-engine-096` | bounded `flex-wrap:wrap-reverse` for fixed-height column/column-reverse flex containers, reflecting horizontal line boxes and cross-axis item alignment while preserving 095 geometry, gaps, align-content, reverse main placement, and complete artifacts | `native-engine-095` | auto-height columns, percentage/fractional/intrinsic main sizes, auto margins, logical direction/writing modes, baseline alignment, grid, and browser Flexbox parity |
| `native-engine-097` | bounded auto main/cross margins for eligible no-wrap row/row-reverse and fixed-height column/column-reverse flex containers, resolved before justify and align distribution with complete artifacts | `native-engine-096` | wrapped lines, auto-height columns, percentage/fractional/intrinsic sizing, normal-flow auto margins, logical direction/writing modes, baseline alignment, grid, and browser Flexbox parity |
| `native-engine-098` | bounded line-local auto main/cross margins for eligible wrapped row/row-reverse and fixed-height column/column-reverse flex containers, resolved after final line sizing and before per-line justify/align distribution with complete artifacts | `native-engine-097` | auto-height columns, new intrinsic/percentage sizing, fractional lengths, normal-flow auto margins, logical direction/writing modes, baseline alignment, grid, and browser Flexbox parity |
| `native-engine-099` | bounded inherited direction ltr/rtl for eligible Flexbox owners, mapping row main-start and column horizontal cross-start through the existing reverse/wrap-reverse and artifact consumers | `native-engine-098` | non-flex bidi/text reordering, `text-align:start|end`, logical properties, vertical writing modes, grid, and browser-wide directionality |
| `native-engine-100` | bounded inherited `text-align:start/end` for fixed-cell inline flow, resolving logical line alignment through inherited direction while preserving physical alignment and source order | `native-engine-099` | Unicode bidi/shaping, mixed bidi runs, justify/match-parent/justify-all, logical properties, vertical writing modes, grid, floats, and browser-wide text conformance |
| `native-engine-101` | bounded inherited `text-align:justify` for eligible collapsed fixed-cell soft-wrapped lines, with deterministic per-space integer expansion through shared text/artifact consumers | `native-engine-100` | final-line/hard-break justification, preformatted and break-all flow, Unicode bidi/shaping, language-specific line breaking, match-parent/justify-all, logical properties, vertical writing modes, and browser-wide text conformance |
| `native-engine-102` | bounded inherited `text-align-last:auto/left/center/right/start/end` for the final non-empty line of eligible fixed-cell blocks, reusing the shared line-flush and artifact consumers | `native-engine-101` | final-line justification, forced-break and intermediate-boundary semantics, Unicode bidi/shaping, text-align match-parent/justify-all, logical properties, vertical writing modes, and browser-wide text conformance |
| `native-engine-103` | explicit inherited `text-align-last:justify` for the final non-empty line of eligible fixed-cell blocks, reusing 101's deterministic separator spacing through 102's final-line flush | `native-engine-102` | inter-character justification, preformatted/break-all/truncated final-line spacing, forced-break ownership, Unicode bidi/shaping, language-specific line breaking, text-justify, logical properties, vertical writing modes, and browser-wide text conformance |
| `native-engine-104` | bounded inherited `text-justify:auto|none|inter-word` control over the existing separator-spacing owner for soft-wrap and explicit final-line justification | `native-engine-103` | inter-character spacing, language-specific line breaking, Unicode bidi/shaping, glyph shaping/metrics, logical properties, vertical writing, fractional metrics, and browser-wide text conformance |
| `native-engine-105` | bounded inherited `text-decoration:none|underline|overline|line-through` single-line state through immutable text commands and fixed-pixel software replay | `native-engine-104` | decoration combinations, colors, thickness, style, offsets, font metrics, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-106` | bounded inherited multi-token `text-decoration` shorthand combinations for underline/overline/line-through through one immutable three-bit text command state and fixed-pixel replay | `native-engine-105` | `text-decoration-line` longhand, duplicate/none combinations, colors, thickness, style, offsets, font metrics, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-107` | bounded local `text-decoration-color` using the existing `NativeColor` grammar, carried separately from glyph color through one immutable text command and fixed-pixel replay | `native-engine-106` | decoration-origin propagation, `currentColor` syntax, shorthand color components, style/thickness/offset, color spaces, animations, font metrics, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-108` | bounded `text-decoration-line` longhand alias for the existing underline/overline/line-through bitset with declaration-order-aware shorthand interaction and shared artifact consumers | `native-engine-107` | full CSS longhand inheritance and decoration propagation, style/thickness/offset, shorthand color components, font metrics, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-109` | bounded inherited `text-decoration-style:solid|dashed|dotted` using the existing integer border-pattern helper through one immutable text command and fixed-pixel replay | `native-engine-108` | double/wavy styles, thickness, offsets, decoration-origin propagation, fragment continuity, font metrics, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-110` | bounded inherited `text-decoration-thickness:1px|2px|3px|4px` as a positive-y fixed-pixel band through one immutable text command and the existing integer style-pattern helper | `native-engine-109` | arbitrary/font-derived/fractional values, zero/negative/percentage/auto/from-font syntax, centering, offsets, baseline metrics, fragment continuity, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-111` | bounded inherited signed `text-underline-offset:-4px..=4px` for underline-only y translation through the immutable text command, preserving the 110 thickness/style raster owner | `native-engine-110` | auto/percentage/fractional/font-derived values, overline/line-through offsets, decoration-origin propagation, centering, baseline metrics, fragment continuity, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-112` | bounded inherited `text-decoration-style:double` through a dedicated text-decoration style type, painting two solid thickness-preserving bands with one separator pixel through the immutable text command | `native-engine-111` | wavy styles, font-metric centering, decoration-origin propagation, fragment continuity, layout/geometry changes, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-113` | bounded inherited `text-decoration-style:wavy` through the dedicated text-decoration style type, painting a continuous eight-pixel fixed-cell phase `[0,1,2,1,0,-1,-2,-1]` with resolved thickness through the immutable text command | `native-engine-112` | CSS metric centering, decoration-origin propagation, fragment continuity, antialiasing, layout/geometry changes, shaping, bidi, vertical writing, and browser-wide text conformance |
| `native-engine-114` | completed bounded inherited `text-decoration-skip-ink:auto|none` through a dedicated value in the immutable text command, suppressing matching same-run glyph intersections for underline and overline replay while leaving line-through unchanged | `native-engine-113` | `all`, font metrics, shaping, bidi, vertical writing, decoration-origin propagation, cross-fragment continuity, antialiasing, layout/geometry changes, and browser-wide text conformance |
| `native-engine-115` | completed bounded inherited `text-decoration-skip-spaces:none|all` through a dedicated value in the immutable text command, suppressing decoration pixels over same-run ASCII-space intervals including word/letter/justification spacing for underline, overline, and line-through | `native-engine-114` | `start`/`end`, Unicode whitespace, line-boundary semantics, font metrics, shaping, bidi, vertical writing, decoration-origin propagation, cross-fragment continuity, antialiasing, layout/geometry changes, and browser-wide text conformance |
| `native-engine-116` | completed bounded inherited `text-decoration-skip-spaces:start|end|start end` through block-owned flow-flushed line-edge provenance beside immutable text commands, suppressing only leading/trailing ASCII-space intervals | `native-engine-115` | Unicode whitespace, initial-value changes, atomic-inline and ancestor propagation, cross-fragment continuity, font metrics, shaping, bidi, vertical writing, antialiasing, layout/geometry changes, and browser-wide text conformance |
| `native-engine-117` | completed bounded Unicode `char::is_whitespace()` classification for `text-decoration-skip-spaces` replay across literal/preformatted fixed-cell runs, reusing 116 line-edge provenance and preserving ASCII spacing arithmetic | `native-engine-116` | CSS whitespace-mode conformance, Unicode line breaking, tab stops, initial-value changes, atomic-inline and ancestor propagation, cross-fragment continuity, font metrics, shaping, bidi, vertical writing, antialiasing, layout/geometry changes, and browser-wide text conformance |
| `native-engine-118` | completed explicit case-insensitive `text-decoration-skip-spaces:initial` keyword mapped to the existing `start end` computed value while preserving the deliberate omitted-property `none` fallback | `native-engine-117` | general CSS-wide keyword machinery, `inherit`/`unset`/`revert` semantics, omitted-value initial-default changes, atomic-inline and ancestor propagation, cross-fragment continuity, layout/geometry changes, and browser-wide text conformance |
| `native-engine-119` | completed explicit case-insensitive `text-decoration-skip-spaces:inherit` through a private declaration-only value resolved at the existing parent-style boundary, keeping the public paint enum finite | `native-engine-118` | `unset`/`revert`/`revert-layer`, general CSS-wide keyword machinery, changed omitted-value behavior, atomic-inline and ancestor propagation beyond the existing DOM walk, cross-fragment continuity, layout/geometry changes, and browser-wide text conformance |
| `native-engine-120` | completed explicit case-insensitive `text-decoration-skip-spaces:unset` resolved as inherited parent state through the existing private declaration-only boundary, keeping the public paint enum finite | `native-engine-119` | `revert`/`revert-layer`, general CSS-wide keyword machinery, changed omitted-value behavior, atomic-inline and ancestor propagation beyond the existing DOM walk, cross-fragment continuity, layout/geometry changes, and browser-wide text conformance |
| `native-engine-121` | completed explicit case-insensitive `text-decoration-skip-spaces:revert` resolved at the current one-author-origin inherited fallback boundary through a distinct private declaration-only state, keeping the public paint enum finite | `native-engine-120` | `revert-layer`, cascade layers, multiple style origins, general CSS-wide keyword machinery, changed omitted-value behavior, atomic-inline and ancestor propagation beyond the existing DOM walk, cross-fragment continuity, layout/geometry changes, and browser-wide text conformance |
| `native-engine-122` | completed bounded top-level named cascade layers with private first-appearance priority and `text-decoration-skip-spaces:revert-layer` rollback through lower candidates and the existing inherited fallback | `native-engine-121` | layer statements, anonymous/comma/nested layers, multiple origins, `!important` inversion, general CSS-wide keyword machinery, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-123` | completed reuse of bounded named-layer priority and private rollback for inherited `text-decoration-skip-ink:revert-layer`, preserving finite `Auto|None` paint replay | `native-engine-122` | other CSS-wide keywords, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-124` | completed reuse of bounded named-layer priority and private rollback for inherited `text-decoration-style:revert-layer`, preserving finite `Solid|Dashed|Dotted|Double|Wavy` paint replay | `native-engine-123` | other CSS-wide keywords, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-125` | completed reuse of bounded named-layer priority and private rollback for inherited `text-decoration-thickness:revert-layer`, preserving finite `1px` through `4px` decoration geometry | `native-engine-124` | other CSS-wide keywords, `auto`/`from-font`/percentages, lengths outside the bounded range, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-126` | completed reuse of bounded named-layer priority and private rollback for inherited `text-underline-offset:revert-layer`, preserving finite signed `-4px` through `4px` underline translation and stable overline/line-through origins | `native-engine-125` | other CSS-wide keywords, `auto`/percentages/fractional/font-derived values, dimensions outside the bounded range, overline/line-through offsets, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-127` | completed reuse of bounded named-layer priority and private rollback for local `text-decoration-color:revert-layer`, preserving the existing `Option<NativeColor>` fallback and separate glyph/decoration paint owner | `native-engine-126` | other CSS-wide keywords, `currentColor`, gradients, system colors, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-128` | completed reuse of bounded named-layer priority and private rollback for `text-decoration-line`/`text-decoration:revert-layer`, preserving the shared inherited three-bit line-state owner, declaration-order interaction, and existing display/raster geometry | `native-engine-127` | other CSS-wide keywords, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-129` | completed reuse of bounded named-layer priority and private rollback for inherited `text-align`, `text-align-last`, and `text-justify`, preserving direction mapping, final-line alignment, separator justification, and the existing fixed-cell line/artifact owners | `native-engine-128` | other CSS-wide keywords, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-130` | completed reuse of bounded named-layer priority and private rollback for inherited `white-space`, preserving the five finite modes, inherited/root fallback, and existing hard-break/fixed-cell wrapping owners | `native-engine-129` | other CSS-wide keywords, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, and browser-wide text conformance |
| `native-engine-131` | completed reuse of bounded named-layer priority and private rollback for inherited positive-pixel `line-height`, preserving the `Option<u32>` inherited/root fallback and existing flow/artifact owners | `native-engine-130` | other CSS-wide keywords, zero/negative/relative/percentage values, `all`, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, changed layout/geometry/paint owners, font metrics, and browser-wide text conformance |
| `native-engine-133` | completed reuse of bounded named-layer priority and private rollback for non-inherited `flex-direction`, preserving finite row/row-reverse/column/column-reverse values, local row fallback, finite flex-flow expansion, and existing flex/artifact owners | `native-engine-132` | other CSS-wide keywords, shorthand rollback, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, animation, script, grid, writing modes, intrinsic/percentage sizing, and browser-wide Flexbox conformance |
| `native-engine-134` | completed reuse of bounded named-layer priority and private rollback for non-inherited `flex-wrap`, `justify-content`, `align-items`, `align-self`, and `align-content`, preserving finite values, native fallbacks, finite flex-flow/place-content expansion, and existing flex/artifact owners | `native-engine-133` | other CSS-wide keywords, shorthand rollback, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, animation, script, grid, writing modes, intrinsic/percentage sizing, and browser-wide Flexbox conformance |
| `native-engine-135` | completed reuse of bounded named-layer priority and private rollback for non-inherited `order`, `flex-grow`, `flex-shrink`, and `flex-basis`, preserving finite values, native fallbacks, finite flex expansion, stable visual order, grow/shrink allocation, base-size selection, min/max constraints, and existing layout/artifact consumers | `native-engine-134` | other CSS-wide keywords, shorthand rollback, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, animation, script, grid, writing modes, percentage/intrinsic sizing, and browser-wide Flexbox conformance |

The DOM remains a single-owner arena. Semantic projections are derived views;
they do not become a second mutable source of truth. The document's current
revision is included in every exported native reference. An action resolves
and validates its locator before any state mutation, then advances the
revision once and records bounded native effects. This gives stale-reference
rejection without exposing arena internals through `browser_backend`.

Phase 2 integration chains added by these slices are:

1. HTML parse -> arena attributes/state -> semantic role/name projection.
2. Current document revision -> native reference generation -> locator
   resolution and stale/detached rejection.
3. Dispatcher action request -> native backend -> locator/actionability check
   -> document mutation -> revision/effect record -> `ActionResult`.
4. Dispatcher effects request -> native engine revision/effect state -> bounded
   `EffectsResult`.
5. Disabled/read-only/ambiguous/unsupported targets fail before mutation.
6. Single-select parsing establishes a deterministic option state; an option
   click updates that group, invalidates its prior reference, and records a
   bounded change effect.
7. Hidden-state derivation excludes hidden text, marks semantic targets, and
   rejects hidden actions before mutation.
8. Raw-text/RCDATA tokenizer state prevents markup-looking script, style,
   title, and textarea content from creating nested semantic elements.
9. Bounded stylesheet and inline declarations produce deterministic
   display/visibility state consumed by visible text and actionability.
10. Alignment rollback candidates resolve before the existing inherited
    direction, final-line, and separator-spacing consumers, with no unresolved
    declaration keyword entering line artifacts.
11. Feature-gated runtime construction creates the native backend directly from
    `NativeEngineConfig` without contacting an endpoint or entering automatic
    selection.
12. The native CLI path accepts only local URL shapes, forwards semantic
    navigate/click/type/text/observe/targets operations, and rejects unsupported
    flags before startup.
13. Default builds retain the Chromium CLI value set and cannot select native
    through an omitted runtime or a fallback path.
13. Native layout derives visible element rectangles from the current DOM and
    viewport without creating a second mutable owner.
14. Native point clicks reject malformed/out-of-viewport points and resolve
    through the deepest layout hit to an actionable semantic ancestor before
    any mutation.
15. Native display lists are derived from a matching layout revision and emit
    bounded deterministic commands without mutating the document.
16. Native software surfaces replay a bounded display list into logical RGBA
    pixels without mutating page state.
17. Native style resolution inherits only `color` through bounded DOM parent
    links, and display-list text consumes that resolved value.
18. Native fill/text commands carry bounded logical clips derived from matching
    `overflow:hidden` ancestors, and software replay enforces them.
19. Native uniform border declarations produce matching revisioned `BorderRect`
    commands, preserve deterministic fill/border/text order, and replay only
    inside the layout box and its ancestor clips.
20. Native PNG capture encodes the current logical surface through the real
    backend dispatcher, enforces the stable capture-byte limit, preserves the
    revision, and denies unsupported JPEG/PDF formats explicitly.
21. Native box-model derivation exposes outer and content rectangles, applies
    bounded border/padding insets to child/text origins, applies uniform
    margins to normal flow, and preserves deterministic hit-test ownership.
22. Native vertical scroll dispatch derives a bounded root offset, clamps it to
    content height, maps point hits and display replay through that offset, and
    preserves revision/effect behavior for moved/no-op/unsupported deltas.
23. Native side-specific border declarations cascade independently, contribute
    to outer/content geometry, emit bounded display-list paint data, and replay
    with clipping and root-scroll translation.
24. Native solid/dashed/dotted border styles survive physical-side cascade,
    typed display-list projection, deterministic pattern replay, clipping, and
    root-scroll translation while unsupported styles remain ignored.
25. Native bounded physical border radii survive shorthand expansion and
    cascade, concrete-box normalization, layout hit testing, display-list
    projection, rounded fill/border replay, rectangular ancestor clipping, and
    root-scroll translation while unsupported radius forms remain ignored.
26. Native inline element boxes use the same bounded width calculation for
    preflight line-fit decisions and final layout, so adjacent inline boxes
    wrap deterministically before display-list, hit-test, and scroll consumers
    observe their document-space geometry.
27. Native fixed-pixel line-height values are resolved before child flow, so
    direct text and inline boxes share a deterministic minimum line height while
    explicit element heights retain box-model precedence for layout, paint, and
    hit-test consumers.
28. Native direct text is collapsed and fragmented during the same flow pass
    that places inline boxes, so display-list origins and source order match
    layout coordinates while style and ancestor clips remain owned by the
    containing element.
29. Native collapsed direct text keeps a complete word on the current fixed
    line when it fits, drops its separator when it wraps, and splits only a
    word that exceeds the full line width; the resulting fragments remain in
    source order for paint and scroll consumers.
30. Native physical padding and margin shorthands expand deterministically,
    physical longhands cascade per side, and the resulting top/right/bottom/
    left values feed content origins, normal-flow margins, and all existing
    layout/paint/hit/scroll consumers.
31. Native direct-text flow retains bounded leading/trailing whitespace
    boundaries, joins only source-separated inline-flow items, paints consumed
    separators through the existing text path, and drops separators that would
    begin a fresh wrapped line.
32. Native layout derives one bounded rectangular `overflow:hidden` ancestor
    intersection per layout box, and viewport projection plus point hit-testing
    consume that same document-space clip before root-scroll translation.
33. Native stylesheet and inline-style parsing records bounded sanitized
    diagnostics for unsupported selectors, properties, values, and malformed
    rules; document preparation carries the list atomically with navigation and
    the explicit Rust API reports its revision and truncation state.
34. Native direct surface output and decoded bounded PNG bytes match one
    complete checked-in logical-pixel golden while capture preserves revision
    state and does not add screenshot evidence.
35. Native descendant selector chains resolve through the existing DOM parent
    links before style cascade, visibility, layout, and paint consume the
    computed result; unsupported combinators remain diagnosed and ignored.
36. Native `overflow: clip` contributes the same bounded rectangular ancestor
    intersection as `overflow: hidden` before paint, viewport projection, and
    point hit-testing consume it; it never creates nested or implicit scroll.
37. Native visible `<br>` elements advance the containing integer flow cursor by
    one fixed line-height floor, reset the inline origin, and create no
    semantic/layout/paint node; hidden breaks are ignored.
38. Native inherited `white-space: pre-line` turns bounded LF/CR/CRLF source
    boundaries into the same hard-break cursor transition while retaining
    collapsed spaces and the default `white-space: normal` behavior.
39. Native inherited `white-space: pre` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and does
    not soft-wrap preformatted segments without changing semantic ownership.
40. Native inherited `white-space: pre-wrap` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and
    splits source runs only at deterministic fixed-cell soft-wrap capacity
    without synthesizing semantic, layout, or paint nodes.
41. Native standard padded base64 `data:text/html` payloads pass through the
    existing local resource loader, bounded pre-decode/decoded-body checks,
    navigation revision, and real backend dispatcher while retaining opaque
    origin and rejecting invalid, non-UTF-8, percent-encoded, non-HTML, and
    unbounded payloads without echoing their contents.
42. Native raw URL fragments are removed from local resource lookup but retained
    in navigation evidence; same-resource fragment navigation preserves the
    document owner while advancing revision/history and applies the bounded
    visible-id scroll rule, and explicit Rust back/forward traversal reuses or
    reparses bounded local resources atomically at the history cursor
    boundaries.
43. Native semantic local anchor clicks resolve bounded fragment-only,
    fixture-relative-from-fixture-base, or absolute local hrefs through the
    existing navigation owner; successful same-resource activation preserves
    bounded document state, successful cross-resource activation parses before
    commit, and unsupported hrefs fail without mutation.
44. Native same-resource fragment navigation resolves one exact visible local
    `id`, clamps its document-space top to the root viewport, records that
    bounded scroll offset in history, and restores saved offsets at traversal;
    unresolved targets preserve the current offset.
45. Native semantic links resolve non-absolute references only against the
    current registered fixture host, preflight the normalized local resource,
    and retain the existing same-/different-resource commit, fragment-scroll,
    and failure-atomicity paths; unsupported bases and host changes fail
    before mutation.
46. Native local fragment navigation percent-decodes bounded `%HH` byte
    sequences as UTF-8 before exact visible `id` lookup; malformed or invalid-
    UTF-8 sequences are unresolved targets that preserve scroll while the
    accepted URL/revision/history transition remains intact, and literal `+`
    is not converted to a space.
47. Native decoded fragment lookup resolves exact unique `id` before a unique
    legacy `<a name>` fallback, while duplicate IDs/names fail closed and
    non-layout targets preserve the existing scroll offset.
48. Native `#:~:text=start[,end]` navigation decodes terms independently,
    matches the first complete visible layout run in document order, requires a
    same-run ordered end term when present, and preserves the existing
    history/resource transaction for unsupported forms.
49. Native text-fragment prefix/suffix affixes are matched exactly adjacent to
    the start/end terms inside the first complete visible layout run, while
    malformed or cross-run forms preserve the existing scroll fallback.
50. Native root horizontal scroll uses measured document overflow width,
    clamps x and y independently, records both coordinates in history/effects,
    and projects the same offset through viewport rectangles, hit testing,
    display-list replay, and raster capture.
51. Native inherited `white-space: nowrap` collapses supported whitespace while
    preventing soft wrapping, measures the resulting one-line overflow, and
    reaches the existing root horizontal scroll/projection path.
52. Native positive-pixel `line-height` inherits through the bounded DOM style
    walk when a descendant has no valid local declaration, while explicit child
    values and explicit heights retain precedence through the existing flow,
    display, hit-test, and raster owners.
53. Native root `content_width` measures text through the existing bounded
    overflow clip, preventing fully clipped text from creating a false root
    horizontal scroll range while preserving unclipped and partially visible
    output.
54. Native `overflow-x` and `overflow-y` accept only bounded `hidden`/`clip`
    values, cascade independently from the shorthand, and constrain only the
    selected axis through the same paint, viewport, point-hit, and root-overflow
    clip owner.
55. Native bounded min/max width and height constraints apply after the
    existing content-box or border-box inset conversion, preserve minimum-size
    overflow, and feed the same normal-flow, projection, hit-test, paint,
    raster, and root-scroll owners.
56. Native local opacity values quantize to bounded 8-bit alpha and bracket
    reduced-opacity rendered subtrees with display-list markers; software
    replay composites nested groups inside-out under explicit depth and
    aggregate-layer-pixel limits, while layout and point hit testing remain
    unchanged.
57. Native inherited physical text alignment applies deterministic left,
    centered, or right offsets to complete fixed-cell line items, including
    supported inline boxes and direct text, while preserving line breaking,
    dimensions, scrolling, paint, raster, and hit-test ownership.
58. Native inherited ASCII `text-transform:none|uppercase|lowercase` applies
    before fixed-cell whitespace handling and fragmentation so layout,
    wrapping, text-fragment matching, display-list projection, root-overflow
    measurement, and capture share one transformed output, while semantic
    source text remains unchanged.
59. Native non-negative fixed-pixel `text-indent` applies only to the first
    line of block-container flow, reduces that line's fixed-cell capacity,
    resets subsequent lines to the full content width, and shares the resulting
    coordinates with text fragments, display-list projection, hit testing, and
    root-overflow measurement while inline and `display:contents` elements do
    not create an independent indent context.
60. Native inherited non-negative fixed-pixel `word-spacing` adds a bounded
    advance after rendered ASCII spaces before soft wrapping and preformatted
    chunking; the same measured result reaches text fragments, alignment,
    display-list metadata, raster glyph positions, hit testing, and root
    overflow while source text and whitespace ownership remain unchanged.
61. Native inherited non-negative fixed-pixel `letter-spacing` adds a bounded
    advance after every rendered fixed-cell character in each emitted text
    fragment, including the final character; it composes with word spacing on
    ASCII spaces and reaches wrapping, fragments, alignment, display-list
    metadata, raster glyph positions, hit testing, and root overflow without
    changing source text or joining fragment/line/flow boundaries.
62. Native inherited `font-weight: normal|bold|400|700` changes only the
    fixed-cell glyph replay: normal/400 retain existing pixels and bold/700
    dilate set pixels one column to the right. The advance, layout, semantic
    text, clipping, hit testing, overflow, and capture coordinates remain
    unchanged, keeping this presentation rule inside the existing display-list
    and software-raster owners without a font dependency.
63. Native inherited `font-style: normal|italic` changes only the fixed-cell
    glyph replay: normal retains existing pixels and italic shifts each glyph
    row through a bounded deterministic shear. Advances, layout, semantic
    text, clipping, hit testing, overflow, and capture coordinates remain
    unchanged, while bold dilation and other existing text presentation bits
    compose in the same display-list and software-raster owners.
64. Native inherited `word-break: normal|break-all` changes only collapsed
    fixed-cell line breaking: normal keeps the existing word-aware policy while
    break-all permits character-boundary chunks for every word. Collapsed
    separators, transformed text, word/letter spacing, text fragments, flow
    overflow, alignment, display projection, and semantics continue to consume
    the same measured path; `pre`, `pre-wrap`, and `nowrap` retain their
    existing behavior.
65. Native inherited `vertical-align: baseline|top|middle|bottom` changes only
    the y-origin of recorded inline and inline-block line-item artifact ranges:
    baseline/top remain at the line origin, middle uses the clamped half-gap,
    and bottom uses the clamped full gap. Line height, horizontal flow, line
    breaks, wrapping, semantics, paint order, clipping, hit-test ownership,
    scrolling, and capture remain unchanged; direct block-flow text stays on
    the baseline path.
66. Native `display: flex` creates one bounded forward row only when direct
    children are eligible element items (whitespace-only text is ignored).
    Items retain source order, fixed explicit/intrinsic widths, and margins;
    nested layout, paint, clips, semantics, hit testing, overflow, scrolling,
    and capture consume their actual item origins. Meaningful direct text,
    `display: contents`, and visible `<br>` preserve the existing normal-flow
    fallback; no flex grow/shrink, wrapping, gaps, reverse/column direction,
    or cross-axis distribution is implied.

67. Native wrapped flex rows apply bounded non-inherited
    `align-content:flex-start|center|flex-end|space-between` only after 070
    line formation and per-line `align-items` placement. Positive explicit
    content-box remainder becomes deterministic leading or inter-line offsets;
    auto and undersized content boxes do not create negative coordinates. The
    complete shifted line artifacts feed item boxes, descendants, text runs,
    display-list paint, viewport projection, root overflow, hit testing, and
    capture together, while `nowrap`, semantic/source order, and normal-flow
    fallback remain unchanged.

68. Native wrapped flex rows accept bounded `align-content:space-around` and
    compute each line's offset from a saturating integer slot-center formula over
    positive explicit cross-axis remainder. The complete translated artifacts
    feed the same boxes, descendants, text runs, paint, projection, overflow,
    hit-test, and capture owners; unsupported values and all prior 071 paths
    retain their existing fallback or geometry.

69. Native wrapped flex rows accept bounded `align-content:space-evenly` and
    compute each line's offset from equal leading, inter-line, and trailing
    slots over positive explicit cross-axis remainder. The complete translated
    artifacts feed the same boxes, descendants, text runs, paint, projection,
    overflow, hit-test, and capture owners; unsupported values and all prior
    072 paths retain their existing fallback or geometry.

70. Native eligible fixed-width flex rows accept bounded non-inherited
`flex-wrap:wrap-reverse`. Sorted visible items still form lines in source
order, but physical line origins are reflected from the resolved cross-axis
end using the existing `align-content` offsets. A signed second-pass delta
translates complete line artifacts so descendants, text, paint, projection,
overflow, scrolling, hit testing, and capture agree; `nowrap`, semantic order,
normal-flow fallback, and unsupported Flexbox features remain unchanged.

71. Native wrapped fixed-width flex rows accept explicit bounded
`align-content:stretch`. Positive explicit content-box remainder expands each
formed line by an integer quotient, assigns leftover pixels to the first
formed lines, rebuilds provisional origins, and lets existing `align-items`
place items inside each expanded line. Normal and wrap-reverse stacking,
complete artifacts, overflow, scrolling, hit testing, capture, and
semantic/source order remain one shared path; auto, undersized, and `nowrap`
geometry retain their existing behavior.

72. Native eligible wrapped fixed-width flex rows accept explicit bounded
`align-content:normal` as an alias for the completed stretch line-box owner.
Positive explicit remainder expands formed lines by the same deterministic
integer shares, then the normal and wrap-reverse artifact passes consume one
set of final coordinates. Omitted `align-content` remains the existing bounded
`flex-start` fallback; semantic/source order and all non-eligible geometry stay
unchanged.

73. Native inherited `white-space: revert-layer` rolls back a winning
declaration through the bounded named-layer and unlayered/inline candidates,
preserving the five finite whitespace modes, inherited/root fallback, hard
breaks, literal whitespace, fixed-cell soft wrapping, and all existing
line-flow/artifact consumers.

74. Native inherited positive-pixel `line-height: revert-layer` rolls back a
winning declaration through the bounded named-layer and unlayered/inline
candidates, preserving `Option<u32>` inherited/root fallback, fixed-cell flow
minimums, inline auto-height, explicit-height precedence, and all existing
flow/artifact consumers.

The semantic action tradeoff is intentional: it provides a real backend path
for deterministic local fixtures while leaving general geometry to Phase 3.
The 009 seed now gives Phase 3 a bounded geometry owner and executable point
input, but it still cannot claim browser line layout, paint, scrolling, or
visual stacking.

## Tradeoffs and mitigations

| Decision | Benefit | Cost / what we miss | Mitigation |
|---|---|---|---|
| custom small HTML parser | owns the DOM boundary and keeps the default graph unchanged | not HTML5-conformant yet; malformed markup coverage is narrow | explicit Phase 2 conformance work and parser fixtures |
| fixture/data-only loader | deterministic, no SSRF/filesystem risk, fast tests | no real web navigation or network behavior | typed unsupported URL errors and later security workstream |
| in-process single owner | simple revision/history invariants and reproducible tests | no crash isolation or hostile-content safety | keep content local-only; process isolation is a promotion gate |
| no async task callbacks | deterministic scheduler with no hidden sleeps/threads | no script/event-loop realism | typed task kinds and test clock establish the future seam |
| single-select only | useful basic form semantics with a small deterministic state model | no keyboard, multi-select, or submission behavior | reject unsupported variants explicitly and keep values private |
| bounded visibility gate | keeps semantic text/actionability consistent without a CSS dependency | no cascade, layout, opacity, or paint semantics | recognize only explicit hidden signals and document the boundary |
| bounded raw-text/RCDATA modes | prevents fake semantic nodes in embedded text while preserving the small parser | no full HTML5 insertion-mode or foreign-content recovery | keep the mode set explicit and cover unterminated content with fixtures |
| bounded CSS presentation seed | makes stylesheet-driven hiding observable without adding a rendering stack | no general CSS, inheritance, layout, or paint semantics | keep selectors/properties explicit and reject unsupported syntax by omission |
| feature-gated runtime/CLI entry | makes the experiment runnable through the same explicit one-shot contract | native CLI cannot register fixtures, start a browser, or accept remote URLs; feature builds have another compile path | keep default builds unchanged, use the Rust constructor for fixtures, and validate native/default matrices separately |
| bounded normal-flow layout and point hit testing | exercises geometry ownership and input validation without a renderer | no browser line metrics, scrolling, stacking contexts, or fractional CSS | keep rectangles Rust-only, use integer pixels, and reject unsupported dimensions/points explicitly |
| derived display-list seed | establishes a renderer-owned immutable artifact without pixel dependencies | no rasterization, fonts, image decode, clipping, or visual evidence | require a matching layout revision, bound commands, and keep the list Rust-only |
| bounded software surface | makes the display-list contract executable with no graphics dependency | no font fidelity, Unicode shaping, images, screenshots, or physical-pixel guarantees | cap logical pixels, use fixed glyphs, clip writes, and keep the surface Rust-only |
| inherited text color | makes nested text styling observable without broadening the CSS grammar | no general inheritance, user-agent styles, font/color management, or style cache | resolve one property through bounded parent links and keep explicit child declarations authoritative |
| bounded paint clipping | makes descendant overflow behavior explicit without inventing nested scrolling | no nested scroll offsets, visible overflow model, border box-model, transforms, or clip stack cache | carry half-open ancestor intersections on immutable commands and intersect again at replay |
| bounded uniform border paint | adds a useful edge primitive without changing layout ownership | no padding/box sizing, individual sides, non-solid styles, or border geometry | paint an inside-the-box ring as one immutable command and reuse existing clip/source-over bounds |
| bounded PNG capture | makes the current logical renderer artifact consumable through the stable capture operation | no physical-device pixels, screenshot evidence schema, JPEG/PDF, image/font fidelity, or capture modes | encode only the bounded current surface, enforce `MAX_CAPTURE_BYTES`, keep capture read-only, and deny unsupported formats |
| bounded box-model layout | gives border, child flow, text origin, and hit testing one explicit outer/content geometry owner | no general box sizing, logical sides, margin collapsing, fractional metrics, or positioned/flex/grid layout | support only bounded non-negative physical pixel values, expose `content_rect`, and keep unsupported values out of the computed style |
| bounded root viewport scrolling | makes the existing action/layout/paint path observable across a tall local document | no horizontal/nested/smooth scrolling, scroll anchoring, or keyboard behavior | retain document-space boxes, carry one explicit offset, translate only at hit-test/replay boundaries, and clamp all deltas |
| bounded side-specific solid borders | makes common asymmetric card/control edges observable while reusing the existing box model | no non-solid styles, border radius/images/gradients, logical sides, or browser corner joins | cascade four physical sides independently, retain integer geometry, and define deterministic replay precedence |
| bounded dashed/dotted borders | makes common patterned card/control edges observable without adding a CSS painting dependency | no other border styles, radius/images/gradients, standalone style properties, logical sides, anti-aliasing, or browser dash metrics | carry a typed per-side style, anchor bounded integer patterns to document-space geometry, and reuse clip/source-over/root-scroll replay |
| bounded circular border radius | makes common rounded controls/cards and their hit ownership observable without importing a CSS geometry engine | no percentages, elliptical radii, corner longhands, rounded descendant clips, anti-aliasing, or browser corner fidelity | expand one-to-four integer-pixel shorthand values, conservatively normalize to each box, and share the rounded mask between hit testing and software replay |
| bounded inline line placement | prevents inline element boxes from being committed at an overflowing x-coordinate before the cursor is flushed | no font metrics, whitespace/word fragments, baseline/bidi/floats, replaced elements, or flex/grid | preflight the same integer outer width used by final layout, flush whole boxes at line boundaries, and retain deterministic fixed line-height behavior |
| bounded fixed line height | makes fixture line boxes intentionally dense or spacious without importing font metrics | no `normal`, relative/percentage values, general inheritance, baselines, vertical-align, or browser line metrics | parse one positive pixel value, apply it as a local flow floor, and keep explicit height authoritative |
| bounded direct-text flow fragments | prevents mixed direct text and inline descendants from overlapping or being painted at a stale content origin | no font metrics/shaping, CSS whitespace modes, general word breaking, baselines, bidi, cross-node whitespace joining, or browser inline parity | collapse once, fragment from the shared flow cursor, and carry bounded source-order entries into paint |
| bounded word-aware text wrapping | keeps fixture words together when the fixed line can hold them while retaining a bounded fallback for long tokens | no CSS whitespace/word-break/overflow-wrap modes, hyphenation, font metrics/shaping, baselines, bidi, cross-node whitespace joining, or browser inline parity | treat collapsed text as ASCII-space-separated words, place complete words first, and split only over-wide words by fixed character capacity |
| bounded physical box edges | makes asymmetric local fixture geometry observable while preserving one layout owner and no new dependency | no logical writing-mode sides, negative/percentage/auto values, margin collapsing, or general CSS layout | expand bounded physical shorthand values, cascade four sides independently, use saturating integer arithmetic, and feed one document-space geometry to all consumers |
| bounded whitespace boundaries | preserves intentional separators without inventing spaces between separate direct-text nodes | no CSS whitespace modes, preserved tabs/newlines, word spacing, Unicode line breaking, bidi, font metrics/shaping, or cross-owner inline parity | retain only source boundary bits, share one pending separator in the containing flow, paint it through existing fragments, and drop it at line starts |
| bounded overflow hit-test/projection clips | keeps layout visibility and point interaction aligned with existing `overflow:hidden` paint clipping | no visible overflow, nested/axis-specific scrolling, rounded descendant clips, stacking contexts, or transforms | retain one bounded document-space ancestor intersection per layout box and apply it before viewport translation and rounded hit testing |
| bounded unsupported-CSS diagnostics | makes the narrow CSS contract auditable without changing its deterministic fallback behavior | no general CSS parser, conformance location model, raw stylesheet echo, or stable transport capability | retain a fixed diagnostic bound, report sanitized source/category tokens, and replace diagnostics atomically with the prepared document |
| bounded pixel-golden capture | detects full-frame drift in the existing logical renderer and PNG encoder | no physical-pixel, font/image, anti-aliasing, color-management, or screenshot compatibility claim | keep one tiny fixed logical fixture, compare direct and decoded output, and retain capture as a read-only explicit operation |
| bounded descendant selectors | makes ancestor-scoped fixture styles observable through the real DOM ownership chain | no sibling/child/pseudo/functional/namespace selectors, selector caching, general CSS conformance, or browser parity | cap chains at eight compounds, traverse only bounded parent links, sum existing specificity, and preserve explicit unsupported-selector diagnostics |
| bounded `overflow: clip` | removes a common false diagnostic while reusing the existing rectangular clip invariant | no axis-specific overflow, nested scrolling, scrollbars, rounded descendant clips, or public distinction from `hidden` | accept only the single value, map both supported values to one internal clip bit, and preserve bounded diagnostics for `visible`/`auto`/`scroll` |
| bounded hard line breaks | makes author-visible `<br>` structure affect fixture flow without adding a renderer or font engine | no `<wbr>`, preserved source newline, CSS whitespace, font metric, Unicode line-breaking, bidi, or browser inline-formatting claim | move only the existing bounded cursor, emit no synthetic node, and keep hidden/display:none breaks inert |
| bounded `white-space: pre-line` breaks | preserves common source newlines through the proven hard-break transition while retaining a collapsed-space model | no `pre`/`pre-wrap`/`break-spaces`/`nowrap`, tabs, arbitrary whitespace, text alignment, font metrics, Unicode line breaking, bidi, or browser parity | inherit one explicit mode through the existing bounded style walk, normalize CRLF, and reuse the `<br>` flow transition |
| bounded `white-space: pre` | preserves bounded literal source whitespace and line boundaries without inventing soft-wrap or tab-stop policy | no `pre-wrap`/`break-spaces`/`nowrap`, browser tab stops, wide-line reflow or horizontal scrolling, text alignment, word spacing, Unicode line breaking, bidi, font metrics, or browser parity | inherit one explicit mode through the existing style walk, retain fixed-cell text runs, normalize CRLF, and reuse the proven hard-break transition |
| bounded `white-space: pre-wrap` | retains authored whitespace while making bounded preformatted content usable in narrow fixed-cell lines | no `break-spaces`/`nowrap`, browser line-breaking opportunities, tab stops, wide-line scrolling, text alignment/justification, word spacing, Unicode line breaking, bidi, font metrics/shaping, or browser parity | inherit one explicit mode through the existing style walk, split source text at deterministic cell capacity, normalize CRLF, and reuse the proven hard-break transition |
| bounded base64 `data:text/html` loading | makes self-contained local HTML fixtures transportable through the real navigation path without network access | no URL-safe/whitespace-tolerant decoding, percent-encoded base64, alternate charsets, subresources, cancellation, network/filesystem policy, or arbitrary data-URL modes | reject non-HTML metadata, enforce a derived encoded bound before decode and the decoded document limit after decode, require UTF-8, and retain the existing opaque local origin |
| bounded fragment navigation and history traversal | makes local URL/history behavior observable through the existing navigation owner without a second document store or network dependency | no anchor scrolling, mutable document snapshots, redirects, HTTP history, credentials/cookies, or shared transport operation | remove only raw fragments for resource lookup, preserve URL/document state for same-resource entries, parse different resources before moving the cursor, and return explicit boundary no-ops |
| bounded local link activation (038 baseline) | makes existing semantic link clicks reach the proven local navigation/history owner without adding a new transport operation | at the baseline: no relative URL base resolution, remote navigation, download/default-action event loop, target contexts, or network policy | resolve fragment-only and absolute local hrefs, preflight the resource, then commit the existing same-document or parsed navigation path; leave empty href as a click-only action |
| bounded fragment-target scrolling | makes local fragment navigation useful without adding a general scroll engine or retaining full documents | no percent-decoding, name/text fragments, duplicate-id recovery, smooth/nested/horizontal/keyboard/snap scrolling, sticky layout, or browser alignment parity | match one exact raw `id`, use its visible document-space top clamped to the root viewport, and retain one bounded scroll point per history entry |
| bounded fixture-relative link resolution | makes common local relative links usable without opening a network or general origin policy | no relative resolution from opaque data/about resources, host changes, remote schemes, credentials, redirects, downloads, or browser URL parity | resolve with the existing URL parser from a fixture base, enforce the fixture scheme/host, and route the normalized result through the existing loader before mutation |
| bounded percent-decoded fragment targets | makes encoded spaces and UTF-8 local IDs reachable without changing resource lookup or history representation | no arbitrary URL decoding, malformed-fragment error policy, or browser alignment parity | decode only bounded `%HH` bytes as UTF-8, preserve literal `+`, match after decoding, and treat malformed/invalid targets as unresolved while retaining the existing scroll fallback |
| bounded legacy `name` fragment targets | makes common historical `<a name>` anchors reachable without broadening arbitrary attribute semantics | no arbitrary `name` targets, duplicate recovery, or browser URL/scrolling parity | use one exact visible `<a name>` only after no matching `id`, preserve ID precedence, and fail closed on duplicate names |
| bounded text fragments | makes simple text-directed local links useful without a second semantic text tree or highlight renderer | no multiple directives, cross-run ranges, highlighting, Unicode normalization, or browser parity | parse raw term separators before decode, match one visible layout run in document order, and preserve the existing scroll fallback |
| bounded text-fragment affixes | makes common prefix/suffix disambiguation useful without adding a range/highlight tree | no cross-run ranges, highlighting, Unicode normalization, or browser parity | strip only raw affix markers before bounded per-term decoding, require exact adjacency in one visible layout run, and preserve the existing scroll fallback |
| bounded root horizontal scrolling | makes wide deterministic local content reachable through the existing root scroll owner without nested scroll state | no nested scrolling, scrollbars, smooth/keyboard/snap scrolling, axis-specific overflow, or browser parity | derive bounded document width from visible layout output, clamp x/y independently, and reuse the existing projection/history path |
| bounded `white-space: nowrap` | preserves collapsed text on one fixed-cell line so the root horizontal path can reach wide local content | no preserved whitespace, nested scrolling, scrollbars, Unicode line breaking, font metrics, or browser parity | inherit one explicit mode through the existing style walk, disable soft wrapping only for that mode, and measure the resulting text runs |
| bounded inherited fixed `line-height` | keeps nested fixture line boxes consistent with the nearest positive pixel floor without importing font metrics | no relative/unitless/percentage values, CSS-wide keywords, baselines, vertical-align, general inheritance, or browser line metrics | carry only the already-supported positive pixel value through the existing parent walk, preserve explicit child/height precedence, and reuse the current flow minimum |
| bounded clip-aware root overflow | keeps root scrolling consistent with the already shared overflow clip and avoids false scroll ranges from invisible text | no browser overflow propagation, nested scroll state, scrollbars, axis-specific overflow, visible overflow parity, or font metrics | intersect each non-empty non-truncated text run with the existing document-space clip before deriving its right edge; retain box extents and all current projection owners |
| bounded axis-specific overflow | keeps one axis visible while clipping the other through the existing rectangle and independently cascaded CSS declarations | no nested scrolling, scrollbars, mixed visible/auto/scroll used values, rounded descendant clips, or browser overflow parity | carry two bounded clip bits, map the selected axis to a finite document-space range, and reuse paint/projection/hit-test/root-overflow consumers |
| bounded min/max dimensions | makes common minimum and maximum box constraints observable without replacing the integer box-model owner | no negative/percentage/auto values, intrinsic sizing, aspect ratio, margin collapsing, positioning, flex/grid, or browser sizing parity | convert bounds through existing insets, apply lower/upper constraints before available-width clamping, and preserve deterministic outer/content rectangles |
| bounded opacity groups | makes local translucent subtrees compose as one paint group without changing the layout owner | off-screen layers add memory and a second pixel pass; no stacking contexts, transforms, filters, animation, or browser compositor parity | parse bounded fixed-point alpha, bracket only reduced-opacity rendered subtrees, cap group depth and aggregate layer pixels, and keep opacity out of layout/hit visibility |
| bounded text alignment | keeps direct text and supported inline boxes visually and interactively together within a line | no justification, logical-direction, writing-mode, vertical-alignment, bidi, font-metric, or browser inline-formatting parity | track bounded line-item artifact ranges and apply deterministic fixed-cell left/center/right offsets at line flush without changing line breaks or dimensions |
| bounded functional alpha colors | makes common translucent background, border, and text fixtures expressible through the existing paint path | no CSS Color 4 syntax, channel percentages, modern space-separated functions, wide-gamut colors, interpolation, or color-management parity | parse decimal integer channels plus the existing fixed-point alpha grammar, then reuse immutable `NativeColor` and integer source-over replay |
| bounded fixed-cell text decoration | makes a deterministic one-pixel underline available across supported text fragments | no font metrics, descender-aware placement, decoration propagation parity, styles, colors, thickness, offsets, overline, line-through, blink, or browser parity | inherit one bounded `none`/`underline` value, carry it in immutable text commands, and draw a clipped alpha-aware line at the fixed glyph baseline offset |
| bounded inherited text transform | makes common ASCII case presentation available across the existing fixed-cell text-flow path | no Unicode case mapping or expansion, locale-sensitive casing, `capitalize`/other transforms, font shaping/metrics, or browser text-rendering parity | inherit one bounded `none`/`uppercase`/`lowercase` value, transform ASCII letters during layout, preserve source semantic text, and keep fixed-cell width invariant |
| bounded first-line text indent | makes common block first-line indentation observable without a second formatting context | no negative/hanging indentation, keyword/percentage/font-relative units, bidi/logical writing modes, or browser CSS parity | keep one non-negative fixed-pixel value in computed style, clamp it to leave one fixed cell, and switch the existing flow cursor to full width after the first line |
| bounded inherited word spacing | makes common fixed-pixel separator spacing observable across the existing text-flow and paint owners | no negative spacing, `letter-spacing`, relative/percentage units, Unicode whitespace or word-boundary policy, browser tab stops, font metrics, bidi, or browser parity | inherit one bounded non-negative pixel value, add it only after rendered ASCII spaces, and carry the measured advance through wrapping, fragments, alignment, display, raster, hit testing, and overflow |
| bounded inherited letter spacing | makes a bounded per-character fixed-cell advance observable across the existing text-flow and paint owners | no negative/relative/percentage values, `normal`, pair-boundary or cross-fragment semantics, Unicode shaping/metrics, grapheme clusters, bidi, or browser parity | inherit one bounded non-negative pixel value, add it after every rendered character in each emitted fragment, compose it with word spacing, and carry the measured advance through wrapping, fragments, alignment, display, raster, hit testing, and overflow |
| bounded inherited font weight | makes normal and bold fixed-cell text presentation observable without changing geometry | real font selection/loading/metrics, numeric interpolation, variable fonts, synthetic-bold policy, Unicode shaping, anti-aliasing, and browser text-rendering parity | inherit one normalizable two-state value, keep fixed-cell advances unchanged, carry it on immutable text commands, and dilate bold glyph pixels through the existing clipped software replay |
| bounded inherited font style | makes normal and italic fixed-cell text presentation observable without changing geometry | oblique angles, real italic faces, font selection/loading/metrics, variable fonts, Unicode shaping, anti-aliasing, and browser text-rendering parity | inherit one normal/italic value, keep fixed-cell advances unchanged, carry it on immutable text commands, and apply a bounded row-dependent shear through the existing clipped software replay |
| bounded inherited word break | makes explicit character-boundary wrapping available for collapsed fixed-cell words | `keep-all`, `break-word`, `overflow-wrap`, Unicode/CJK line breaking, grapheme policy, hyphenation, bidi, writing modes, font metrics, and browser parity | inherit one normal/`break-all` value, retain the current separator policy, and reuse fixed-cell capacity to split every collapsed word without changing semantic source text |
| bounded text overflow | makes clipped single-line fixture text visibly indicate an omitted suffix without changing semantic source text | multi-line truncation, line-clamp, nested inline formatting, multiple text nodes, visible overflow, RTL/vertical writing, Unicode ellipsis, grapheme policy, font metrics, shaping, and browser parity | resolve local `clip`/`ellipsis`, require a rendered nowrap block with one direct text child and horizontal clipping, then reuse fixed-cell prefix measurement and immutable text commands for a bounded ASCII marker |
| bounded vertical alignment | makes fixed-cell inline items visibly align within the existing line box without adding a typographic baseline engine | font ascent/descent, real baselines, `text-top`/`text-bottom`, sub/super, lengths, percentages, bidi, writing modes, ruby, table-cell/replaced-element alignment, and browser parity | inherit four bounded keywords, keep the current line-box height and horizontal flow, and shift each recorded inline item's box/text artifact range by a clamped top/middle/bottom offset during line flush |
| bounded flex row | makes common horizontal card/control groups observable without replacing the existing box-model, paint, or hit-test owners | column directions, anonymous text items, `display:contents` flattening, and browser Flexbox parity | accept block-level `display:flex`, lay eligible direct element children once in source order at fixed explicit/intrinsic widths, preserve normal-flow fallback for unsupported child shapes, and reuse document overflow/projection consumers |
| bounded flex-row justification | makes common fixed-width rows support leading, centered, trailing, space-between, space-around, and space-evenly distribution without a second layout owner | fractional/subpixel rounding, auto margins, negative-free-space fallback, column directions, logical direction/RTL, anonymous items, and browser Flexbox parity | preflight existing item widths/margins, preserve the configured gap as a minimum, distribute only positive free space with documented cumulative integer offsets, and clamp overflowing rows to a zero leading offset |
| bounded flex-item order | makes common fixed-width rows support deterministic visual reordering without changing semantic/source identity | column directions, anonymous text-item sorting, semantic/accessibility/keyboard reordering, stacking-context parity, and browser Flexbox parity | parse a bounded signed integer, sort eligible visual items by `(order, source_index)`, preserve source-order semantic evidence, and reuse the existing gap/justification/layout/paint/hit-test owners |
| bounded flex cross-axis alignment | makes common fixed-width rows with different item heights visually align without adding a second layout owner | baseline/logical alignment, auto margins, column directions, fractional/intrinsic sizing, and browser Flexbox parity | parse bounded non-inherited item values, determine an explicit or maximum-item line height, lay out once at the top edge, and translate each complete item artifact range by a clamped integer offset |
| bounded flex direction | makes common fixed-width rows and eligible fixed-height columns support deterministic physical direction while preserving semantic/source identity | wrapping columns, multi-line cross-axis distribution, logical direction/RTL, auto margins, percentage/fractional/intrinsic sizing, and browser Flexbox parity | parse non-inherited `row|row-reverse|column|column-reverse`, preserve the existing order-sorted sequence, map justification and row-gap to the selected physical axis, perform a margin-aware forward/reverse walk, and route complete subtrees through the shared layout, paint, overflow, scroll, hit-test, and capture consumers |
| bounded flex column wrapping | makes eligible fixed-height column and column-reverse containers form deterministic vertical main-axis lines across the horizontal cross axis | wrap-reverse, auto-height columns, percentage/fractional/intrinsic main sizing, auto margins, logical direction/writing modes, baseline alignment, grid, and browser Flexbox parity | require fixed-height column containers and visible direct elements with bounded main sizes, form order-sorted lines using row-gap, size and justify each line independently, distribute line widths with column-gap and existing align-content offsets, and reuse complete subtree/artifact consumers |
| bounded flex wrapping | makes common fixed-width rows form deterministic physical lines without adding a second layout owner | column directions, auto margins, logical direction/RTL, percentage/fractional/intrinsic sizing, and browser Flexbox parity | parse non-inherited `nowrap|wrap`, partition sorted visible items by integer outer width plus the existing gap, reuse per-line row/reverse placement and alignment, stack maximum-height lines, and preserve shared subtree/scroll consumers |
| bounded flex cross-line alignment | makes explicit-height wrapped rows use positive cross-axis space without changing line membership or item sizing | fractional distribution, auto margins, column directions, logical direction/RTL, and browser Flexbox parity | retain provisional line records, resolve only positive explicit-height remainder, allocate deterministic integer leading/inter-line offsets, and translate complete line artifact ranges after per-line item alignment |
| bounded flex cross-line space-around | makes explicit-height wrapped rows distribute positive remainder around each physical line with deterministic slot centers | fractional distribution, auto margins, column directions, logical direction/RTL, and browser Flexbox parity | retain formed line records, calculate saturating integer `floor(remainder * (2*i + 1) / (2*line_count))` offsets, and translate complete line artifact ranges through the existing consumers |
| bounded flex cross-line space-evenly | makes explicit-height wrapped rows distribute positive remainder into equal leading, inter-line, and trailing slots | fractional distribution, auto margins, column directions, logical direction/RTL, and browser Flexbox parity | retain formed line records, calculate saturating integer `floor(remainder * (i + 1) / (line_count + 1))` offsets, and translate complete line artifact ranges through the existing consumers |
| bounded flex wrap-reverse | makes eligible wrapped rows stack formed lines from the physical cross-axis end without adding a second layout owner | column directions, auto margins, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity | retain source-order line formation and provisional records, reflect each line from the resolved cross-axis end using existing integer `align-content` offsets, and apply one signed coordinate delta to every line artifact range |
| bounded flex cross-line stretch | makes explicit-height wrapped rows expand each formed line using the existing item-alignment owner | fractional distribution, auto margins, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity | retain formed line records, distribute only positive remainder by integer quotient plus first-line remainder pixels, rebuild line origins, then reuse `align-items` and complete artifact translation for normal and wrap-reverse rows |
| bounded flex cross-line normal | makes the explicit `normal` keyword select the proven stretch line-box behavior without duplicating layout ownership | changed omitted-value initial-value semantics, fractional distribution, auto margins, column directions, logical direction/RTL, intrinsic or percentage sizing, and browser Flexbox parity | parse a separate non-inherited keyword, route it through the existing stretch branch for eligible wrapped rows, preserve the established omitted-value `flex-start` fallback, and reuse all complete artifact consumers |
| bounded flex row-gap | makes explicit cross-line spacing observable while preserving one line-record and artifact owner | percentage/fractional values, auto margins, column directions, logical direction/RTL, and browser Flexbox parity | insert one bounded integer-pixel gap between provisional wrapped lines, include it once before `align-content` distribution, and reuse the existing normal/reverse artifact, overflow, paint, and hit-test consumers |
| bounded flex gap family | makes one- and two-axis gap spacing and shorthand/longhand precedence observable through one shared flex geometry owner | percentage/fractional lengths, negative values, column-direction flex, grid, auto margins, logical direction/RTL, and browser Flexbox parity | preserve per-rule valid declaration positions only for the interacting gap family, resolve row/column axes with specificity/order/inline precedence, then feed column spacing and cross-line spacing into the existing item/line records |
| bounded flex grow | makes positive fixed-width row free space observable through weighted item sizing while preserving one line/coordinate owner | fractional factors, full flex base-size reflow, auto margins, column directions, percentage/intrinsic sizing, and browser Flexbox parity | resolve bounded integer grow factors, allocate positive line remainder with prefix-floor shares, freeze max-width items and redistribute their remainder, then pass final outer widths through existing flex/justify/paint/hit/scroll consumers |
| bounded flex shrink | makes negative fixed-width row free space observable through weighted item reduction while preserving one line/coordinate owner | fractional factors, full flex base-size reflow, auto margins, column directions, percentage/intrinsic sizing, and browser Flexbox parity | resolve bounded integer shrink factors, weight them by original flex base widths, allocate deficit with prefix-floor shares, freeze effective min-width floors and redistribute remainder, then pass final outer widths through existing flex/justify/paint/hit/scroll consumers |
| bounded flex basis | makes explicit fixed-pixel or `auto` flex bases override item width before the shared line-formation, grow, and shrink owners | percentages, fractional lengths, `calc()`, `content`, intrinsic sizing changes, auto margins, columns, and browser Flexbox parity | resolve non-inherited `auto|Npx`, reuse box-sizing and min/max conversion, leave explicit bases unclamped before line formation, and pass final widths through the existing geometry consumers |
| bounded flex shorthand | makes common `flex` presets and compact grow/shrink/basis declarations feed the completed component owners with CSS-like declaration-order precedence | CSS-wide reset keywords, percentage/fractional bases, fractional factors, ambiguous token forms, auto margins, columns, and browser Flexbox parity | expand bounded `none`, `auto`, integer-factor, and pixel/`auto` basis forms directly into the existing component fields, preserve valid source-order longhand overrides, and reuse the existing flex sizing and artifact consumers |
| bounded flex-flow shorthand | makes common row/reverse-row and fixed-height column/no-wrap combinations feed the completed direction and wrapping owners with shorthand reset and source-order precedence | wrapping columns, multi-line cross-axis distribution, logical direction/RTL, duplicate tokens, CSS-wide reset keywords, ambiguous forms, and browser Flexbox parity | classify one or two bounded direction/wrap tokens including column directions, expand omitted components to row/nowrap, preserve valid longhand overrides, and reuse the existing physical placement, line formation, artifact, overflow, and hit-test consumers |
| bounded flex align-self | makes common per-item cross-axis overrides observable without adding a second line or artifact owner | baseline metrics, logical/safe alignment, auto margins, column directions, fractional/intrinsic sizing, and browser Flexbox parity | parse non-inherited bounded item values, resolve `auto` against the parent `align-items` at placement time, and translate the complete item subtree through existing boxes, text, paint, overflow, projection, hit-test, scroll, and capture consumers |
| bounded flex place-content | makes common two-axis flex distribution declarations feed the existing line and main-axis owners without adding a second cascade or geometry representation | full CSS shorthand grammar, logical direction/writing modes, safe/unsafe alignment, grid, fractional/intrinsic sizing, and browser Flexbox parity | parse one shared or two explicit bounded tokens, expand them to `align-content` and `justify-content` at the existing declaration precedence, and reuse the current wrapped-line/main-axis distribution and complete artifact consumers |
| bounded flex align-self stretch | makes auto-height direct flex items fill the existing line cross size without replacing the natural child layout or line owner | baseline/logical alignment, auto margins, column directions, fractional/intrinsic sizing, and browser Flexbox parity | parse the additional non-inherited item value, retain the natural bounded child layout, adjust only the eligible root box to the line cross size subject to existing physical insets and min/max heights, and reuse complete artifact consumers |
| bounded flex auto margins | makes common `margin:auto` edges absorb positive free space through the existing no-wrap row and fixed-height column owners without adding a second geometry representation | wrapped-line auto margins, auto-height columns, normal-flow centering, logical writing modes, baseline alignment, intrinsic/percentage sizing, grid, and browser Flexbox parity | retain auto-edge provenance while treating auto margins as zero during flex sizing, allocate positive main-axis remainder before justify and positive cross-axis remainder before item alignment with deterministic integer shares, preserve reverse/source order, and reuse shared layout, overflow, hit-test, paint, raster, capture, and semantic consumers |
| bounded flex wrapped auto margins | makes common wrapped rows and fixed-height column lines resolve `margin:auto` independently per formed line without adding a second line or artifact owner | auto-height columns, new intrinsic/percentage sizing, fractional lengths, normal-flow centering, logical writing modes, baseline alignment, grid, and browser Flexbox parity | keep auto edges zero for line formation and provisional sizing, resolve positive main/cross remainder after final line and `align-content` sizing with deterministic per-line shares, preserve reverse and wrap-reverse physical mapping, and reuse shared layout, overflow, hit-test, paint, raster, capture, and semantic consumers |
| bounded text justification | makes collapsed soft-wrapped text consume positive line remainder through explicit, observable separator expansion | final-line/hard-break behavior, preformatted and break-all whitespace, bidi/shaping, language-specific line breaking, logical properties, vertical writing, and browser text conformance | record a soft-wrap line boundary, count emitted eligible separators, distribute integer remainder in source order, and carry the extra per-space advance through text layout, display-list, raster, overflow, scrolling, capture, and inline subtree translation |
| bounded text-justification control | makes explicit inherited `none` suppression and `auto`/`inter-word` selection observable without changing line ownership or base word spacing | inter-character distribution, language-specific word boundaries, bidi/shaping, font metrics, logical properties, vertical writing, fractional metrics, and browser text conformance | inherit a three-state bounded value, gate the existing soft-wrap/final-line separator expansion at the shared flush owner, preserve base spacing and all artifact consumers, and retain typed diagnostics for unsupported modes |
| bounded alignment rollback | extends the proven layer rollback across the three related inherited line-alignment owners without changing artifact schemas | private declaration types and candidate arrays add small parser/cascade code and do not provide generic CSS-wide semantics | keep the public enums finite, resolve before line flush, test ordinary/final-line/justification paths together, and retain typed boundaries for unsupported origins and keywords |
| bounded whitespace rollback | extends the proven layer rollback to the inherited whitespace owner without changing line-flow or artifact schemas | private declaration types and per-layer candidates add property-local cascade state; no generic CSS-wide semantics, multiple origins, or browser whitespace parity | keep the five public whitespace modes finite, resolve rollback before the existing inherited style walk feeds flow, test repeated/unlayered/inline fallback across every supported mode, and retain typed boundaries for unsupported keywords |
| bounded line-height rollback | extends the proven layer rollback to the inherited positive-pixel line-height owner without changing flow or artifact schemas | private declaration types and per-layer candidates add property-local cascade state; no generic CSS-wide semantics, changed omitted-value behavior, font metrics, or browser line-height parity | keep `Option<u32>` and the existing root `None` fallback, resolve rollback before the style walk feeds line flow, test inherited/explicit-height interactions, and retain typed boundaries for unsupported values |
| bounded direction rollback | completed extension of the proven layer rollback to the inherited `direction:ltr|rtl` owner without changing text, flex, wrapping, or artifact schemas | private declaration types and per-layer candidates add property-local cascade state; no generic CSS-wide semantics, bidi, writing modes, or browser direction parity | keep the public direction enum finite, resolve rollback before logical-edge and flex mapping, test text/row/column/wrapped-line physical consumers together, preserve source/semantic order, and retain typed boundaries for unsupported values |
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

## Delivery evidence

The task file for each slice owns its touched paths and verification commands;
`docs/plan/tasks/native-engine-135.md` is the latest completed task;
`docs/plan/tasks/native-engine-134.md` is the preceding completed task;
`docs/plan/tasks/native-engine-133.md` is the preceding completed task;
`docs/plan/tasks/native-engine-131.md` is the preceding completed task;
`docs/plan/tasks/native-engine-130.md` is the preceding completed task;
`docs/plan/tasks/native-engine-129.md` is the earlier completed task;
`docs/plan/tasks/native-engine-127.md` is the preceding completed task;
`docs/plan/tasks/native-engine-126.md` is the preceding completed task;
`docs/plan/tasks/native-engine-125.md` is the preceding completed task;
`docs/plan/tasks/native-engine-124.md` is the preceding completed slice;
`docs/plan/tasks/native-engine-123.md` is the preceding completed slice;
`docs/plan/tasks/native-engine-122.md` is the preceding completed slice;
`docs/plan/tasks/native-engine-121.md` is the earlier completed task;
`docs/plan/tasks/native-engine-120.md` is the preceding completed task;
`docs/plan/tasks/native-engine-119.md` is the earlier completed task;
`docs/plan/tasks/native-engine-118.md` is the latest completed checkpoint;
`docs/plan/tasks/native-engine-117.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-116.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-115.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-114.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-113.md` is the earlier completed checkpoint;
`docs/plan/tasks/native-engine-112.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-111.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-110.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-109.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-108.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-107.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-106.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-105.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-103.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-102.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-101.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-100.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-099.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-098.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-095.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-094.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-093.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-092.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-085.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-083.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-082.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-081.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-080.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-079.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-078.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-077.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-076.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-075.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-074.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-069.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-068.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-067.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-066.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-065.md` is the earlier completed checkpoint;
`docs/plan/tasks/native-engine-064.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-063.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-062.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-061.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-060.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-059.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-058.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-057.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-056.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-055.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-054.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-053.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-052.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-051.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-050.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-049.md` is the earlier completed checkpoint;
`docs/plan/tasks/native-engine-048.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-047.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-046.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-045.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-044.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-043.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-042.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-041.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-040.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-039.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-038.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-037.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-036.md` is the preceding completed checkpoint,
`docs/plan/tasks/native-engine-035.md` is the preceding completed checkpoint,
`docs/plan/tasks/native-engine-034.md` is an earlier completed checkpoint,
`docs/plan/tasks/native-engine-032.md` is an earlier completed checkpoint,
`docs/plan/tasks/native-engine-031.md` is the earlier completed checkpoint,
and `docs/plan/tasks/native-engine-030.md` is the earlier selector checkpoint.
The completed current checkpoint is recorded for
`docs/plan/tasks/native-engine-101.md`: the design is `959cbbc9`, the
implementation is `8ff29aa1`, and the explicit word-spacing acceptance test is
`15cf0c85`. Focused/full native, feature-library, strict-Clippy, rustdoc,
binary, paired-package, dependency, fuzz, and documentation/release evidence
is recorded in the task file; exact isolated-target cleanup is recorded in the
final cleanup checkpoint. Remote CI remains pending because the branch is
local-only.
The completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-100.md`: the design is `2dae80fc` and the
implementation is `3380978c`. Focused/full native, feature-library,
strict-Clippy, rustdoc, binary, paired-package, dependency, and fuzz evidence
is recorded in that task file; static documentation/release validators and
exact isolated-target cleanup are completed in the final closeout. Remote CI
remains pending because the branch is local-only.
The completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-099.md`: the design is `2f18abb4` and the
implementation is `3bf658e8`. Focused/full native, feature-library,
strict-Clippy, rustdoc, binary, package/dependency, fuzz, documentation, and
directionality fallback evidence is recorded in that task file. Remote CI
remains pending because the branch is local-only.
The completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-098.md`: design is `a4b05f07`, implementation
is `77a4b629`, and the final test-only checkpoint is `866a8862`. Focused/full
native, feature-library, strict-Clippy, rustdoc, binary, package/dependency,
fuzz, documentation, and exact-target cleanup evidence is recorded in that
task file. The default-stack overflow in the pre-existing large-Clap parser
test remains isolated; the feature library suite passes with
`RUST_MIN_STACK=8388608`. Remote CI remains pending because the branch is
local-only.
A completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-097.md`: design is `2901c830`, implementation
is `815794ce`, and focused auto-margin, full-native, feature-library,
strict-Clippy, rustdoc, binary, validator, and exact-target-cleanup evidence
is recorded in that task file. The default-stack overflow in the pre-existing
large-Clap parser test remains isolated; the feature library suite passes with
`RUST_MIN_STACK=8388608`. Remote CI remains pending because the branch is
local-only.
A completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-096.md`: design is `f5026f3c`, implementation
is `6862aff6`, and focused column-wrap-reverse, full-native, feature-library,
strict-Clippy, fallback, documentation, package, fuzz, and exact-target
evidence is recorded in that task file. The default-stack overflow in the
pre-existing large-Clap parser test is reproduced with and without
`native-engine`; the feature library suite passes with
`RUST_MIN_STACK=33554432`. Remote CI remains pending because the branch is
local-only.
A completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-095.md`: design is `598228a7`, implementation
is `96ea62a3`, and focused column-wrap, full-native, feature-library,
strict-Clippy, fallback, and exact-target evidence is recorded in that task
file. The default-stack overflow in the pre-existing large-Clap parser test
is reproduced with and without `native-engine`; the feature library suite
passes with `RUST_MIN_STACK=33554432`. Remote CI remains pending because the
branch is local-only.
A completed preceding checkpoint is recorded for
`docs/plan/tasks/native-engine-094.md`: design is `aea47b17`, implementation
checkpoints are `7dc92517` and `4e212151`, and the focused parser/column,
full-native, feature-library, strict-Clippy, rustdoc, locked-binary,
documentation-validator, and exact-target-cleanup evidence is recorded in
that task file. The final local release-documentation audit reported 508
Markdown documents, 83 current documents, 57 previous-version hits, 568
semantic hits, and 0 current-claim failures; remote CI remains pending
because the branch is local-only.
A completed checkpoint is also recorded for
`docs/plan/tasks/native-engine-085.md`: design is `adc61a1f` and implementation
is `02f866e6`. Focused CSS parser/cascade coverage passed 2/2 in 6m57s;
focused shared-layout integration passed 1/1 in 20.68s; full native
integration passed 111/111; full native library coverage passed 887 with 1
ignored; strict all-feature and no-default-feature Clippy passed with
warnings denied; rustdoc and the locked `glass-dev` build passed; and all
repository documentation/reliability/adapter/Web IR validators passed. Exact
target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.
A checkpoint is complete only when
the native feature tests pass, strict lint
passes for the touched code, and the diff confirms no unrelated browser/TUI/
release behavior changed.
