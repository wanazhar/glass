# Native engine browser-complete expansion analysis

Status: Active implementation analysis for issue #40. Phase 0/1 and the first
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
The dependency-ordered `native-engine-136` implementation is complete in
`710ed3bb` and is recorded in
`docs/plan/tasks/native-engine-136.md`. It reuses the private component
candidates for standalone, case-insensitive `flex:revert-layer`, preserving
finite shorthand expansion, same-block longhand precedence, independent
component fallback, and existing layout/artifact consumers. Focused,
full-native, affected-library, and strict affected-package local gates passed;
exact evidence and cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered `native-engine-137` implementation is complete in
`7da4dfd5` and recorded in `docs/plan/tasks/native-engine-137.md` (design
`79e2d2fa`). It reuses the existing private direction/wrap and
align-content/justify-content candidates for standalone, case-insensitive
`flex-flow:revert-layer` and `place-content:revert-layer`, preserving finite
shorthand expansion, same-block longhand precedence, independent fallbacks,
and existing flex/artifact consumers. Focused parser/cascade, integration,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed; exact evidence and cleanup are recorded in the
task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-138` implementation is complete in
`bded96ae` (design `656dc37d`) and recorded in
`docs/plan/tasks/native-engine-138.md`. It reuses the existing private gap
precedence tuple for standalone, case-insensitive `gap:revert-layer`,
`row-gap:revert-layer`, and `column-gap:revert-layer` through component-local
row/column candidates. Finite integer-pixel expansion, same-block
shorthand/longhand precedence, independent zero fallback, and all current
flex layout/artifact consumers remain unchanged; percentages, other CSS-wide
keywords, multiple origins, layer statements, and browser-wide gap conformance
remain outside the boundary. Focused parser/cascade, integration, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed; exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered `native-engine-139` implementation is complete in
`0114659d` (design `16589ae4`) and recorded in
`docs/plan/tasks/native-engine-139.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on inherited
`text-transform`, `font-weight`, `font-style`, and `word-break`, keeping each
property's candidate sequence independent and the public values finite. The
existing fixed-cell transform/wrapping and bold/italic display/raster owners,
overflow/capture, hit testing, and semantic/source order remain unchanged.
Unicode case mapping, font metrics, other word-break modes, multiple origins,
and browser-wide text conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; remote CI remains pending because the
checkout is local-only.
The dependency-ordered `native-engine-140` implementation is complete in
`7d40cf87` (design `34f8ec1a`) and recorded in
`docs/plan/tasks/native-engine-140.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on inherited
`word-spacing` and `letter-spacing`, keeping each spacing candidate sequence
independent and the public values finite. Existing collapsed/preformatted
spacing, wrapping, alignment, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners remain the consumers. The shared
parser also preserves earlier valid inherited-text declarations when a later
declaration is invalid. Negative, relative, percentage, fractional,
cross-fragment, font-metric, multi-origin, and browser-wide text semantics
remain outside the boundary. Focused, full-native, affected-library, strict
Clippy, formatting, and static documentation gates passed locally; remote CI
remains pending because the checkout is local-only.
The dependency-ordered `native-engine-141` implementation is complete in
`271bfaf2` (design `0d1f7281`) and recorded in
`docs/plan/tasks/native-engine-141.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on inherited
`vertical-align`, keeping the public values finite and preserving parent/root
fallback plus the existing inline line-item, text-fragment, display-list,
raster, overflow, capture, hit-test, and semantic/source-order owners. Baseline
metrics, lengths, percentages, bidi, writing modes, multiple origins, and
browser-wide text conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-142` implementation is complete in
`be860447` (design `2794365f`) and recorded in
`docs/plan/tasks/native-engine-142.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on the local
`text-indent` and `text-overflow` owners, keeping finite non-negative
fixed-pixel indentation, `clip|ellipsis`, local `0px`/`clip` fallbacks, and the
existing first-line flow, eligible clipped-nowrap truncation, text-fragment,
display-list, raster, overflow, capture, hit-test, and semantic/source-order
owners unchanged. Negative or hanging indentation, percentages, font-relative
units, inherited text-overflow, marker customization, multiple origins, and
browser-wide text conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-143` implementation is complete in
`b55751da` (design `edc29d7c`) and recorded in
`docs/plan/tasks/native-engine-143.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on local `width`,
`height`, `min-width`, `max-width`, `min-height`, and `max-height`, keeping
finite non-negative pixel dimensions, absent local fallbacks, and the existing
box-model, normal-flow, flex, overflow, capture, hit-test, display-list,
raster, and semantic/source-order owners unchanged. Percentages, negative
dimensions, intrinsic sizing, aspect ratio, multiple origins, and browser-wide
CSS sizing conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered `native-engine-144` implementation is complete in
`7ce9c52c` (design `255a1ac8`) and is recorded in
`docs/plan/tasks/native-engine-144.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on local
`box-sizing`, physical padding and margin edges, including shorthand/longhand
rollback and bounded `margin:auto`, keeping independent edge ownership,
content-box/zero local fallbacks, and the existing box-model, normal-flow,
flex, overflow, capture, hit-test, display-list, raster, and semantic/source-
order consumers unchanged. Percentages, negative/logical edges, margin
collapsing, positioned or replaced-element sizing, multiple origins,
`!important` inversion, layer statements, and browser-wide box-model
conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered `native-engine-145` implementation is complete in
`997d4aa7` (design `553c5f89`) and is recorded in
`docs/plan/tasks/native-engine-145.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on local
`background-color` and inherited `color`, keeping independent paint-color
candidate ownership, `None`/inherited fallbacks, and the existing fill/text
display-list, capture, raster, clipping, opacity, hit-test, and
semantic/source-order consumers unchanged. Border-color, `currentColor`,
gradients, system colors, multiple origins, and browser-wide CSS color
conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote
CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-146` implementation is complete in
`462d2a70` (design `d1cd1eab`) and is recorded in
`docs/plan/tasks/native-engine-146.md`. It reuses the private bounded layer
resolver for standalone, case-insensitive `revert-layer` on local `overflow`,
`overflow-x`, and `overflow-y`, keeping independent x/y candidates, the
existing visible fallback, and the shared paint, viewport projection, point-hit,
root-overflow, capture, and semantic/source-order consumers unchanged. Nested
scrolling, scrollbars, `visible`/`auto`/`scroll` used-value parity, multiple
origins, and browser-wide CSS overflow conformance remain outside the boundary.
Focused, full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-147` implementation is complete in
`74cc1cf9` (design `f76720f7`) and recorded in the task. It reuses the same
private bounded layer resolver for standalone, case-insensitive `revert-layer`
on the local bounded one-to-four-value integer `border-radius` shorthand,
preserving the zero-corner fallback and the existing rounded fill, border,
point-hit, capture, raster, overflow, and semantic/source-order owners.
Elliptical, percentage, corner-longhand, nested-clip, anti-aliasing,
multiple-origin, and browser-wide border-radius conformance remain outside the
boundary. Focused, full-native, affected-library, strict Clippy, formatting,
and static documentation gates passed locally; exact evidence and cleanup are
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-148` implementation is complete in
`d3f89a6c` (design `d882d846`) and is recorded in
`docs/plan/tasks/native-engine-148.md`. It reuses the same private bounded
layer resolver for standalone, case-insensitive `revert-layer` on the existing
local 8-bit `opacity` owner, preserving the full-opacity fallback,
reduced-opacity group markers and software compositing, and the existing
layout, point-hit, capture, raster, overflow, and semantic/source-order
owners. Inherited opacity, stacking-context/blending parity, filters,
animation, multiple origins, and browser-wide opacity conformance remain
outside the boundary. Focused parser/cascade and integration tests, full-native
integration/library tests, strict affected-package Clippy, and formatting
passed locally. Static documentation gates passed with 562 Markdown documents,
83 current documents, 57 previous-version hits, 656 semantic-audit hits, and
0 current-claim failures. Documentation coverage passed with 562 Markdown
files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
public modules; depth passed with 93 guides and 19 substantive contracts;
parity passed for 14 capabilities across 4 targets; TUI passed at 15
implementation help keys/63 documentation markers; adapters passed at 5;
reliability passed at 6 scenarios across 4 targets; and Web IR passed at 8
fixtures/8 scenarios/11 categories. Remote CI remains pending because the
checkout is local-only.
The dependency-ordered `native-engine-149` implementation is complete in
`6a7dc305`, with the diagnostic compatibility fix in `3072f6e5` (design
`bd3b87b3`), and is recorded in `docs/plan/tasks/native-engine-149.md`. It
reuses the same private bounded layer resolver for standalone, case-insensitive
`revert-layer` on the existing local `display` and `visibility` owners,
preserving the normal-flow `display:auto`/visible fallbacks and the existing
hidden-subtree, normal-flow, point-hit, display-list, capture, raster, and
semantic/source-order owners. Inherited visibility, display decomposition,
formatting-context parity, table/ruby/flow-root details, animation, multiple
origins, and browser-wide CSS display/visibility conformance remain outside the
boundary. Focused, full-native, affected-library, strict Clippy, feature
rustdoc, formatting, and static documentation gates passed locally; exact
evidence and cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered `native-engine-150` implementation is complete in
`1fdbe75d` (design `27cf6c1a`) and is recorded in
`docs/plan/tasks/native-engine-150.md`. It reuses the same private bounded layer
resolver for standalone, case-insensitive `revert-layer` on the existing
physical `border`, `border-top`, `border-right`, `border-bottom`, and
`border-left` owners, preserving the zero-width/no-paint fallback and the
existing box-model inset, border display-list, capture, raster, point-hit, and
semantic/source-order owners. Logical sides, border-image, gradients, other
border styles, animation, multiple origins, and browser-wide CSS border
conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, feature rustdoc, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-151` implementation is complete in
`c26482b7` (design `4f23d85a`) and is recorded in
`docs/plan/tasks/native-engine-151.md`. It adds one-to-four-value physical
`border-color` and physical color longhands through an independent private
per-side color stream, including case-insensitive `revert-layer`, same-block
declaration order, and bounded black fallback, while retaining the existing
width/style, zero-width, box-model, border-artifact, capture, raster, point-hit,
and semantic/source-order owners. Standalone border-width/style, logical sides,
`currentColor`, gradients, border-image, animation, multiple origins,
`!important` inversion, and browser-wide CSS border conformance remain outside
the boundary. Local implementation and gates passed; exact evidence is
recorded in the task and remote CI remains pending because the checkout is
local-only.
The dependency-ordered `native-engine-152` implementation is complete in
`7dfcc7f5` (design `ff4d7803`) and is recorded in
`docs/plan/tasks/native-engine-152.md`. It adds one-to-four-value physical
`border-width` and physical width longhands through an independent private
per-side width stream, including case-insensitive `revert-layer`, while
retaining the existing border style/color, zero-width, box-model,
border-artifact, capture, raster, point-hit, and semantic/source-order owners.
Width-only declarations do not invent a style or paint a border. Standalone
border-style, logical sides, `currentColor`, gradients, border-image,
fractional/percentage widths, animation, multiple origins, `!important`
inversion, and browser-wide CSS border conformance remain outside the
boundary. Local implementation and gates passed; exact evidence is recorded
in the task and remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-153` implementation is complete in
`6169cabc` (design `77d81fdc`) and is recorded in
`docs/plan/tasks/native-engine-153.md`. It adds one-to-four-value physical
`border-style` and physical style longhands through an independent private
per-side style stream, including case-insensitive `revert-layer`, same-block
declaration order, and typed rejection of mixed or unsupported forms. Resolved
width, style, and color components compose only after independent resolution;
width-only or style-only declarations do not invent missing paint components.
Existing zero-width, box-model, border-artifact, capture, raster, point-hit,
and semantic/source-order owners remain unchanged. `none`, logical sides,
`currentColor`, gradients, border-image, unsupported styles, animation,
multiple origins, `!important` inversion, and browser-wide CSS border
conformance remain outside the boundary. Local implementation and gates passed;
exact evidence is recorded in the task and remote CI remains pending because
the checkout is local-only.
The dependency-ordered `native-engine-154` implementation is complete in
`875cdad8` (design `e7c9ad40`) and is recorded in
`docs/plan/tasks/native-engine-154.md`. It adds explicit physical
`border-style:none` to the bounded one-to-four-value shorthand and four
physical style longhands through a private no-paint sentinel. A winning
`none` blocks lower styles and converts to the existing no-side/zero-width
behavior before layout and artifacts, without changing the public paint enum
or display-list schema. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, formatting, static documentation, and bounded cleanup gates passed
locally; remote CI remains pending because the checkout is local-only. `hidden`,
other border styles, logical sides, `currentColor`, gradients, border-image,
and browser-wide border conformance remain outside the boundary.
The dependency-ordered `native-engine-155` implementation is complete in
`2b07f109` (design `37126fa1`) and is recorded in
`docs/plan/tasks/native-engine-155.md`. It adds explicit physical
`border-style:hidden` to the bounded one-to-four-value shorthand and four
physical style longhands through a distinct private no-paint sentinel. In the
current non-table engine, a winning `hidden` blocks lower styles and uses the
same no-side/zero-width result as `none`, while retaining a private
distinction for future collapsed-table conflict resolution. Public enums and
display-list schemas remain unchanged; table conflict resolution and other
border styles remain outside the boundary. Focused parser/cascade and artifact
integration tests, full-native integration/library tests, strict Clippy,
feature rustdoc, two-crate check/build, formatting, and static documentation
gates passed locally; exact evidence and bounded target cleanup are recorded
in the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered `native-engine-156` implementation is complete in the
current local checkpoint and is recorded in
`docs/plan/tasks/native-engine-156.md`. It batches the painted physical styles
`double`, `groove`, `ridge`, `inset`, and `outset` through the bounded style
parser, public computed paint enum, and deterministic software replay.
Integer-pixel double stripes and two-tone/edge-directed shading are explicit
native rules; the public display-list shape remains stable and no
browser-fidelity claim is made. Logical sides, table conflict resolution,
gradients, border images, and other general CSS border conformance remain
outside the boundary. Complete local evidence and exact cleanup are recorded
in the task; remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-157` slice is recorded in
`docs/plan/tasks/native-engine-157.md`; design is `ab9d6628` and implementation
is `fb2c56a2`. It accepts only exact case-insensitive omitted-component `none`
in the complete and physical border shorthands and routes it through a private
declaration wrapper into the existing style stream. Width and color do not
receive synthetic candidates, so a winning `none` blocks paint while a later
bounded `revert-layer` can expose an existing lower painted component.
Arbitrary omitted-component defaults, `border:hidden`, CSS-wide resets, table
conflict resolution, and browser-wide border conformance remain outside the
bounded boundary. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, formatting, and static documentation gates passed locally; exact
target cleanup is recorded in the task. Remote CI remains pending because the
branch is local-only.
The completed dependency-ordered `native-engine-158` slice is recorded in
`docs/plan/tasks/native-engine-158.md`; design is `6041a479` and implementation
is `f04623fc`. It accepts only exact case-insensitive omitted-component
`hidden` in the complete and physical border shorthands and routes it through
the existing private hidden style stream, preserving the private distinction
needed for future table conflict resolution and the current public/artifact
schemas. Width and color do not receive synthetic candidates, so a winning
`hidden` blocks paint while a later bounded `revert-layer` can expose an
existing lower painted component. Arbitrary omitted-component defaults,
CSS-wide resets, logical sides, table conflict resolution, and browser-wide
border conformance remain outside the bounded boundary. Focused/full-native/
library, strict Clippy, rustdoc, two-crate, formatting, and static
documentation gates passed locally; exact target cleanup is recorded in the
task. Remote CI remains pending because the branch is local-only.
The completed dependency-ordered `native-engine-159` slice is recorded in
`docs/plan/tasks/native-engine-159.md`; design is `bdbdb208` and implementation
is `329b3cfb`. It accepts bounded complete `Npx hidden color` values for the
complete and physical border shorthands, preserves declared width/color as
private component candidates, maps only style to the existing private hidden
sentinel, and keeps public/artifact schemas unchanged. Width and color cannot
resurrect a hidden side in current non-table composition, while a later
bounded `revert-layer` can expose a lower painted style with the retained
complete components. Arbitrary omitted defaults, CSS-wide resets, logical
sides, table conflict resolution, and browser-wide border conformance remain
outside the bounded boundary. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, formatting, and static documentation gates passed locally;
exact target cleanup is recorded in the task. Remote CI remains pending
because the branch is local-only.
The completed dependency-ordered `native-engine-160` slice is recorded in
`docs/plan/tasks/native-engine-160.md`; design is `bf1a7236` and implementation
is `a880c570`. It accepts bounded complete `Npx none color` values for the
complete and physical border shorthands, carries declared width/color through
private component candidates while projecting only the existing private
`NativeBorderStyleValue::None` sentinel, and keeps current non-table
composition no-paint/no-side with public/artifact schemas unchanged. Focused,
full-native/library, strict Clippy, rustdoc, two-crate, formatting, static
documentation, and bounded cleanup gates passed locally; remote CI remains
pending because the branch is local-only.
The completed dependency-ordered `native-engine-161` slice is recorded in
`docs/plan/tasks/native-engine-161.md`; design is `c5a58f29`, implementation is
`299de40d`, and the test-lint follow-up is `2042fb3e`. It adds standalone
physical `border-color` and `border-top|right|bottom|left-color` `currentColor`
substitution through a private color-value enum, resolving after the existing
local/inherited `color` owner and preserving the public `NativeColor` border
surface. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
formatting, static documentation, and bounded cleanup gates passed locally;
exact evidence is recorded in the task. Complete border shorthands with
`currentColor`, broader CSS color semantics, and all existing
non-table/browser-conformance exclusions remain outside the boundary. Remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-162` slice is recorded in
`docs/plan/tasks/native-engine-162.md`; design is `4272f0fa` and implementation
is `44b887c4`. It extends the private deferred-color path to complete physical
`Npx <style> currentColor` values for painted, `none`, and `hidden` styles,
projecting only concrete public border values. Omitted defaults, broader CSS
color semantics, and existing non-table/browser-conformance exclusions remain
outside the completed boundary. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates
are recorded in the task; remote CI remains pending because the checkout is
local-only.
The completed dependency-ordered `native-engine-163` slice is recorded in
`docs/plan/tasks/native-engine-163.md`; design is `764e0c86` and implementation
is `7a406855`. It extends the private deferred-color path to
`background-color: currentColor`, resolving against the element's local or
inherited `color` only at computed-style construction while projecting the
existing concrete public fill value. `color: currentColor`, gradients, images,
system colors, color spaces, percentages, CSS-wide reset machinery, and
browser-wide color conformance remain outside the completed boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, package,
formatting, static documentation, workspace all-target/all-feature, and bounded
cleanup gates passed locally; exact evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-164` slice is recorded in
`docs/plan/tasks/native-engine-164.md`; design is `e11821c5`, implementation is
`cceb61bf`, the diagnostics follow-up is `005083c3`, and synchronized product
documentation is `2960ecc5`. It adds case-insensitive local
`text-decoration-color: currentColor` through private deferred decoration
state, resolving against the existing local/inherited `color` owner before
projecting the public optional concrete decoration color. Separate glyph/line
paint, layer rollback, invalid-later handling, and all text artifact consumers
remain unchanged. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. `color: currentColor`, gradients, images,
system colors, color spaces, percentages, animations, multiple origins, and
browser-wide text-color conformance remain outside the completed boundary.
Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-165` slice is recorded in
`docs/plan/tasks/native-engine-165.md`; design is `75544c39`, implementation is
`f7b5fd4e`, and synchronized product documentation is `01438316`. It adds
local `color: currentColor` by resolving the self-reference from the
already-computed inherited color or bounded initial black fallback, keeping
the optional public color value and existing background/border/text consumers
unchanged. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
package, formatting, static documentation, workspace all-target/all-feature,
and bounded cleanup gates passed locally; exact evidence and cleanup are
recorded in the task. Gradients, images, system colors, color spaces,
percentages, custom-property graphs, CSS-wide reset machinery beyond existing
`revert-layer`, multiple origins, animation, and browser-wide color
conformance remain outside the completed boundary. Remote CI remains pending
because the checkout is local-only.
The completed dependency-ordered `native-engine-166` slice is recorded in
`docs/plan/tasks/native-engine-166.md`; design is `d148f766`, implementation is
`b57ba2b8`, and synchronized product documentation is `d26a9059`. It adds
bounded case-insensitive `inherit`, `unset`, `initial`, and one-author-origin
`revert` handling to inherited `color`, resolving inherited forms through the
bounded parent-color/black-root fallback and resetting `initial` to black while
preserving concrete public values and current paint consumers. Focused/full-
native/library, strict Clippy, rustdoc, two-crate, package, formatting, static
documentation, workspace all-target/all-feature, and bounded cleanup gates
passed locally; exact evidence is recorded in the task. Multiple origins,
`!important` inversion, gradients, images, system colors, color spaces,
percentages, custom-property graphs, animation, and browser-wide color
conformance remain outside the completed boundary. Remote CI remains pending
because the checkout is local-only.
The completed dependency-ordered `native-engine-167` slice is recorded in
`docs/plan/tasks/native-engine-167.md`; design is `9e143200`, implementation is
`1ae1f103`, and synchronized current product documentation is `debd6ab4`. It
adds exact case-insensitive `inherit`, `unset`, `initial`, and one-author-origin
`revert` handling to non-inherited `background-color`: only `inherit` copies
the parent's optional concrete fill, while reset forms and omission preserve
the existing no-fill `None` fallback. Focused/full-native/library, strict
Clippy, rustdoc, two-crate, package, formatting, static documentation,
workspace all-target/all-feature, and bounded cleanup gates passed locally;
exact evidence is recorded in the task. Multiple origins, `!important`
inversion, gradients, images, system colors, color spaces, percentages,
custom-property graphs, animation, and browser-wide background conformance
remain outside the completed boundary. Remote CI remains pending because the
checkout is local-only.
The completed dependency-ordered `native-engine-168` slice is recorded in
`docs/plan/tasks/native-engine-168.md`; design is `0bb67e8b`, implementation is
`4521f151`, and synchronized current product documentation is `2184d98`. It
adds exact case-insensitive `inherit`, `unset`, `initial`, and one-author-origin
`revert` handling to local `text-decoration-color`, with only explicit
`inherit` copying the parent's effective concrete decoration color and reset
forms resolving to the current element color. Omission remains the public
`None`/glyph-color fallback; `currentColor`, `revert-layer`, separate glyph/
line paint, and all text artifact owners remain unchanged. Focused/full-native/
library, strict Clippy, rustdoc, two-crate, package, formatting, static
documentation, workspace all-target/all-feature, and bounded cleanup gates
passed locally; exact evidence is recorded in the task. Multiple origins,
`!important` inversion, gradients, images, system colors, color spaces,
percentages, custom-property graphs, animation, and browser-wide
text-decoration conformance remain outside the completed boundary. Remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-169` slice is recorded in
`docs/plan/tasks/native-engine-169.md`; design is `b88177dc`, implementation is
`4d9e6979`, test-fixture corrections are `a0e77c84` and `801f7b19`, and
synchronized current product documentation is `07ba3dc4`. It adds bounded
case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert`
to local physical `border-color` and its four physical color longhands, with
only explicit `inherit` copying the parent's effective per-side color and
reset forms resolving to the current element color. Omitted colors retain the
black fallback; `currentColor`, `revert-layer`, independent side cascade,
border geometry, and all existing artifact owners remain unchanged. Focused/
full-native/library, strict, package, static, workspace, and bounded cleanup
gates passed locally. The first workspace replay exposed and the next replay
cleared one unrelated parallel extension-test `ETXTBSY` temporary-script race;
the plain `glass-dev` package verification remains a known registry API
mismatch while the patched local-release archive is exact. Remote CI remains
pending because the checkout is local-only.
The completed dependency-ordered `native-engine-171` slice is recorded in
`docs/plan/tasks/native-engine-171.md`; implementation is `67e04c0d`. It adds
the same bounded case-insensitive CSS-wide family to physical `border-style`
and its four style longhands, with only explicit `inherit` copying effective
parent side styles, including private `none`/`hidden` and styles from
unpainted or zero-width parents, and reset/omission retaining the private
no-style fallback. Existing `revert-layer`, width/color composition, border
geometry, and all artifact consumers remain unchanged. Focused/full-native/
library, strict, package, static, workspace, and bounded cleanup gates passed
locally. Mixed CSS-wide/style shorthand, logical sides, table conflict
resolution, multiple origins, and browser-wide border conformance remain
outside the completed boundary. Remote CI remains pending because the checkout
is local-only.
The completed dependency-ordered `native-engine-172` slice is recorded in
`docs/plan/tasks/native-engine-172.md`; implementation is `b0bbe45a`. It adds
the same bounded case-insensitive CSS-wide family to the physical
`border-radius` shorthand, copying only explicit effective parent radii while
reset and ordinary omission retain the default zero-corner fallback. Existing
bounded one-to-four-value expansion, `revert-layer`, rounded geometry, display
replay, raster, point-hit, capture, and semantic/source-order owners remain
unchanged. Focused/full-native/library tests pass locally; remaining
certification evidence is recorded in the task. Mixed CSS-wide/concrete or
slash-separated radii, corner longhands, elliptical and percentage radii,
multiple origins, and browser-wide border conformance remain outside the
completed boundary. Remote CI remains pending because the checkout is
local-only.
The completed dependency-ordered `native-engine-173` slice is recorded in
`docs/plan/tasks/native-engine-173.md`; implementation is `f5f53cec`. It adds
exact case-insensitive `inherit`, `unset`, `initial`, and one-author-origin
`revert` to the complete physical `border`, `border-top`, `border-bottom`,
`border-right`, and `border-left` shorthands by projecting private values into
the existing width/style/color candidate streams. Explicit `inherit` copies
effective parent side values; reset forms project zero-width, private `none`,
and `currentColor` so later component declarations can compose; ordinary
omission remains omission and mixed CSS-wide/concrete forms remain
unsupported. Existing concrete, omitted-component, `currentColor`,
`revert-layer`, geometry, display/raster, capture, hit, and semantic owners
remain unchanged. Focused/full-native/library, strict Clippy, warning-denied
rustdoc, paired-crate check/build, packaging, static documentation, and
workspace all-target/all-feature gates pass locally; exact evidence is
recorded in the task. Logical sides, table conflict resolution, multiple
origins, `!important` inversion, and browser-wide border conformance remain
outside the completed boundary. Remote CI remains pending because the checkout
is local-only.
The completed dependency-ordered `native-engine-174` slice is recorded in
`docs/plan/tasks/native-engine-174.md`; implementation is `f6953813`, with the
strict-cascade cleanup at `23b09864`. It adds bounded horizontal-tb logical
border shorthands and their width/style/color component families, mapping
block-start/end to physical top/bottom and inline-start/end through the
resolved ltr/rtl `direction` owner. Logical declarations project into the
existing physical per-side component streams so cascade, layer/source-order
precedence, `currentColor`, CSS-wide values, `revert-layer`, layout, display,
raster, capture, hit, and semantic owners remain shared. Vertical writing
modes, logical radius, border images, table conflict resolution, multiple
origins, and browser-wide logical-border conformance remain outside the
completed boundary. Focused/full-native/library, strict Clippy,
warning-denied rustdoc, paired-crate check/build, and formatting gates pass
locally; remaining package, static, workspace, and cleanup evidence is recorded
in the task. Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-175` slice is recorded in
`docs/plan/tasks/native-engine-175.md`; implementation is `2b082ddf`, with the
resolver/test-shape correction at `e6f3259d`. It adds four physical
`border-radius` corner longhands that project into private per-corner
candidates and reuse the existing shorthand/CSS-wide/`revert-layer` resolver
and rounded consumers. The completed contract is physical, horizontal-tb,
integer-pixel, and non-elliptical; logical corners, writing-mode mapping,
percentages, and browser corner fidelity remain outside it. Focused,
full-native/library, strict Clippy, warning-denied rustdoc, paired-crate,
packaging, static documentation, workspace all-target/all-feature,
security/fuzz, and formatting gates pass locally; exact evidence and bounded
cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.
The completed dependency-ordered `native-engine-176` slice is recorded in
`docs/plan/tasks/native-engine-176.md`; implementation is `6543b2b6`. Four
logical border-radius corner longhands project through resolved horizontal-tb
ltr/rtl direction into the existing physical per-corner candidate streams.
The completed contract is integer-pixel, non-elliptical, and local-resource-
only; vertical writing modes, text orientation, percentages, and browser
logical-radius fidelity remain outside it. Focused, full-native/library, strict
Clippy, warning-denied rustdoc, paired-crate, packaging, static documentation,
workspace all-target/all-feature, security/fuzz, and formatting gates pass
locally; exact evidence and bounded cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.
The preceding dependency-ordered `native-engine-177` through
`native-engine-197` slices are recorded in their task files. The preceding
chain's latest slice, `native-engine-197`, adds standalone case-insensitive `initial`, `unset`,
and one-author-origin `revert` to the six local dimension declarations
(`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`).
Winning reset candidates resolve to the existing optional `None`/auto fallback
without falling through; `revert-layer` remains the separate lower-layer
rollback candidate. Omission, explicit `inherit`, important/source-order,
invalid-later, min/max, content-box/border-box, normal-flow/flex,
display-list, raster, PNG-capture, point-hit, and semantic/source-order owners
remain bounded. Percentages, negative lengths, intrinsic sizing, aspect ratio,
margin collapsing, positioning, vertical writing modes, additional origins,
transitions, animations, and browser-wide sizing conformance remain outside
this slice. The preceding `native-engine-196` slice adds standalone
case-insensitive `inherit` to the same six local dimension declarations.
Explicit values copy the parent's computed optional pixel value through the
existing private DOM style walk, including a parent `None`/auto result; omitted
dimensions remain local and do not inherit. The existing important/source-order,
invalid-later, `revert-layer`, min/max, content-box/border-box, normal-flow/flex,
display-list, raster, PNG-capture, point-hit, and semantic/source-order owners
remain bounded. Percentages, negative lengths, intrinsic sizing, aspect ratio,
margin collapsing, positioning, vertical writing modes, additional origins,
transitions, animations, and browser-wide sizing conformance remain outside
that slice. The preceding `native-engine-195` slice adds standalone
case-insensitive `inherit` to
`box-sizing`, physical padding/margin shorthands and longhands, and the
supported horizontal-tb logical padding/margin family. Physical values copy the
parent's effective edges, box-sizing, and private margin `auto` provenance;
logical values read the parent side in its resolved `ltr`/`rtl` direction
before projecting into the child. Root fallbacks, omitted-property
non-inheritance, important/source-order behavior, and `revert-layer` rollback
remain bounded by the existing private cascade. Percentages, negative lengths,
margin collapsing, positioning, vertical writing modes, additional logical
properties, multiple origins, transitions, animations, and browser-wide CSS
conformance remain outside the bounded contract. The preceding
`native-engine-194` slice added standalone case-insensitive `initial`,
`unset`, and one-author-origin `revert` to the same box-model families.
These forms normalize to the existing content-box and zero-edge fallbacks;
`revert-layer` remains a separate rollback candidate and terminal
`!important` behavior remains unchanged. The preceding `native-engine-193`
slice added horizontal-tb logical padding/margin shorthands and block/inline
start/end longhands through resolved `ltr`/`rtl` direction and existing
physical per-edge candidate streams. The preceding
`native-engine-192` slice extended the bounded author-origin `!important`
partition from the radius, paint-color, border, text, local-presentation,
flex/gap, dimension, and physical box-model families to the normal-only
`overflow`, `overflow-x`, and `overflow-y` declarations through private doubled
x/y candidate streams and shorthand/x/y importance bits. Important-over-normal
ordering, reversed named-layer priority, inline precedence in the unlayered
important bucket, invalid-later preservation, independent axis projection, and
`revert-layer !important` rollback are covered without changing public schemas,
artifact owners, dependencies, defaults, or the two-crate boundary. Nested
scrolling, visible/auto/scroll used-value parity, logical writing modes,
multiple origins, transitions, animations, and browser-wide CSS conformance
remain outside the bounded contract.
Local certification for the latest slice is recorded below; the checkout
remains local-only.

The completed dependency-ordered `native-engine-198` slice is recorded in
`docs/plan/tasks/native-engine-198.md` and implemented at `95a988d0` (design
`927cccca`). It adds standalone case-insensitive `inherit`, `initial`, `unset`,
and one-author-origin `revert` to inherited `white-space`, positive-pixel
`line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
`vertical-align`, `word-spacing`, and `letter-spacing`. Parent/root fallback,
terminal reset behavior, invalid-later preservation, existing cascade priority,
and `revert-layer` distinction reuse the existing private cascade and
computed-style/artifact owners. The fixed-cell public schemas and two-crate
boundary remain unchanged. Focused and full native integration (236/236), the
feature library (1019 passed, 1 ignored with `RUST_MIN_STACK=8388608`), strict
Clippy, warning-denied rustdoc, locked scoped check, and formatting pass
locally. Static release truth passes with 612 Markdown documents (83 current,
57 previous-version hits, 715 semantic audit hits, 0 current-claim failures);
coverage, depth, parity, TUI, and version-sync also pass with 612 Markdown
files, 345 full-product MCP tools (100 browser-only), 17 examples, 22 public
modules, 93 current guides, 19 substantive contracts, 14 capabilities across
4 targets, 15 implementation help keys, 63 documentation markers, and version
`0.3.14`. The default-stack library run reproduces the pre-existing large-Clap
parser-test overflow. Paired-crate, package, workspace, security/fuzz,
cleanup, issue-level, and remote-CI gates remain pending until issue #40
reaches its final validation boundary.

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
DOM/interaction slices, initial Phase 3 presentation slices, and a bounded
HTTP(S) document-navigation slice. Those slices are the foundation of the
expanded browser-complete program; they do not yet claim general external-web
compatibility or CDP replacement.

## Browser-complete expansion analysis

The expanded target is a Glass Core Web Profile that can run ordinary
external web pages and all Glass-supported automation operations through the
native backend alone. The profile is versioned rather than pretending that
“complete browser” is an infinite, moving target. Its conformance anchors are
the WHATWG HTML and Fetch standards, the stable CSS profile, ECMAScript/Web
IDL, and a pinned Web Platform Tests manifest. The implementation may use
mature lower-level components, but it must not delegate browser ownership to
Chromium/CDP, Gecko, WebKit, or a remote browser.

The critical path is intentionally ordered around dependencies:

1. runtime substrate: process model, IPC, cancellation, task/microtask
   scheduling, crash recovery, quotas, and deterministic observability;
2. URL/network/origin security: HTTP(S), TLS, redirects, MIME/charset, cache,
   cookies, CORS, CSP, mixed content, service workers, permissions, and
   site/process isolation;
3. standards HTML/DOM: tokenizer/tree builder and error recovery, document
   lifecycle, DOM mutation/selection/ranges, events, forms, focus, shadow DOM,
   custom elements, frames, and the accessibility tree;
4. ECMAScript and Web IDL: realms, bindings, promises, timers, modules,
   structured clone, fetch/XHR, workers, and the DOM API surface;
5. CSS/style/layout: tokenization, cascade, selectors, inheritance, custom
   properties, media/container queries, block/inline/flex/grid/table/position,
   overflow/scrolling, writing modes/bidi, animation, and fragmentation;
6. rendering/compositing: fonts/shaping, images/SVG/canvas, clipping,
   transforms, filters, layers, hit testing, software/headless/GPU surfaces,
   and deterministic screenshot/print output;
7. browser primitives: tabs/windows, frames/popups, opener/history topology,
   keyboard/pointer/touch/IME/selection/drag/drop/clipboard, uploads,
   downloads, dialogs, permissions, profiles, storage, and recovery;
8. Glass parity and promotion: every normal BrowserBackend/CLI/MCP/TUI path,
   cross-platform packaging, real-site compatibility, WPT, security, and
   performance gates.

JavaScript and network are prerequisites for useful external browsing; CSS
micro-slices alone cannot make the engine a browser. Validation is therefore
batched at completed behavioral boundaries. WPT, differential runs, and the
real-site corpus are milestone gates, not commands to run after every edit.

The checked-in profile contract is
[`GCWP-0.1`](../native-engine-browser-profile.md), with its M0 delivery record
in [`native-engine-browser-000`](../tasks/native-engine-browser-000.md). The
next implementation batch must advance one declared profile family and retain
the profile's native-only, no-silent-fallback, two-crate, and security-boundary
constraints.

The first executable BE-01 batch is
[`native-engine-browser-001`](../tasks/native-engine-browser-001.md). It
introduces a `NativeRuntime` owner around the existing deterministic scheduler,
with typed microtasks, one-shot cancellation, bounded privacy-safe traces,
startup rollback, and terminal close. It deliberately stops before network,
JavaScript, asynchronous Web APIs, IPC, or process isolation.

The next executable network batch is
[`native-engine-browser-002`](../tasks/native-engine-browser-002.md). It adds a
bounded asynchronous HTTP(S) HTML-document loader, redirect and response-size
limits, HTML MIME checks, UTF-8 decoding, normalized tuple origins, and native
backend integration coverage. It deliberately stops before subresources,
charset sniffing, cookies/cache, CORS/CSP, JavaScript, process isolation, and
browser parity.

The runtime follow-up is
[`native-engine-browser-003`](../tasks/native-engine-browser-003.md). It adds a
bounded typed Tokio worker over the single runtime state and routes
asynchronous initialization/navigation commits through that worker. It is an
in-process BE-01b slice only; process isolation, supervisor restart, and the
remaining hostile-content gates are still required.

The completed process-control follow-up is
[`native-engine-browser-004`](../tasks/native-engine-browser-004.md). It keeps
the two-crate boundary while adding the `glass-native-content-worker` helper
binary and a bounded request-ID-correlated length-framed IPC protocol. Native
external initialization/navigation requires a live helper and explicit
ping/start/commit acknowledgements; backend shutdown uses an explicit close
acknowledgement with a bounded wait. The parent still owns HTTP(S) fetching,
decoding, and document construction, so this is not yet content-process
execution isolation or hostile-content safety.

The completed resource-transfer follow-up is
[`native-engine-browser-005`](../tasks/native-engine-browser-005.md). The child
now invokes the shared bounded HTTP(S) loader, parses the HTML tree, and
transfers only final URL and a base64-encoded typed DOM snapshot under bounded
frame/document quotas. The parent validates the transfer and origin before
reconstructing the arena; load deadlines and malformed responses poison the
child, while ordinary child-side HTTP rejections remain typed failures.
Stylesheet parsing, computed style, and DOM mutation remain parent-owned, so
the next process gate must move those owners and their recovery boundary into
the child.

The completed computed-style follow-up is
[`native-engine-browser-006`](../tasks/native-engine-browser-006.md). The child
now serializes one typed computed-style record per parsed node; the parent
validates the bounded record set and uses it for external layout/visibility
without reparsing stylesheet sources. Local/data/fixture documents retain the
direct stylesheet owner.

The completed mutation-ownership follow-up is
[`native-engine-browser-007`](../tasks/native-engine-browser-007.md). The child
retains the external document, applies bounded click/type control mutations on
a transactional clone, and returns a fresh DOM/style snapshot with typed
privacy-safe effects. The parent validates the returned node indices and
publishes one revision/effect batch; a five-second mutation deadline,
malformed transfer, or worker rejection poisons the child without success or
silent fallback. Scroll and link navigation remain parent-owned handoffs.
CSS diagnostic transfer, script/event-loop execution, sandboxing, supervisor
recovery, and browser parity remain open.

The completed recovery follow-up is
[`native-engine-browser-008`](../tasks/native-engine-browser-008.md). The
content channel now classifies spawn, exit, transport, timeout, protocol,
rejection, and invalid-transfer failures. A failed mutation cannot be retried
through another owner; external navigation is the deliberate fresh-worker
recovery boundary, while shutdown remains idempotent after child exit. The
remaining BE-01 gate is OS-specific sandboxing and cross-platform process
containment; diagnostics transfer, script/event-loop execution, network
security, and browser parity remain open.

The completed sandbox-launch follow-up is
[`native-engine-browser-009`](../tasks/native-engine-browser-009.md). Linux
requires Bubblewrap with user/PID/UTS/IPC isolation, read-only runtime mounts,
private temporary storage, parent-death cleanup, and `no_new_privs`; macOS
uses a deny-by-default Seatbelt profile; Windows uses an attached Job Object
with active-process and kill-on-close limits. Sandbox construction failure is
typed and fail-closed. The shared network namespace, origin/site policy,
restricted Windows tokens, and full cross-platform containment evidence remain
open for BE-02 and later security gates.

The completed redirect/charset follow-up is
[`native-engine-browser-010`](../tasks/native-engine-browser-010.md). The
shared loader rejects credential-bearing and non-HTTP(S) redirects before
follow, caps the chain at eight hops, rebuilds final tuple origins, and
supports bounded UTF-8, UTF-16, Latin-1, and Windows-1252 document decoding.
The completed stateful network follow-up is
[native-engine-browser-011](../tasks/native-engine-browser-011.md). The
content process now retains bounded session-only cookies and a bounded
in-memory document cache across navigations, with domain/path/secure matching,
explicit cache safety denials, and no persistence or sensitive-data logging.
Full cache freshness/revalidation, origin/referrer policy, CORS/CSP, mixed
content, service workers, permissions, subresources, complete WHATWG encoding
sniffing, and script/browser parity remain open.

The completed origin/referrer follow-up is
[native-engine-browser-012](../tasks/native-engine-browser-012.md). Native
top-level HTTP(S) navigation now derives a strict-origin-when-cross-origin
referrer from the committed URL, recomputes it for each manually validated
redirect, and keeps redirect cookies pending until final document validation.
Same-origin full URLs, cross-origin origin-only referrers, downgrade
suppression, and child-wire validation are covered. CORS/CSP, mixed content,
service workers, permissions, subresources, full cache semantics, script
request mediation, and browser parity remain open.

The completed stylesheet-subresource follow-up is
[native-engine-browser-013](../tasks/native-engine-browser-013.md). The child
now discovers a bounded set of link stylesheets, mediates them with the
document CSP and HTTPS mixed-content policy, fetches validated text/css
resources under the shared cookie/redirect/referrer quotas, and reparses
accepted rules before the child snapshot transfer. Images, media, fonts,
scripts, fetch/XHR, service workers, permissions, broad CORS/CSP, and browser
parity remain open.

The completed network-policy foundation is
[native-engine-browser-014](../tasks/native-engine-browser-014.md). A typed
resource-family vocabulary now selects `default-src` fallback and the
directive lists for style, script, image, font, media, frame, connect, and
worker candidates. Shared HTTP(S) subresource resolution rejects credentials
and unsupported schemes, one mixed-content helper blocks HTTPS-to-HTTP
subresources at every stylesheet redirect hop, and a private CORS helper
produces cross-origin `Origin` values and validates wildcard/exact and
credentialed response authorization. The stylesheet caller is migrated to
the shared URL/CSP/mixed-content helpers. No script/fetch/XHR or other
resource caller exists yet, so the remaining BE-02e gate is executable
request mediation rather than policy-only coverage.

The completed child-fetch follow-up is
[native-engine-browser-015](../tasks/native-engine-browser-015.md). The
existing content worker now owns a bounded GET-only fetch caller from the
current external document. It applies the connect CSP family, shared URL and
HTTPS mixed-content policy, eight-hop redirects, explicit credential cookie
use, and CORS `Origin`/ACAO authorization before transferring a capped typed
response. It returns HTTP status/content-type metadata without changing the
document revision. This is an executable network primitive, not a Fetch/Web
IDL implementation: custom request shapes, preflights, streams, service
workers, script execution, and remaining resource classes remain open.

The completed JavaScript-realm follow-up is
[native-engine-browser-016](../tasks/native-engine-browser-016.md). The native
backend now dispatches bounded script requests into a persistent QuickJS
realm. Local documents keep the realm in the engine owner; external documents
keep it in the sandboxed content process, and full navigation resets it.
`JSON.stringify` provides a bounded JSON result with a 16 KiB source cap,
64 KiB result cap, 32 MiB runtime memory cap, 1 MiB stack cap, and five-second
interrupt deadline. Script exceptions do not poison a healthy worker, while
IPC failures and deadlines retain typed process recovery behavior. The
following host-view follow-up is complete.

The completed JavaScript host-view follow-up is
[native-engine-browser-017](../tasks/native-engine-browser-017.md). Each
evaluation refreshes the same bounded read-only `window`/`document` snapshot
for local and child-owned pages, including explicit element finders,
location/origin, viewport, title/text, and form state. Synchronous completion
values remain direct JSON; top-level `await` drives bounded QuickJS jobs before
the result is converted. Live Web IDL identity, mutation, event dispatch,
timers, modules, page-script loading, Fetch/XHR, service workers, and browser
compatibility remain open.

The completed JavaScript DOM-mutation follow-up is
[native-engine-browser-018](../tasks/native-engine-browser-018.md). The host
projection now emits bounded typed `click()`, form-state, and attribute
commands. Glass validates the complete batch against a document clone and
commits one revision; the child process applies the same commands before its
typed snapshot/effects transfer. Live Web IDL identity, ancestor event
propagation, Rust-action listener dispatch, script navigation, timers, modules,
page-script loading, Fetch/XHR, service workers, and browser compatibility
remain open.

The completed JavaScript event/focus follow-up is
[native-engine-browser-019](../tasks/native-engine-browser-019.md). Elements,
`document`, and `window` retain deduplicated target-local listeners across
evaluations; bounded `Event` and `CustomEvent` values support synchronous
dispatch and cancellation. Scripted focus/blur transitions emit typed
commands, and scripted click activation is suppressed when a target listener
calls `preventDefault()`. Parent and child owners validate and commit focus
transitions with the same one-revision batch contract. Ancestor propagation,
listeners for Rust semantic actions, default-action ordering, mutation
invalidation, timers, modules, page-script loading, Fetch/XHR, service workers,
and remaining resource classes stay open.

The completed JavaScript event-graph follow-up is
[native-engine-browser-020](../tasks/native-engine-browser-020.md). The host
snapshot now carries nearest-element parent indices and non-enumerable
`parentElement`/`parentNode` links. Listener records retain capture and once
options; dispatch walks a bounded window/document/ancestor path through
capture, target, and bubble phases with propagation controls. Rust semantic
actions still do not re-enter the page realm.

The completed Rust-action event bridge is
[native-engine-browser-021](../tasks/native-engine-browser-021.md). After a
Rust semantic action commits, bounded event metadata is converted to internal
host-event source and delivered to the existing local realm or sandboxed child
realm. Callback commands return through the existing clone/typed-transfer
owner path, with no executable callback ABI or parent-side evaluation of
external pages. The action and callback mutation currently use separate
revisions; callback errors cannot undo the action, and `preventDefault()` does
not yet suppress it. The transactional click preflight is the next refinement.

The completed cancelable-click follow-up is
[native-engine-browser-022](../tasks/native-engine-browser-022.md). When a
JavaScript realm exists, local semantic clicks and external child clicks now
run focus and click listeners against a clone, apply callback commands, honor
`preventDefault()`, and commit one final document/effect revision. A child
uses one typed preflight request and transfers only its final snapshot/effects;
the parent never evaluates the external page. Pages without a local realm keep
the Rust-only click path. Type/input/change ordering, link navigation/default
actions, timers, modules, Fetch/XHR, and remaining resource classes stay open.

The completed transactional type-event follow-up is
[native-engine-browser-023](../tasks/native-engine-browser-023.md). Local and
child type actions now update a clone, dispatch focus/blur, input, and change
in order, apply callback commands, and publish one revision. The bootstrap's
active command sink also lets persistent callbacks use previously captured
element methods without writing to a detached evaluation buffer. This is not
full live Web IDL identity: snapshot properties/tree links remain refresh
bound, and insertion/removal, `beforeinput`, composition, full selection and
keyboard lifecycle, submission, link navigation/default actions, timers,
modules, Fetch/XHR, and remaining resource classes stay open.

The completed script-navigation follow-up is
[native-engine-browser-024](../tasks/native-engine-browser-024.md). Top-level
script link clicks now emit a validated navigation handoff: local documents
reuse the existing resource/history owner, while external documents transfer
the request from the child and the parent performs the next child-owned load.
Full navigation resets the realm and same-document navigation retains it. The
click and navigation commits currently use separate revisions; target contexts,
form submission, unload/navigation task ordering, timers, modules, Fetch/XHR,
and remaining resource classes stay open.

The completed relative-URL/history follow-up is
[native-engine-browser-025](../tasks/native-engine-browser-025.md). Relative
and root-relative HTTP(S) link references now resolve against the current
document. Local and external fragment navigation uses the same-document
history/revision owner and retains the page realm without a redundant load;
external non-fragment links remain child-owned. Target contexts, form
submission, lifecycle/default-action ordering, timers, modules, Fetch/XHR, and
remaining resource classes stay open.

The completed inline-page-script follow-up is
[native-engine-browser-026](../tasks/native-engine-browser-026.md). Local
prepared navigations and child-owned HTTP(S) loads now discover and execute a
bounded set of inline JavaScript sources in the realm that owns the document.
The parsed document receives only validated typed commands, local failures do
not publish a partial navigation, and child globals/listeners persist without
transferring executable state over IPC. External `src` scripts, modules,
parser timing, timers, Fetch/XHR, CSP script enforcement, and full Web IDL
identity remain open.

The completed classic external-script follow-up is
[native-engine-browser-027](../tasks/native-engine-browser-027.md). HTTP(S)
content loads now resolve accepted classic `src` scripts in DOM order through
the existing child resource-policy owner and execute them in the same realm as
inline sources. Script CSP/default-src, mixed content, redirects,
referrer/cookies, MIME, and byte quotas are enforced before execution;
module/unknown types are not requested. Local subresource ownership, module
graphs, parser timing, timers, Fetch/XHR, and full Web IDL identity remain
open.

The completed bounded form follow-up is
[native-engine-browser-028](../tasks/native-engine-browser-028.md). Local and
child-owned realms now emit typed form submission commands, encode bounded
named controls for GET, and route both explicit `submit()` calls and submit
button clicks through the existing local/history or parent/content navigation
owner. POST/multipart, validation and submit events, target contexts, module
timing, timers, Fetch/XHR, and full Web IDL identity remain open.

The completed bounded module-root follow-up is
[native-engine-browser-029](../tasks/native-engine-browser-029.md). Local and
HTTP(S) documents now classify inline and external `type="module"` roots and
execute them through QuickJS's module evaluator in document order. External
module source retains its validated final URL, and both local and child-owned
realms keep the resulting globals/listeners while typed commands remain
clone-and-commit operations. Static import graphs, dynamic `import()`, local
external subresources, parser timing, and full Web IDL identity remain open.

The completed bounded static-module-graph follow-up is
[native-engine-browser-030](../tasks/native-engine-browser-030.md). The child
prefetches relative and absolute HTTP(S) static imports/exports through the
owner-document script policy, bounds duplicate/cyclic graph entries and total
bytes, and installs only validated final-URL/source pairs into QuickJS's
in-memory loader. Bare specifiers, import maps, dynamic `import()`, parser
timing, local external module subresources, and full Web IDL identity remain
open.

The completed bounded literal-dynamic-import follow-up is
[native-engine-browser-031](../tasks/native-engine-browser-031.md). Literal
`import("...")` calls reuse the admitted module graph and a bounded QuickJS
pending-job drain, preserving module namespace/export resolution and promise
callback effects. Computed specifiers, bare packages/import maps, parser
timing, non-HTTP(S) modules, and full Web IDL identity remain open.

The completed bounded task-turn follow-up is
[native-engine-browser-032](../tasks/native-engine-browser-032.md). The local
and child-owned realms expose bounded `queueMicrotask` and next-host-turn
`setTimeout` semantics, with pending QuickJS jobs drained after evaluation and
event callback turns. Wall-clock scheduling, intervals, animation/idle
callbacks, parser timing, and full browser task-source ordering remain open.

The completed bounded keyboard-input follow-up is
[native-engine-browser-033](../tasks/native-engine-browser-033.md). Local and
child-owned `KeyPress` actions now edit the focused text control for printable
keys and bounded Backspace/Delete behavior, dispatch cancelable keydown plus
input/keyup callbacks with key metadata, and refresh retained host wrappers
before callbacks. Selection/caret ranges, IME/composition, navigation keys,
beforeinput, modifier shortcuts, and form-submit defaults remain open.

The completed bounded GET-form lifecycle follow-up is
[native-engine-browser-034](../tasks/native-engine-browser-034.md). Local and
child owners now distinguish direct `form.submit()` from `requestSubmit()`,
dispatch a cancelable bubbling `submit` event before GET query serialization,
retain listener mutations, and transfer semantic submit-button navigation
through the existing owner path.

The completed bounded urlencoded-POST follow-up is
[native-engine-browser-035](../tasks/native-engine-browser-035.md). Local and
child owners now serialize accepted `method="post"` forms into one bounded
`application/x-www-form-urlencoded` body and send it through the shared
navigation loader and content-process IPC. Existing submit cancellation,
cookie/referrer/redirect policy, response parsing, and no-POST-cache behavior
remain in force; unsupported methods and multipart/text/plain encodings fail
explicitly. Full constraint validation, submitter serialization, target
contexts, multipart bodies, and unload/navigation task ordering remain open.

The completed bounded parser-time script-ordering follow-up is
[native-engine-browser-036](../tasks/native-engine-browser-036.md). DOM script
discovery now retains parser-blocking, external-async, and deferred/module
timing metadata; both local and child owners execute those buckets through one
deterministic ordering helper. Incremental parsing, wall-clock completion
races, script lifecycle events, dynamic insertion, and full task-source timing
remain open.

The completed bounded page-lifecycle follow-up is
[native-engine-browser-037](../tasks/native-engine-browser-037.md). After the
accepted script schedule, both owners dispatch `DOMContentLoaded` on the
document and then `load` on the window through the typed event bridge, so
listener mutations remain inside the existing clone-and-commit path. Ready
state transitions, resource-specific events, unload/pagehide, completion
races, dynamic insertion, and full task-source timing remain open.

The completed bounded form-validation follow-up is
[native-engine-browser-038](../tasks/native-engine-browser-038.md). Both
owners now validate the bounded required-control set before interactive
submission, dispatch non-bubbling `invalid` events for blocked controls, and
expose the initiating button as `event.submitter` for valid submit callbacks.
Direct `form.submit()` remains validation-free. Full constraint-validation
APIs, multipart encoding, and target contexts remain open; bounded submitter
serialization and external form association are covered by later slices.

The completed bounded ready-state lifecycle follow-up is
[native-engine-browser-039](../tasks/native-engine-browser-039.md). Both
owners now expose `loading`, `interactive`, and `complete` at deterministic
script/lifecycle boundaries, dispatch document `readystatechange` at the
interactive and complete transitions, and preserve the order
`DOMContentLoaded` then window `load`. Pages without scripts still retain a
persistent realm with final `document.readyState === "complete"`. Resource
specific completion events, wall-clock races, incremental parsing,
unload/pagehide/pageshow, and full task-source timing remain open.

The completed bounded submitter-serialization follow-up is
[native-engine-browser-040](../tasks/native-engine-browser-040.md). Local and
child owners now carry the initiating submit control through bounded GET and
urlencoded-POST serialization, validate the typed handoff at the parent, and
honor form `novalidate`/submitter `formnovalidate` without suppressing normal
submit events. Image coordinates, target contexts,
multipart/text/plain, full constraint validation, and FormData/Web IDL parity
remain open.

The completed bounded resource-lifecycle follow-up is
[native-engine-browser-041](../tasks/native-engine-browser-041.md). The
process-backed owner dispatches successful external stylesheet/script `load`
events at a deterministic post-resource boundary before `DOMContentLoaded`,
retaining callback mutations through the existing typed realm transfer.
Failure/error events, dynamic insertion, resource timing, network concurrency,
and full task-source ordering remain separate work.

The completed bounded replacement-navigation follow-up is
[native-engine-browser-042](../tasks/native-engine-browser-042.md). Full
replacement navigations deliver window `pagehide` then `unload` before
resource replacement and `pageshow` after new-page publication in both local
and child owners. Effects and callback mutations use the existing typed
boundary; same-document fragments remain in-place. Cancelable `beforeunload`,
bfcache/history traversal, popup/opener contexts, visibility, and full HTML
navigation task ordering remain open.

The completed bounded same-document navigation follow-up is
[native-engine-browser-043](../tasks/native-engine-browser-043.md). GET
fragment changes retain the current document/realm, avoid reload, update the
URL owner, and dispatch window `hashchange` with `oldURL`/`newURL` in local and
child paths. `beforeunload`, `popstate`, bfcache/history lifecycle parity, and
full HTML navigation task ordering remain open.

The completed bounded external form-ownership follow-up is
[native-engine-browser-044](../tasks/native-engine-browser-044.md). Controls
with an explicit `form="id"` now associate with the matching form outside
ancestor traversal, unresolved references do not fall back, and local/child
validation plus GET/urlencoded-POST serialization preserve document order.
External submit buttons use the same typed `requestSubmit(button)` handoff.
Multipart/text/plain, full constraint validation, target contexts, and the
remaining browser-context primitives remain open.

The completed bounded POST-encoding follow-up is
[native-engine-browser-045](../tasks/native-engine-browser-045.md). POST forms
now carry bounded `multipart/form-data` text fields and `text/plain` bodies
through parent/content-process IPC with explicit content types; redirects
clear content type when POST becomes GET. File parts, FormData/Web IDL
identity, full constraint validation, target contexts, and the remaining
browser-context primitives remain open.

The completed bounded navigation-cancellation/history-event follow-up is
[native-engine-browser-046](../tasks/native-engine-browser-046.md). Replacement
navigation now dispatches cancelable window `beforeunload` before
`pagehide`/`unload` and resource loading, honors `preventDefault()` and
non-empty `returnValue`, and keeps the current page when canceled. Same-
document history traversal dispatches window `popstate` before `hashchange` in
the local and child owners. Prompts, bfcache/session-history parity,
cross-document restoration, target contexts, and full HTML task-source
semantics remain open.

The completed bounded due-time timer-turn follow-up is
[native-engine-browser-047](../tasks/native-engine-browser-047.md). Local and
child realms now retain normalized `setTimeout` due times, drain only due
callbacks on a later host turn in due-time/ID order, and honor `clearTimeout`.
There is no background page event loop yet; intervals, animation/idle
callbacks, task-source fairness, and full wall-clock scheduling remain open.

The completed bounded submitter-override follow-up is
[native-engine-browser-048](../tasks/native-engine-browser-048.md). Local and
child-owned script submissions now apply validated `formaction`, `formmethod`,
and `formenctype` overrides before request construction, including the
effective POST content type across the process boundary. Form target contexts,
dialog submission, file parts, and general form-control/Web IDL identity remain
open.

The completed bounded repeating-timer follow-up is
[native-engine-browser-049](../tasks/native-engine-browser-049.md). Local and
child-owned realms now expose `setInterval`/`clearInterval`; due callbacks run
at most once per supplied host turn, reschedule from that turn's monotonic
time, and can cancel themselves. There is no background page loop or
task-source fairness; animation and idle callbacks remain open.

The completed bounded common-constraint follow-up is
[native-engine-browser-050](../tasks/native-engine-browser-050.md). Local and
child-owned forms now validate required, email/URL, UTF-16 length, and numeric
min/max/step constraints before ordered `invalid` events and submit handoff.
Pattern/file constraints and full `ValidityState` Web IDL identity remain open;
the bounded validation API is covered by 059.

The completed bounded script-fetch follow-up is
[native-engine-browser-051](../tasks/native-engine-browser-051.md). Explicit
evaluations in process-backed HTTP(S) documents can issue policy-owned GET
`fetch()` requests and resolve bounded text/JSON promises, including typed DOM
callback mutations. Page-load fetch scheduling, non-GET uploads, XHR/WebSocket,
and full Fetch Web IDL identity remain open.

The completed bounded page-load fetch follow-up is
[native-engine-browser-052](../tasks/native-engine-browser-052.md). Fetches from
initial page scripts and lifecycle evaluation now settle before the child
publishes its first document snapshot, including typed callback DOM mutations.
Callback navigation during initial publication, non-GET uploads, XHR/WebSocket,
and full Fetch Web IDL identity remain open.

The completed bounded same-origin POST fetch follow-up is
[native-engine-browser-053](../tasks/native-engine-browser-053.md). Explicit
and initial page scripts can issue bounded string-body POST requests with an
optional `Content-Type`, while the existing document-policy, cookie, referrer,
redirect, response-size, and CORS limits remain in force. Redirects rewrite
POST to GET for 301/302/303 and retain POST data for 307/308. Cross-origin
preflight/simple-POST coverage, custom headers, multipart/FormData/blob/stream
bodies, AbortController, XHR/WebSocket, and full Fetch Web IDL identity remain
open.

The completed bounded CORS preflight follow-up is
[native-engine-browser-054](../tasks/native-engine-browser-054.md). Cross-origin
simple POSTs use the direct Origin/response-CORS path; non-simple POSTs now
perform a bounded OPTIONS preflight and fail closed unless the origin, method,
and supported `content-type` request header are authorized. Preflight cache,
custom headers, private-network access, opaque `no-cors` responses,
multipart/FormData/blob/stream bodies, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed bounded XHR follow-up is
[native-engine-browser-055](../tasks/native-engine-browser-055.md). The
persistent page realm exposes asynchronous GET/POST `XMLHttpRequest` with
string bodies, the supported `Content-Type` header, bounded response
status/text/URL/header access, and `readystatechange`/`load`/`error` callbacks
through the existing fetch/CORS command path. Synchronous XHR,
upload/progress, binary response types, timeout/abort, streaming,
WebSocket/EventSource, and full Web IDL identity remain open.

The completed bounded text FormData follow-up is
[native-engine-browser-056](../tasks/native-engine-browser-056.md). The page
realm now serializes string-only `FormData` through the existing fetch/XHR
owner with deterministic bounded multipart boundaries and matching content
types. File/blob parts, chooser/upload progress, streaming, URLSearchParams,
and full FormData/Web IDL iterator identity remain open.

The completed bounded URLSearchParams follow-up is
[native-engine-browser-057](../tasks/native-engine-browser-057.md). String-only
`URLSearchParams` now serializes through the existing fetch/XHR owner as
bounded `application/x-www-form-urlencoded;charset=UTF-8` POST data with
`+`-encoded spaces. Full constructors, sorting, iterator/Web IDL identity,
and streaming remain open.

The completed bounded temporal-validation follow-up is
[native-engine-browser-058](../tasks/native-engine-browser-058.md). Local and
child-owned forms now strictly validate `date`, `month`, `time`, and
`datetime-local` values, including calendar validity and bounded `min`/`max`/
`step` checks in the correct temporal units. Pattern/file constraints, custom
validity, and full `ValidityState` Web IDL identity remained open at that
checkpoint; custom validity is covered by the later 059 API slice.

The completed bounded form-validation API follow-up is
[native-engine-browser-059](../tasks/native-engine-browser-059.md). Local and
child-owned controls now expose bounded `validity`, `validationMessage`, and
`willValidate` snapshots; `checkValidity()`/`reportValidity()` dispatch the
existing ordered `invalid` events; and `setCustomValidity()` persists through
the typed owner boundary. Pattern/file validation, picker/UI behavior, and
full live `ValidityState` Web IDL identity remain open.

The completed bounded FormData-constructor follow-up is
[native-engine-browser-060](../tasks/native-engine-browser-060.md). Local and
child-owned `new FormData(form)` calls collect named, enabled text controls in
document order, including controls associated through an external `form`
attribute, while submitter-only controls and unchecked checkbox/radio controls
are excluded. File controls fail closed with a `TypeError`; File/Blob parts,
picker/upload behavior, and full FormData Web IDL identity remain open.

The completed bounded pattern-validation follow-up is
[native-engine-browser-061](../tasks/native-engine-browser-061.md). Local and
child-owned text-like controls apply Rust-owned whole-value `pattern` checks
and expose `patternMismatch` through the existing validity API and submission
preflight. Invalid or unsupported regex syntax follows the HTML invalid-pattern
fallback and is ignored. Full JavaScript RegExp `v`-flag and Unicode-set
parity, file constraints, picker/UI behavior, and full live `ValidityState` Web
IDL identity remain open.

The completed bounded FormData select-control follow-up is
[native-engine-browser-062](../tasks/native-engine-browser-062.md). Local and
child-owned `new FormData(form)` calls preserve textarea values, selected
single-select values, and every initially selected enabled option of a
multi-select in document order. Interactive multi-select actions, `optgroup`
disabled inheritance, File/Blob parts, and full FormData Web IDL identity
remain open.

The completed bounded multi-select interaction follow-up is
[native-engine-browser-063](../tasks/native-engine-browser-063.md). Local and
child-owned option clicks toggle multiple selections, script `option.selected`
writes preserve them, `select.value` deterministically selects the first
matching option, and the host view exposes bounded `multiple`, `options`, and
`selectedOptions`. Modifier-key/range selection, keyboard listbox behavior,
text selection/IME, `optgroup` disabled inheritance, and option-collection Web
IDL identity remain open.

The completed bounded semantic storage-contract follow-up is
[native-engine-browser-064](../tasks/native-engine-browser-064.md). The native
dispatcher and `BrowserRuntimeSession` execute bounded local/session key-value
read, write, and clear operations with active-context validation. The state is
backend-instance scoped and intentionally not page-visible, durable,
origin-keyed, cookie-synchronized, or IndexedDB-backed; cookie-scope requests
remain explicitly unsupported.

The completed bounded page Web Storage realm follow-up is
[native-engine-browser-065](../tasks/native-engine-browser-065.md). The shared
QuickJS bootstrap exposes bounded `localStorage` and `sessionStorage` objects
with `length`, `key`, `getItem`, `setItem`, `removeItem`, and `clear` across
local and child-owned evaluations. Realm-local persistence and independent
stores are covered; origin navigation persistence, durable profiles, storage
events, cookie synchronization, IndexedDB, and full Storage Web IDL identity
remain open.

The completed origin-keyed page Web Storage transfer follow-up is
[native-engine-browser-066](../tasks/native-engine-browser-066.md). Bounded
local/session mutations are consumed by the runtime owner and transferred into
fresh local realms and the sandboxed content worker. Tuple origins share state
across navigation, while opaque local documents use a fragment-free document
key; local and session stores remain independent. State is volatile and
separate from the semantic `StorageRequest` maps. Durable profiles, storage
events, cookie synchronization, IndexedDB, quota policy, and full Storage Web
IDL identity remain open.

The completed bounded text-backed File/Blob FormData follow-up is
[native-engine-browser-067](../tasks/native-engine-browser-067.md). The shared
bootstrap exposes capped text-backed `Blob` and `File` values with bounded
`text()`/`slice()` metadata, accepts them in FormData `append`/`set`, and emits
filename/content-type multipart parts through the existing fetch owner in both
local and child realms. Binary buffers, streams, file pickers, disk file
controls, upload progress, and full Blob/File/FormData Web IDL identity remain
open.

The completed opt-in durable page-storage profile follow-up is
[native-engine-browser-068](../tasks/native-engine-browser-068.md). A bounded
JSON profile path restores origin-keyed page `localStorage` in local and
child-owned realms, persists mutations through navigation and close, and
keeps `sessionStorage` session-scoped. The path is explicit, with no
concurrent-writer coordination; storage events, cookie profile persistence,
IndexedDB, quota policy, and full Storage Web IDL identity remain open.

The completed bounded `document.cookie` synchronization follow-up is
[native-engine-browser-069](../tasks/native-engine-browser-069.md). Network
page realms read non-HttpOnly session cookies and submit bounded cookie lines;
the Rust-owned transport jar applies Secure/domain/path/expiry and
HttpOnly-visibility policy to later navigation and fetch requests. Storage
events, cookie profile persistence, IndexedDB, full binary/stream FormData
support, and full Storage Web IDL identity remain open.

The `7ac4c39d` navigation-semantics correction repaired revision allocation,
nonfatal page-script exceptions, POST form handoff, hidden-action rejection,
timer-clock determinism, and lifecycle effect ordering; the native integration
gate was re-run at 359 passed before the next feature slice.

The completed bounded same-profile local storage-event follow-up is
[native-engine-browser-070](../tasks/native-engine-browser-070.md). Concurrently
alive local native documents sharing the same explicit profile path receive
origin-filtered `storage` events for effective `localStorage` changes; source
documents are excluded, no-op mutations do not publish, and the receiving Rust
and JavaScript stores are synchronized before dispatch. This is an in-process
coordinator only. Session-storage browsing-context routing, profile-writer
locking, IndexedDB, full binary/stream FormData support, full Storage Web IDL
identity, and the remaining browser-context gates remain open.

The completed process-backed local storage-event follow-up is
[native-engine-browser-071](../tasks/native-engine-browser-071.md). Separate
sandboxed HTTP(S) content workers report effective page `localStorage` changes
through bounded IPC responses; the parent coordinator validates and fans them
out to other live engines sharing the explicit profile path, and a receiving
worker applies and dispatches the event before its next page operation. The
source worker remains excluded and session-storage events never enter the
cross-document coordinator. Independent Glass process/profile-writer
coordination, browsing-context routing, IndexedDB, full binary/stream FormData
support, full task ordering/navigation edge cases, remaining full
pattern-regex/file constraint validation, target contexts, and the remaining
browser-context primitives remain open.

The completed bounded Web Storage profile-I/O locking follow-up is
[native-engine-browser-072](../tasks/native-engine-browser-072.md). Profile
reads use shared retained OS locks and complete bounded snapshot writes use
exclusive retained locks through rename or the verified platform fallback;
contention returns a typed error after a short bounded retry, and the Linux
sandbox exposes the lock file to the worker. This serializes physical profile
I/O but does not merge stale independent full-state snapshots. Session-storage
browsing-context routing, cross-process event delivery, IndexedDB, full
binary/stream FormData support, full task ordering/navigation edge cases,
remaining full pattern-regex/file constraint validation, target contexts, and
the remaining browser-context primitives remain open.

The completed bounded session-storage routing follow-up is
[native-engine-browser-073](../tasks/native-engine-browser-073.md). Each native
engine now carries an explicit bounded browsing-context identity through local
and sandboxed JavaScript realms. Same-profile `sessionStorage` events are
origin-filtered and delivered only to other live engines with the same context
identity; different contexts receive no event or state mutation, and the
configured identity is reflected by native backend context validation and
responses. The coordinator is still process-local, session state is still
volatile, and cross-process event delivery is not claimed. Stale-snapshot
ownership or merge, IndexedDB, full binary/stream FormData support, full task
ordering/navigation edge cases, remaining full
pattern-regex/file constraint validation, target contexts, and the remaining
browser-context primitives remain open.

The completed bounded profile convergence follow-up is
[native-engine-browser-074](../tasks/native-engine-browser-074.md). Profile
snapshots carry a revision, and an exclusive write re-reads the current
snapshot then applies only the writer's bounded local-storage deltas. This
prevents unrelated keys from being lost when independent stale snapshots are
serialized, while same-key mutations retain lock-order last-writer behavior;
profiles written before the revision field remain readable. Session storage is
still volatile, and cross-process event delivery, cookie profile persistence,
IndexedDB, full
binary/stream FormData support, full task ordering/navigation edge cases,
remaining full pattern-regex/file constraint validation, target contexts, and
the remaining browser-context primitives remain open.

The completed bounded cross-process storage-event follow-up is
[native-engine-browser-075](../tasks/native-engine-browser-075.md). Profile-
backed engines use unique writer identities and cursors into a bounded
newline-delimited event journal protected by the retained profile lock. Local
and session changes are appended and live receivers poll before page
operations, exclude their writer, and apply origin/context routing before
dispatch. Incomplete tails are repaired, malformed records are typed errors,
and the journal is capped at 4 MiB. Retention and crash recovery are recorded
in the following slice.

The completed bounded cookie-profile persistence follow-up is
[native-engine-browser-076](../tasks/native-engine-browser-076.md). Supplying
the existing explicit profile path restores accepted Rust-owned cookies for
page `document.cookie`, HTTP navigation/fetch, and sandboxed content workers;
profiles without the optional cookie field remain readable. Cookie updates
re-read the latest locked snapshot and merge key-level replacements,
deletions, and bounded-jar evictions before the existing atomic commit path.
Expiry uses bounded wall-clock metadata, while session cookies are retained
for the explicit profile lifetime. Without a profile path cookies remain
volatile; the profile is sensitive credential-bearing state, and IndexedDB
plus full cookie policy/Web IDL parity remain open.

The current bounded journal-retention and recovery follow-up is
[native-engine-browser-077](../tasks/native-engine-browser-077.md). Profile-
backed engines register bounded leases at `P.readers` beside the `P.events`
journal, refresh healthy cursors on a bounded heartbeat, and explicitly remove
their lease on close. A 15-minute stale lease is reclaimable. When an append
would exceed 4 MiB, only a complete prefix acknowledged by every live lease is
compacted; retained cursors are shifted under `P.lock`, while a slow live
reader receives typed backpressure if it pins too much data. Missing leases and
out-of-range cursors trigger authoritative profile reload and a full-state
replacement in the local or sandboxed runtime; event callbacks discarded by
that recovery are not replayed, and a live engine preserves its volatile
session state. IndexedDB, full cookie policy/Web IDL parity, and the remaining
browser-complete gates remain open.

The completed bounded IndexedDB persistence and transaction follow-up is
[native-engine-browser-078](../tasks/native-engine-browser-078.md). Local and
sandboxed HTTP(S) content realms now share an origin-keyed JSON IndexedDB
subset with positive-version upgrades, object-store lifecycle, ordered
readonly/readwrite transactions, key paths, auto-increment keys, and bounded
CRUD requests. The profile carries at most 16 databases per origin, 128 stores
per database, 128 records per store, 8 KiB per JSON value, and 64 KiB for the
full IndexedDB state; content workers exchange validated snapshots through
IPC and do not open profile files. Indexes, cursors, key ranges, non-JSON
structured-clone values, full transaction/version-change coordination, quota
APIs, and cross-process IndexedDB journal/delta merging remained open at that
checkpoint. The same follow-up suppresses the timer pump while initial page
scripts are loaded and resets the deterministic timer clock at the
page-operation boundary.

The completed bounded IndexedDB journal-convergence follow-up is
[native-engine-browser-079](../tasks/native-engine-browser-079.md). Local
realms and sandboxed content workers now emit bounded typed IndexedDB deltas
beside Web Storage journal records. The parent validates and applies them,
merges each writer against the latest profile snapshot under `P.lock`, and
appends the delta batch in journal order. Live receivers apply IndexedDB-only
records and refresh the current origin in their persistent realm through a
bounded full-state transfer; worker responses no longer repeat the full
snapshot, and workers never open profile, journal, lock, or lease files.
Disjoint live database writers are covered on both local and process-backed
paths. Same-database schema conflicts use journal order as the explicit
last-writer rule. Indexes, cursors, key ranges, non-JSON structured-clone
values, full transaction/version-change coordination, quota APIs, and the
remaining browser-complete gates remain open.

The completed bounded IndexedDB query-primitives follow-up is
[native-engine-browser-080](../tasks/native-engine-browser-080.md). Object
stores now retain bounded string-key-path indexes with unique and multi-entry
constraints; `IDBKeyRange` supports exact, lower, upper, and bound queries;
and store/index lookups, counts, deletes, ordered key/value cursors,
continuation, advancement, update, and delete are available for the supported
JSON key model. Index metadata uses a dedicated delta and record changes keep
the established merge path. Query-time derivation scans at most the bounded
128-record store, trading large-database throughput for deterministic resource
use and a compact profile. Compound keys, array keys outside `multiEntry`,
non-JSON values, full transaction/version-change coordination, quota APIs, and
the remaining browser-complete gates remain open.

The completed bounded IndexedDB version-change follow-up is
[native-engine-browser-081](../tasks/native-engine-browser-081.md). A
persistent same-realm connection registry dispatches `versionchange`, keeps a
higher-version `open()` pending behind one `blocked` notification while an
older connection remains open, and retries the upgrade after the final
connection calls `close()`. The resumed version-change transaction exposes its
current object-store list and runs the existing upgrade callbacks. Cross-
process live connection identity, complete `deleteDatabase` blocking,
rollback, and the remaining browser-complete gates remain open.

The completed bounded IndexedDB deletion-lifecycle follow-up is
[native-engine-browser-082](../tasks/native-engine-browser-082.md).
Same-realm `deleteDatabase()` dispatches `versionchange` with
`newVersion: null`, keeps the request pending behind one `blocked` event while
an older connection remains open, and removes state after the final connection
calls `close()`. Missing deletes are successful no-ops and a later open can
recreate the database. Cross-process live connection identity, full factory
operation-queue ordering, rollback, and the remaining browser-complete gates
remain open.

The completed bounded IndexedDB transaction-rollback follow-up is
[native-engine-browser-083](../tasks/native-engine-browser-083.md). Ordinary
write transactions snapshot the bounded JSON state and restore it on request
failure or explicit `abort()`, delivering one `onabort` and never a false
`oncomplete`. Concurrent transaction scheduling, upgrade-failure rollback,
structured-clone values, quota APIs, and the remaining browser-complete gates
remain open.

The completed bounded StorageManager quota follow-up is
[native-engine-browser-084](../tasks/native-engine-browser-084.md).
`navigator.storage.estimate()` reports deterministic JSON-length usage against
the fixed 4 MiB native profile quota; `persist()` and `persisted()` are stable
asynchronous APIs that return `false` until an explicit permission policy is
implemented. Quota prompts, reservation, cross-process arbitration,
structured-clone values, and the remaining browser-complete gates remain open.

The completed bounded IndexedDB transaction-serialization follow-up is
[native-engine-browser-085](../tasks/native-engine-browser-085.md). Same-realm
transactions for one database execute through an ordered queue; request
callbacks precede the next operation, and queued transactions snapshot state
only when they begin. Rollback therefore cannot erase a predecessor’s
committed work. Cross-realm/process scheduling, upgrade-failure rollback,
structured-clone values, quota permission policy, and the remaining
browser-complete gates remain open.

The completed bounded IndexedDB structured-clone follow-up is
[native-engine-browser-086](../tasks/native-engine-browser-086.md). Tagged
JSON preserves `undefined`, non-finite and negative-zero numbers, `Date`,
`RegExp`, `Map`, and `Set` across API reads, worker transfer, profile
persistence, and restart; cyclic, unsupported, reserved-tag, invalid-date,
and oversize values fail closed. Binary buffers, typed arrays, Blob/File
payloads, BigInt, transfer lists, and full clone/prototype parity remain open.

The completed bounded IndexedDB Blob/File clone follow-up is
[native-engine-browser-087](../tasks/native-engine-browser-087.md). Existing
text-backed native `Blob` and `File` values retain payload, MIME type, file
name, and non-negative `lastModified` across IndexedDB reads and profile
restart. Byte-exact binary buffers, typed arrays, streams, transfer lists, and
full clone/prototype parity remained open at that checkpoint.

The completed bounded IndexedDB binary structured-clone follow-up is
[native-engine-browser-088](../tasks/native-engine-browser-088.md). `ArrayBuffer`,
common typed arrays, and `DataView` now cross the bounded tagged-JSON profile
and shared worker state-transfer paths as visible byte vectors, including API
read and profile-restart reconstruction. `SharedArrayBuffer` fails closed;
transfer lists, detached-buffer identity, binary Blob/File methods, streams,
and full clone/prototype parity remain open.

The completed bounded Blob/File binary-read follow-up is
[native-engine-browser-089](../tasks/native-engine-browser-089.md). The
existing text-backed objects now provide fresh UTF-8 `ArrayBuffer` and
`Uint8Array` results through `arrayBuffer()` and `bytes()`, with deterministic
surrogate handling and bounded output in local and worker realms. Binary Blob
construction, streams, transfer semantics, upload progress, and full
Blob/File Web IDL parity remain open.

The completed bounded fetch Blob/File-body follow-up is
[native-engine-browser-090](../tasks/native-engine-browser-090.md). Direct
`fetch()` requests now use the existing text-backed Blob/File payload and
inherit its normalized MIME type when no explicit supported content type is
provided. The worker network path is covered; XHR Blob bodies, binary Blob
construction, streams, abort, upload progress, and full Fetch/Blob parity
remain open.

The completed bounded XHR Blob/File-body follow-up is
[native-engine-browser-091](../tasks/native-engine-browser-091.md). Asynchronous
XHR now preserves text-backed Blob/File bodies through the existing fetch
bridge, including normalized MIME propagation and worker HTTP coverage.
Binary responses, upload progress, timeout/abort, streams, synchronous XHR,
binary Blob construction, and full XHR/Blob parity remain open.

The completed bounded fetch-abort follow-up is
[native-engine-browser-092](../tasks/native-engine-browser-092.md). Native
`AbortController`/`AbortSignal` state and event behavior now reject associated
observable fetch promises, remove settled listeners, and ignore late host
responses. This does not cancel the already-issued bounded network operation;
socket cancellation, XHR abort, timeout/progress, `AbortSignal.timeout/any`,
and full Web IDL parity remain open.

The completed bounded URLSearchParams follow-up is
[native-engine-browser-093](../tasks/native-engine-browser-093.md).
`URLSearchParams` now handles bounded query strings, pair/record/instance
construction, stable sorting, size, callbacks, optional-value deletion, and
snapshot iterators with corrected form-urlencoded escaping. Full live Web IDL
iterator identity and exotic iterable inputs remain open.

The completed bounded fetch response-body follow-up is
[native-engine-browser-094](../tasks/native-engine-browser-094.md). Fetch
responses now provide fresh text-backed Blob, UTF-8 ArrayBuffer, and Uint8Array
results through `blob()`, `arrayBuffer()`, and `bytes()` while preserving
independent text/json reads. Streaming and byte-preserving non-UTF-8 response
transport remain open.

The completed bounded fetch response-headers follow-up is
[native-engine-browser-095](../tasks/native-engine-browser-095.md). The
response now exposes a read-only normalized content-type snapshot through
case-insensitive lookup, `has`, `get`, bounded iterators, and `forEach`.
Multiple/raw response headers, trailers, mutation, and full Headers/Web IDL
parity remain open; bounded request-header dictionaries are covered by the
later 097 slice.

The completed bounded XHR-abort follow-up is
[native-engine-browser-096](../tasks/native-engine-browser-096.md). An active
asynchronous XHR abort now uses a request-local signal, resets observable
state to `UNSENT`, emits bounded `readystatechange`/`abort` callbacks, and
guards against late `load`/`error` continuations. Transport cancellation,
timeout/progress, and complete XHR/Web IDL parity remain open.

The completed bounded byte-preserving response follow-up is
[native-engine-browser-098](../tasks/native-engine-browser-098.md). A bounded
base64 response payload now preserves raw bytes for Fetch `Blob`,
`ArrayBuffer`, `bytes()`, and binary `Blob.slice()` reads, while text and JSON
remain replacement-decoded UTF-8 views. Streaming/BYOB, transfer identity,
binary Blob/File construction, binary/stream FormData parity, and complete
Fetch/Blob Web IDL parity remain open.

The completed bounded binary Blob/File request-body follow-up is
[native-engine-browser-099](../tasks/native-engine-browser-099.md). Fetch and
asynchronous XHR now carry raw bytes for Blob/File values that already expose
a bounded byte snapshot; the content process validates the base64 transfer and
the resource loader sends the resulting bytes directly. Text-backed bodies,
MIME propagation, redirects, and abort behavior remain stable. Binary
Blob/File construction, multipart FormData byte parity, streaming, upload
progress, and complete Fetch/XHR/Blob parity remain open.

The completed bounded binary Blob/File construction follow-up is
[native-engine-browser-100](../tasks/native-engine-browser-100.md). Native
`Blob` and `File` accept bounded `ArrayBuffer` and typed-array parts, retain
exact bytes and byte length, and feed the 099 Fetch/XHR request-body bridge;
text-only parts retain their established behavior. Streaming multipart
FormData, streams, upload progress, and full Blob/File Web IDL parity remain
open.

The completed bounded response-header follow-up is
[native-engine-browser-101](../tasks/native-engine-browser-101.md). Fetch and
asynchronous XHR now expose bounded normalized response-header snapshots,
combine duplicate names, and apply same-origin/CORS-exposed filtering.
`Set-Cookie`, invalid raw header bytes, trailers, mutation, and full Headers
Web IDL parity remain open.

The completed bounded binary FormData-part follow-up is
[native-engine-browser-102](../tasks/native-engine-browser-102.md). Multipart
Fetch and asynchronous XHR now carry raw-byte-backed Blob/File parts through
the existing bounded serializer, preserving boundary, filename, MIME, and
text-field behavior. Streaming FormData, upload progress, iterator identity,
and complete FormData/Blob Web IDL parity remain open.

The completed bounded Fetch `Headers` init/mutation follow-up is
[native-engine-browser-103](../tasks/native-engine-browser-103.md). Native
Fetch now accepts bounded records, pair sequences, and native `Headers`
instances with case-insensitive append/set/delete/get/iteration behavior;
response headers remain read-only snapshots, and full Headers Web IDL identity
and exotic iterable parity remain open.

The completed bounded XHR binary-response follow-up is
[native-engine-browser-104](../tasks/native-engine-browser-104.md). Async XHR
now supports bounded `arraybuffer` and `blob` response types with
byte-preserving `response` values while retaining text-mode behavior.

The completed bounded XHR-timeout follow-up is
[native-engine-browser-105](../tasks/native-engine-browser-105.md). Async XHR
now carries a bounded non-zero timeout through the existing request bridge;
zero disables the extra deadline, and a timed-out request reports `DONE`,
status zero, cleared response state, and `ontimeout` with stale completion
suppressed. Upload progress, transport cancellation beyond the bounded
deadline, streaming, synchronous XHR, and complete XHR Web IDL parity remain
open.

The completed bounded CORS-preflight-cache follow-up is
[native-engine-browser-106](../tasks/native-engine-browser-106.md). Successful
preflights with a positive `Access-Control-Max-Age` are cached by document
origin, target, method, credentials mode, and sorted requested headers within
a bounded 64-entry/10-minute budget; failed, invalid, and zero-age responses
are not cached. Private-network access, opaque `no-cors` responses, and full
Fetch/CORS Web IDL parity remain open.

The completed bounded FormData-iterator follow-up is
[native-engine-browser-107](../tasks/native-engine-browser-107.md). FormData
now exposes deterministic snapshot iterators for `entries()`, `keys()`,
`values()`, and `[Symbol.iterator]()` with self-iterating `next()` results,
while `forEach()` and multipart serialization retain the existing ordered text
and Blob/File entry owner. Live mutation during iteration, exotic iterables, and
complete FormData/Web IDL parity remain open.

The completed bounded URLSearchParams-iterable follow-up is
[native-engine-browser-108](../tasks/native-engine-browser-108.md). The
constructor now accepts bounded Map, Set, and other pair-iterable inputs as
two-value sequences while retaining insertion order and the existing mutation,
sorting, encoding, and snapshot-iterator owners. Live iterator mutation/
identity and complete URLSearchParams Web IDL parity remain open.

The completed bounded Fetch-mode follow-up is
[native-engine-browser-109](../tasks/native-engine-browser-109.md). Fetch now
defaults to `cors`, supports bounded `cors`, `no-cors`, and `same-origin`
policy, rejects cross-origin `same-origin` targets and non-safelisted
cross-origin `no-cors` request headers/content types before network I/O, and
projects successful cross-origin `no-cors` requests as opaque responses with
status zero, an empty URL/header view, and rejected body reads. Service-worker
and private-network integration, streaming, redirect parity, and complete
Fetch/Response Web IDL parity remain open.

The completed bounded Fetch-redirect follow-up is
[native-engine-browser-110](../tasks/native-engine-browser-110.md). Fetch now
accepts `redirect: "follow" | "error" | "manual"`; follow reports a bounded
`redirected` flag and final URL, error rejects at the first redirect, manual
returns a filtered `opaqueredirect` response, and same-origin mode rejects
cross-origin redirect hops. Full redirect-status/referrer parity,
service-worker/private-network routing, streaming, and complete
Fetch/Response Web IDL parity remain open.

The completed bounded AbortSignal-combinator follow-up is
[native-engine-browser-111](../tasks/native-engine-browser-111.md).
`AbortSignal.timeout()` now schedules one bounded `TimeoutError` abort on a due
host turn, while `AbortSignal.any()` composes a bounded iterable of native
signals with first-reason propagation, empty-input non-abortion, and listener
cleanup. Transport cancellation, XHR integration, and complete AbortSignal/Web
IDL parity remain open.

The completed bounded live-iterator follow-up is
[native-engine-browser-112](../tasks/native-engine-browser-112.md).
URLSearchParams `entries()`, `keys()`, and `values()` now retain their owner,
observe bounded later mutations, and return self-iterating cursors, while
`[Symbol.iterator]` remains the `entries` method and existing encoding/body
owners stay unchanged. Full Web IDL descriptor parity and complex deletion or
reordering mutation semantics remain open.

The completed bounded static-abort follow-up is
[native-engine-browser-113](../tasks/native-engine-browser-113.md).
`AbortSignal.abort(reason)` now creates an already-aborted signal with the
default or supplied reason through the existing signal consumers, without
allocating a timer or dispatching a post-construction event. Transport
cancellation, XHR integration, and complete AbortSignal/Web IDL parity remain
open.

The completed bounded live-FormData-iterator follow-up is
[native-engine-browser-114](../tasks/native-engine-browser-114.md). FormData
`entries()`, `keys()`, and `values()` now retain their owner, observe bounded
later mutations, and return self-iterating cursors while multipart serialization
and `forEach()` stay on their existing owners. Complex deletion/reordering
semantics and complete FormData/Web IDL parity remain open.

The completed bounded live-request-Headers follow-up is
[native-engine-browser-115](../tasks/native-engine-browser-115.md). Mutable
request `Headers` `entries()`, `keys()`, and `values()` now retain their owner,
observe bounded `append()`/`set()` mutations, and return self-iterating cursors;
immutable response-header views remain bounded snapshots. Full Headers Web IDL
parity, raw response headers, and trailers remain open.

The completed bounded Fetch Response-stream follow-up is
[native-engine-browser-116](../tasks/native-engine-browser-116.md). Ordinary
responses now expose a bounded one-chunk native `ReadableStream` body with
`instanceof`, default-reader completion, lock/release, cancel, and async-
iterator hooks, while opaque and `opaqueredirect` responses keep `body ===
null` and existing filtered body-method failures. Progressive transport
streaming, backpressure, body disturbance/`bodyUsed`, BYOB readers,
transport-level cancellation, trailers, and complete ReadableStream/Response
Web IDL parity remain open.

The completed bounded Fetch Response-clone follow-up is
[native-engine-browser-117](../tasks/native-engine-browser-117.md). Native
`Response.clone()` now returns a fresh bounded response with independent
headers and body owners; ordinary clones preserve the existing body readers
and one-chunk stream, while opaque and `opaqueredirect` clones retain their
filtered shells and `body === null`. Full body disturbance/`bodyUsed`, clone
rejection for locked or consumed bodies, shared tee/backpressure semantics,
Request/Response constructors, and complete Response Web IDL parity remain
open.

The completed bounded Fetch Request-object follow-up is
[native-engine-browser-118](../tasks/native-engine-browser-118.md). Bounded
`Request` construction, cloning, and `fetch(request, overrides)` now reuse the
existing GET/POST, header, mode, redirect, credentials, body, signal, CORS,
abort, and transport owners. Full Request body streams, disturbance rules,
URL/cache/referrer/integrity fields, duplex, and complete Request/Headers Web
IDL parity remain open.

The completed bounded URL-object follow-up is
[native-engine-browser-119](../tasks/native-engine-browser-119.md). HTTP(S)-
focused `URL` construction, relative path/query/fragment resolution,
component inspection, and snapshot `searchParams` now feed bounded Request and
Fetch URL inputs. URL setters, full percent-encoding/IDNA/IPv6/default-port
parity, live search-parameter synchronization, non-HTTP scheme parity, and
complete URL/Web IDL identity remain open.

The completed bounded live-URL-search-parameter follow-up is
[native-engine-browser-122](../tasks/native-engine-browser-122.md). URL
`searchParams` owners now synchronize bounded `append()`, `set()`, `delete()`,
and `sort()` mutations to `search`/`href`; bounded `search` and `hash`
assignment updates the same URL owner, and Request/Fetch handoff observes the
current href. Full URL setter/parser and encoding parity, default-port/
IDNA/IPv6 behavior, and complete URL/Web IDL identity remain open.

The completed bounded Response-constructor follow-up is
[native-engine-browser-120](../tasks/native-engine-browser-120.md). Fetched and
constructed responses now share `Response` identity; `new Response`,
`Response.json`, `Response.error`, and `Response.redirect` reuse the bounded
body/header/clone projections, with null bodies exposing `body === null`.
Stream input, body disturbance/`bodyUsed`, complete factory and redirect/error
internals, trailers, shared tee/backpressure, and complete Response Web IDL
identity remain open.

The completed bounded response-Headers-identity follow-up is
[native-engine-browser-121](../tasks/native-engine-browser-121.md). Fetch and
asynchronous XHR response-header views now satisfy `headers instanceof Headers`
while remaining immutable normalized snapshots; lookup, duplicate combination,
iteration, filtering, and `forEach()` remain unchanged, and response mutation
methods reject. Raw header bytes, trailers, live mutation, descriptor parity,
and complete Headers/Fetch/XHR Web IDL parity remain open.

## Baseline and constraints

The current checkout has exactly two installable crates. The native engine
must stay inside `glass-browser`, remain default-off, and keep native-only
dependencies optional behind the `native-engine` feature. Chromium/CDP remains
the production path; native selection is explicit-only.

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
| `native_engine::runtime` | runtime lifecycle, cancellation, typed microtasks, bounded trace, and scheduler ownership | lifecycle events, typed work, delay, cancellation | runtime state, task/microtask readiness, trace events | scheduler + native error |
| `native_engine::worker` | bounded async command ownership over the single runtime state | typed runtime commands and cancellation | serialized task/microtask results, traces, and worker failures | Tokio sync/task + runtime |
| `native_engine::content_process` | child-helper discovery, bounded framed lifecycle/document/script/fetch/document-transfer IPC, request correlation, response validation, and fail-closed process shutdown | protocol frames, helper executable, bounded HTTP(S) requests, and script source | typed process liveness/load/commit/script/fetch/close acknowledgements or worker errors | Tokio process/io + serde JSON + resource loader + JavaScript realm |
| `native_engine::scheduler` | logical clock and bounded ordered tasks | task kind, delay | deterministic task IDs/order | native limits; owned by runtime |
| `native_engine::history` | current local history and per-entry root scroll state | committed URL/revision/scroll offset | bounded entries/current index | native limits + layout point |
| `native_engine::origin` | opaque local origins and normalized HTTP(S) tuple origins | loaded URL | origin state/serialization | `url`, typed native error |
| `native_engine::resource_loader` | fixture/data/about resources plus bounded HTTP(S) HTML document loading | validated URL, async transport response | bounded HTML resource | `url`, `reqwest`, `futures-util`, config limits |
| `native_engine::javascript` | one bounded persistent ECMAScript realm, refreshed host projection, typed DOM command collection, promise completion, and JSON result conversion | validated script source, document snapshot, committed URL/origin, viewport, runtime limits | JSON-serializable script result, bounded host commands, or typed evaluation failure | optional `rquickjs` QuickJS-NG binding + native DOM snapshot |
| `native_engine::css` | bounded selector/rule parsing, display/visibility presentation, inherited color, positive-pixel line-height, pixel dimensions, physical solid/dashed/dotted borders, circular border radii, physical padding/margin edges, local opacity alpha, inherited `font-weight:normal|bold|400|700`, inherited `font-style:normal|italic`, inherited `word-break:normal|break-all`, inherited `vertical-align:baseline|top|middle|bottom`, bounded author-origin `!important` priority for the supported text-flow and text-decoration declarations, and bounded local flex-row `justify-content:normal|flex-start|center|flex-end|space-between|space-around|space-evenly|stretch`, explicit `justify-content:inherit` parent propagation, flex-item `order`, flex cross-axis `align-items`, explicit `align-items:inherit` parent propagation, `align-self:auto|flex-start|center|flex-end`, explicit `align-self:inherit` parent propagation, `flex-direction`, `flex-wrap`, `flex-flow`, `align-content:flex-start|center|flex-end|space-between|space-around|space-evenly|stretch|normal`, explicit `align-content:inherit` parent propagation, integer `flex-grow`, integer `flex-shrink`, `flex-basis:auto|Npx`, and `flex` shorthand | style text, inline style, native element attributes, ancestor styles | deterministic computed presentation values | native DOM element surface |
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
   `NativeRuntime` around the deterministic scheduler.
2. `NativeEngineBackend::new` creates one `NativeEngine` and validates its
   experimental `BackendProfile`.
3. `BackendFactory::native` registers the backend without adding it to
   automatic selection candidates.
4. `BackendFactory::start` permits the native candidate only when the request's
   preferred backend ID is `native-engine`.
5. `BrowserBackendDispatcher::initialize` reaches the engine and runtime
   lifecycle states, including the startup commit trace.
6. `BrowserBackendDispatcher::navigate` reaches resource loading, DOM parsing,
   runtime-owned scheduler commit, history, and revision generation.
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
- JavaScript-driven DOM mutation/event dispatch, timers, storage, workers,
  modules, page-script loading, Web APIs, or downloads;
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
| `native-engine-136` | completed reuse of bounded named-layer priority and private component rollback for standalone non-inherited `flex:revert-layer`, preserving finite shorthand expansion, same-block longhand precedence, independent component fallbacks, and existing layout/artifact consumers | `native-engine-135` | other CSS-wide keywords, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, animation, script, grid, writing modes, percentage/intrinsic sizing, and browser-wide Flexbox conformance |
| `native-engine-137` | completed reuse of bounded named-layer priority and private component rollback for standalone `flex-flow:revert-layer` and `place-content:revert-layer`, preserving finite shorthand expansion, same-block longhand precedence, independent component fallbacks, and existing flex/artifact consumers | `native-engine-136` | other CSS-wide keywords, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, animation, script, grid, writing modes, percentage/intrinsic sizing, and browser-wide Flexbox conformance |
| `native-engine-138` | completed reuse of bounded named-layer priority and private component rollback for standalone `gap:revert-layer`, `row-gap:revert-layer`, and `column-gap:revert-layer`, preserving finite integer-pixel shorthand expansion, same-block shorthand/longhand precedence, independent zero fallback, and existing flex/artifact consumers | `native-engine-137` | other CSS-wide keywords, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, percentages/fractional lengths, grid, and browser-wide gap conformance |
| `native-engine-139` | completed reuse of bounded named-layer priority and private per-property rollback for inherited `text-transform:revert-layer`, `font-weight:revert-layer`, `font-style:revert-layer`, and `word-break:revert-layer`, preserving finite public values, parent/root fallback, and existing fixed-cell text/layout/raster consumers | `native-engine-138` | other CSS-wide keywords, multiple origins, `!important` inversion, layer statements, anonymous/comma/nested layers, Unicode case mapping, font metrics, other word-break modes, writing modes, and browser-wide text conformance |
| `native-engine-140` | completed reuse of bounded named-layer priority and private per-property rollback for inherited `word-spacing:revert-layer` and `letter-spacing:revert-layer`, preserving finite non-negative pixel values, parent/root fallback, and existing fixed-cell spacing/layout/raster consumers | `native-engine-139` | other CSS-wide keywords, negative/relative/percentage/fractional spacing, `normal`, pair-boundary and cross-fragment semantics, multiple origins, `!important` inversion, layer statements, Unicode shaping/metrics, writing modes, and browser-wide text conformance |
| `native-engine-141` | completed reuse of bounded named-layer priority and private rollback for inherited `vertical-align:revert-layer`, preserving finite `baseline|top|middle|bottom` values, parent/root fallback, and existing inline line-item/layout/raster consumers | `native-engine-140` | other CSS-wide keywords, baseline metrics, lengths, percentages, multiple origins, `!important` inversion, layer statements, bidi, writing modes, ruby/table-cell/replaced-element alignment, and browser-wide text conformance |
| `native-engine-142` | completed reuse of bounded named-layer priority and private rollback for local `text-indent:revert-layer` and `text-overflow:revert-layer`, preserving finite non-negative fixed-pixel indentation, `clip|ellipsis`, local fallbacks, and existing text-flow/truncation/layout/raster consumers | `native-engine-141` | other CSS-wide keywords, negative or hanging indentation, percentages, font-relative units, inherited text-overflow, marker customization, multiple origins, `!important` inversion, layer statements, and browser-wide text conformance |
| `native-engine-143` | completed reuse of bounded named-layer priority and private rollback for local `width:revert-layer`, `height:revert-layer`, `min-width:revert-layer`, `max-width:revert-layer`, `min-height:revert-layer`, and `max-height:revert-layer`, preserving finite non-negative pixel dimensions, absent local fallbacks, and existing box-model/layout/raster consumers | `native-engine-142` | other CSS-wide keywords, percentages, negative dimensions, intrinsic sizing, aspect ratio, multiple origins, `!important` inversion, layer statements, and browser-wide CSS sizing conformance |
| `native-engine-144` | completed reuse of bounded named-layer priority and private rollback for local `box-sizing:revert-layer`, physical padding/margin shorthand and longhands, and bounded `margin:auto`, preserving independent edge ownership, content-box/zero fallbacks, and existing box-model/layout/artifact consumers | `native-engine-143` | other CSS-wide keywords, percentages, negative/logical edges, margin collapsing, positioned/replaced sizing, multiple origins, `!important` inversion, layer statements, and browser-wide box-model conformance |
| `native-engine-145` | completed reuse of bounded named-layer priority and private rollback for local `background-color:revert-layer` and inherited `color:revert-layer`, preserving independent `None`/inherited fallbacks and existing fill/text artifact consumers | `native-engine-144` | other CSS-wide keywords, `currentColor`, gradients, system colors, percentages, color spaces, border-color, multiple origins, `!important` inversion, layer statements, and browser-wide CSS color conformance |
| `native-engine-146` | completed reuse of bounded named-layer priority and private rollback for local `overflow:revert-layer`, `overflow-x:revert-layer`, and `overflow-y:revert-layer`, preserving independent x/y visible fallbacks and existing clip consumers | `native-engine-145` | other CSS-wide keywords, nested scrolling, scrollbars, `visible`/`auto`/`scroll` used-value parity, multiple origins, `!important` inversion, layer statements, and browser-wide CSS overflow conformance |
| `native-engine-147` | completed reuse of bounded named-layer priority and private rollback for local one-to-four-value integer `border-radius:revert-layer`, preserving zero-corner fallback and existing rounded fill/border/point-hit consumers | `native-engine-146` | other CSS-wide keywords, elliptical or percentage radii, corner longhands, nested clips, anti-aliasing, multiple origins, and browser-wide CSS border-radius conformance |
| `native-engine-148` | completed reuse of bounded named-layer priority and private rollback for local 8-bit `opacity:revert-layer`, preserving full-opacity fallback, reduced-opacity group/compositing consumers, and unchanged geometry/artifact owners | `native-engine-147` | other CSS-wide keywords, inherited opacity, stacking-context/blending parity, filters, animation, multiple origins, and browser-wide CSS opacity conformance |
| `native-engine-149` | completed reuse of bounded named-layer priority and private rollback for local `display:revert-layer` and `visibility:revert-layer`, preserving normal-flow/visible fallbacks and existing hidden-subtree/artifact consumers | `native-engine-148` | other CSS-wide keywords, inherited visibility, display decomposition, formatting-context parity, table/ruby/flow-root details, animation, multiple origins, and browser-wide CSS display/visibility conformance |
| `native-engine-150` | completed reuse of bounded named-layer priority and private per-side rollback for physical `border:revert-layer` and `border-top|right|bottom|left:revert-layer`, preserving zero-width/no-paint fallbacks and existing box-model/border-artifact consumers | `native-engine-149` | other CSS-wide keywords, logical sides, border-image, gradients, unsupported border styles, animation, multiple origins, and browser-wide CSS border conformance |
| `native-engine-151` | completed physical `border-color` one-to-four-value expansion and `border-top|right|bottom|left-color` longhands with private per-side `revert-layer` color candidates, same-block declaration order, and bounded black fallback, preserving independent width/style and border-artifact consumers | `native-engine-150` | other CSS-wide keywords, standalone border-width/style, logical sides, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-152` | completed physical `border-width` one-to-four-value expansion and `border-top|right|bottom|left-width` longhands with a private per-side `revert-layer` width stream, preserving independent style/color and border-artifact consumers | `native-engine-151` | other CSS-wide keywords, standalone border-style, logical sides, `currentColor`, gradients, border-image, fractional/percentage widths, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-153` | completed physical `border-style` one-to-four-value expansion and `border-top|right|bottom|left-style` longhands with a private per-side `revert-layer` style stream, same-block declaration order, and final width/style/color composition, preserving no-style fallback and border-artifact consumers | `native-engine-152` | other CSS-wide keywords, `none`, logical sides, `currentColor`, gradients, border-image, unsupported styles, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-154` | completed explicit physical `border-style:none` in one-to-four-value shorthand and physical style longhands through a private no-paint sentinel, preserving no-side/zero-width and public paint artifacts; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-153` | `hidden`, other border styles, logical sides, `currentColor`, gradients, border-image, omitted-component `border:none`, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-155` | completed explicit physical `border-style:hidden` in one-to-four-value shorthand and physical style longhands through a distinct private no-paint sentinel, preserving current no-side/zero-width and public paint artifacts while retaining future table-conflict meaning; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-154` | collapsed-table border conflict resolution, other border styles, logical sides, `currentColor`, gradients, border-image, omitted-component `border:none`, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-156` | completed batched painted physical `border-style` variants `double`, `groove`, `ridge`, `inset`, and `outset` through public computed paint values and deterministic integer-pixel software replay, preserving the existing display-list shape; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-155` | anti-aliased joins, percentage/fractional widths, logical sides, table layout/conflict resolution, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-157` | completed exact omitted-component `border:none` and physical `border-top|right|bottom|left:none` forms through a private declaration wrapper that reuses the existing no-paint style stream and preserves public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally | `native-engine-156` | arbitrary omitted-component defaults, `border:hidden`, other CSS-wide reset keywords, logical sides, table layout/conflict resolution, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-158` | completed exact omitted-component `border:hidden` and physical `border-top|right|bottom|left:hidden` forms through the existing private hidden style stream, preserving its future table-conflict distinction and public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally | `native-engine-157` | arbitrary omitted-component defaults, other CSS-wide reset keywords, logical sides, table layout/conflict resolution, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-159` | completed bounded complete `Npx hidden color` values for `border` and physical border shorthands, preserving private declared width/color candidates while projecting hidden style without changing public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally | `native-engine-158` | arbitrary omitted-component defaults, other CSS-wide reset keywords, logical sides, table layout/conflict resolution, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |

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
14. Native layout derives visible element rectangles from the current DOM and
    viewport without creating a second mutable owner.
15. Native point clicks reject malformed/out-of-viewport points and resolve
    through the deepest layout hit to an actionable semantic ancestor before
    any mutation.
16. Native display lists are derived from a matching layout revision and emit
    bounded deterministic commands without mutating the document.
17. Native software surfaces replay a bounded display list into logical RGBA
    pixels without mutating page state.
18. Native style resolution inherits only `color` through bounded DOM parent
    links, and display-list text consumes that resolved value.
19. Native fill/text commands carry bounded logical clips derived from matching
    `overflow:hidden` ancestors, and software replay enforces them.
20. Native uniform border declarations produce matching revisioned `BorderRect`
    commands, preserve deterministic fill/border/text order, and replay only
    inside the layout box and its ancestor clips.
21. Native PNG capture encodes the current logical surface through the real
    backend dispatcher, enforces the stable capture-byte limit, preserves the
    revision, and denies unsupported JPEG/PDF formats explicitly.
22. Native box-model derivation exposes outer and content rectangles, applies
    bounded border/padding insets to child/text origins, applies uniform
    margins to normal flow, and preserves deterministic hit-test ownership.
23. Native vertical scroll dispatch derives a bounded root offset, clamps it to
    content height, maps point hits and display replay through that offset, and
    preserves revision/effect behavior for moved/no-op/unsupported deltas.
24. Native side-specific border declarations cascade independently, contribute
    to outer/content geometry, emit bounded display-list paint data, and replay
    with clipping and root-scroll translation.
25. Native solid/dashed/dotted border styles survive physical-side cascade,
    typed display-list projection, deterministic pattern replay, clipping, and
    root-scroll translation while unsupported styles remain ignored.
26. Native bounded physical border radii survive shorthand expansion and
    cascade, concrete-box normalization, layout hit testing, display-list
    projection, rounded fill/border replay, rectangular ancestor clipping, and
    root-scroll translation while unsupported radius forms remain ignored.
27. Native inline element boxes use the same bounded width calculation for
    preflight line-fit decisions and final layout, so adjacent inline boxes
    wrap deterministically before display-list, hit-test, and scroll consumers
    observe their document-space geometry.
28. Native fixed-pixel line-height values are resolved before child flow, so
    direct text and inline boxes share a deterministic minimum line height while
    explicit element heights retain box-model precedence for layout, paint, and
    hit-test consumers.
29. Native direct text is collapsed and fragmented during the same flow pass
    that places inline boxes, so display-list origins and source order match
    layout coordinates while style and ancestor clips remain owned by the
    containing element.
30. Native collapsed direct text keeps a complete word on the current fixed
    line when it fits, drops its separator when it wraps, and splits only a
    word that exceeds the full line width; the resulting fragments remain in
    source order for paint and scroll consumers.
31. Native physical padding and margin shorthands expand deterministically,
    physical longhands cascade per side, and the resulting top/right/bottom/
    left values feed content origins, normal-flow margins, and all existing
    layout/paint/hit/scroll consumers.
32. Native direct-text flow retains bounded leading/trailing whitespace
    boundaries, joins only source-separated inline-flow items, paints consumed
    separators through the existing text path, and drops separators that would
    begin a fresh wrapped line.
33. Native layout derives one bounded rectangular `overflow:hidden` ancestor
    intersection per layout box, and viewport projection plus point hit-testing
    consume that same document-space clip before root-scroll translation.
34. Native stylesheet and inline-style parsing records bounded sanitized
    diagnostics for unsupported selectors, properties, values, and malformed
    rules; document preparation carries the list atomically with navigation and
    the explicit Rust API reports its revision and truncation state.
35. Native direct surface output and decoded bounded PNG bytes match one
    complete checked-in logical-pixel golden while capture preserves revision
    state and does not add screenshot evidence.
36. Native descendant selector chains resolve through the existing DOM parent
    links before style cascade, visibility, layout, and paint consume the
    computed result; unsupported combinators remain diagnosed and ignored.
37. Native `overflow: clip` contributes the same bounded rectangular ancestor
    intersection as `overflow: hidden` before paint, viewport projection, and
    point hit-testing consume it; it never creates nested or implicit scroll.
38. Native visible `<br>` elements advance the containing integer flow cursor by
    one fixed line-height floor, reset the inline origin, and create no
    semantic/layout/paint node; hidden breaks are ignored.
39. Native inherited `white-space: pre-line` turns bounded LF/CR/CRLF source
    boundaries into the same hard-break cursor transition while retaining
    collapsed spaces and the default `white-space: normal` behavior.
40. Native inherited `white-space: pre` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and does
    not soft-wrap preformatted segments without changing semantic ownership.
41. Native inherited `white-space: pre-wrap` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and
    splits source runs only at deterministic fixed-cell soft-wrap capacity
    without synthesizing semantic, layout, or paint nodes.
42. Native standard padded base64 `data:text/html` payloads pass through the
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
| bounded local plus HTTP(S) document loader | real external HTML navigation enters the native document owner while local fixtures remain deterministic | no subresources, full charset/security policy, complete cache/cookie semantics, or hostile-content isolation | explicit async path, redirect/size/MIME/charset bounds, typed failures, process-owned session state, and later BE-01/BE-02 promotion gates |
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
| bounded inherited text-presentation rollback | completed extension of the proven layer rollback to inherited `text-transform`, `font-weight`, `font-style`, and `word-break` through independent private candidates without changing public computed values or text/display/raster schemas | four fixed candidate arrays add bounded style-walk state; no generic CSS-wide semantics, font metrics, Unicode mapping, other word-break modes, or browser text parity | share one generic private resolver, keep property-local fallback and finite public values, and test parsing, repeated/unlayered/inline rollback, descendants, root fallback, wrapping, transform, bold/italic raster, capture, hit testing, and semantic/source order together |
| bounded inherited text-spacing rollback | completed extension of the proven layer rollback to inherited `word-spacing` and `letter-spacing` through independent private candidates without changing public computed values or text/display/raster schemas | two fixed candidate arrays add bounded style-walk state; shared parser hardening preserves earlier valid values before invalid later declarations; no generic CSS-wide semantics, negative/relative/percentage/fractional spacing, font metrics, or browser text parity | reuse the generic private resolver, keep spacing candidates property-local, and test standalone/repeated/unlayered/inline rollback, descendants, root fallback, invalid-later preservation, collapsed/preformatted flow, alignment, capture, raster, hit testing, and semantic/source order together |
| bounded inherited vertical-align rollback | completed extension of the proven layer rollback to inherited `vertical-align` through a private candidate without changing public computed values or text/display/raster schemas | one fixed candidate array adds bounded style-walk state; line placement remains tied to the existing fixed-cell geometry owner; no baseline metrics, lengths, percentages, bidi, writing modes, or browser text parity | reuse the generic private resolver, keep the vertical-align candidate property-local, and test standalone/repeated/unlayered/inline rollback, descendants, root fallback, invalid-later preservation, all four finite values, line geometry, capture, raster, hit testing, and semantic/source order together |
| bounded local text-geometry rollback | completed extension of the proven layer rollback to local `text-indent` and `text-overflow` through independent private candidates without changing public computed values or text/display/raster schemas | two fixed candidate arrays add bounded local cascade state; grouping first-line geometry with truncation broadens the focused behavioral surface and retains fixed-cell eligibility limits; no negative/hanging indentation, percentages, font-relative units, marker customization, multiple origins, or browser text parity | share the existing bounded resolver with local fallbacks, keep both candidate sequences property-local, and test same-block/invalid-later/repeated/unlayered/inline rollback together with first-line coordinates, clipped-nowrap ellipsis, text fragments, display/raster output, overflow, hit testing, and semantic/source order |
| bounded local dimension rollback | completed extension of the proven layer rollback to six local pixel dimension owners through independent private candidates without changing public computed values or box/layout/raster schemas | six fixed candidate arrays add bounded local cascade state; grouping width/height and min/max constraints broadens the geometry regression and keeps absent-option fallbacks distinct; no percentages, negative values, intrinsic sizing, aspect-ratio, multiple origins, or browser sizing parity | share the existing local resolver with `None` fallbacks, keep each dimension candidate property-local, and test named/repeated/unlayered/inline rollback, same-block/invalid-later preservation, absent fallback, constraint interactions, box geometry, flex/flow, overflow, capture, raster, hit testing, and semantic/source order together |
| bounded local box-model rollback | completed extension of the proven layer rollback to local box sizing and eight physical padding/margin edge owners, including shorthand/longhand rollback and bounded auto-margin provenance without changing public computed values or artifact schemas | nine edge sequences plus one box-sizing sequence add bounded local cascade state; grouping box conversion, normal-flow margins, and flex auto margins broadens the geometry regression; no percentages, negative/logical edges, margin collapsing, multiple origins, or browser box-model parity | share the generic local resolver, keep each edge/box owner independent with zero/content-box fallbacks, and test shorthand/longhand/repeated/inline rollback with box conversion, flow, flex auto-space, overflow, capture, raster, hit testing, and semantic/source order together; focused and full-native gates passed locally |
| bounded paint-color rollback | completed extension of the proven layer rollback to local `background-color` and inherited `color` through independent private candidates without changing public computed values or artifact schemas | two fixed candidate arrays add bounded paint cascade state; grouping fill and glyph color broadens the display/raster regression; no `currentColor`, gradients, system colors, color spaces, border-color, multiple origins, or browser color parity | share the optional local resolver, keep fill and inherited text candidates property-local, preserve `None`/inherited fallbacks, and test layer order, descendant inheritance, transparent override, fill/text display-list ownership, decoded raster, clipping, opacity, and source/semantic order together |
| bounded overflow rollback | completed extension of the proven layer rollback to local `overflow`, `overflow-x`, and `overflow-y` through independent x/y candidates without changing public computed values or artifact schemas | two fixed candidate arrays add bounded clip cascade state; grouping shorthand expansion with axis-specific rollback broadens the clip/projection/hit-test regression; no nested scrolling, scrollbars, `visible`/`auto`/`scroll` used-value parity, multiple origins, or browser overflow parity | reuse the optional local resolver with visible/no-clip fallbacks, expand shorthand to both axes at the existing declaration boundary, keep x/y candidates independent, and test paint, projection, hit testing, root overflow, capture, and semantic/source order together |
| bounded border-radius rollback | completed extension of the proven layer rollback to the local one-to-four-value integer `border-radius` shorthand without changing public computed values or artifact schemas | one fixed candidate array adds bounded local cascade state; grouping rounded fill, border, point-hit, capture, and raster checks broadens the existing geometry regression; no elliptical/percentage radii, corner longhands, nested clips, anti-aliasing, multiple origins, or browser border-radius parity | reuse the optional local resolver with a zero-corner fallback, keep the existing shorthand expansion/normalization and shared rounded geometry owner, and test named/repeated/unlayered/inline rollback, invalid-later preservation, rounded display/raster/hit behavior, overflow, capture, and semantic/source order together; focused and full-native gates passed locally |
| bounded opacity rollback | completed extension of the proven layer rollback to the local quantized 8-bit `opacity` owner without changing layout, semantic, display-list, or raster schemas | one fixed candidate array adds bounded local cascade state; group-marker/compositing assertions broaden the artifact regression while full-opacity remains the explicit fallback; no inherited opacity, stacking-context/blending parity, filters, animation, multiple origins, or browser opacity parity | reuse the optional local resolver with a `255` fallback, preserve the existing quantization and reduced-opacity group owner, and test named/repeated/unlayered/inline rollback, invalid-later preservation, zero/full/reduced alpha, group replay, layout, hit testing, capture, and semantic/source order together; focused and full-native gates passed locally |
| bounded display/visibility rollback | completed extension of the proven layer rollback to local `display` and `visibility` owners without changing public computed-style or artifact schemas | two fixed candidate arrays add bounded local cascade state; the hidden-subtree gate retains normal-flow/visible fallbacks and existing `display:none`, `visibility:hidden`, and `display:contents` behavior; no inherited visibility, display decomposition, formatting-context parity, table/ruby/flow-root details, animation, multiple origins, or browser display/visibility parity | reuse the generic local resolver with `display:auto`/visible fallbacks, keep display and visibility property-local, and test named/repeated/unlayered/inline rollback, invalid-later preservation, hidden subtree, contents descendants, layout, hit testing, capture, display list, raster, and semantic/source order together; focused and full-native gates passed locally |
| bounded border rollback | completed extension of the proven layer rollback to the four physical `border` side owners without changing public computed-style or artifact schemas | four fixed side candidate arrays add bounded local cascade state; shorthand expansion and side-longhand precedence remain explicit while zero-width/no-paint fallback and existing box-model/paint owners stay unchanged; no logical sides, border-image, gradients, other border styles, animation, multiple origins, or browser border parity | reuse the generic local resolver with a zero-width/no-paint side fallback, preserve shorthand expansion and independent side ownership, and test named/repeated/unlayered/inline rollback, invalid-later preservation, geometry, border commands, capture, decoded raster, point hit testing, and semantic/source order together; focused and full-native gates passed locally |
| bounded border-color rollback | completed extension of the physical border owner with one-to-four-value `border-color`, physical color longhands, private per-side `revert-layer` candidates, and explicit standalone `currentColor` substitution while keeping width/style and zero-width fallback independent | a second component stream and same-block declaration-position metadata add parser/cascade state; complete shorthand `currentColor` is a separate follow-up boundary, while gradients, border-image, animation, multiple origins, `!important` inversion, and browser border parity remain outside | carry existing `border` colors into the private color stream, resolve color after width/style, substitute the local/inherited element color only at computed-style resolution, preserve source order, and verify border geometry/display/raster/hit/semantic consumers together; focused, full-native, library, strict, rustdoc, and two-crate gates passed locally |
| bounded border-width rollback | completed extension of the physical border owner with one-to-four-value `border-width`, physical width longhands, and private per-side `revert-layer` candidates while keeping style/color and zero-width fallback independent | a third component stream and declaration-position metadata add bounded parser/cascade state; no standalone border-style, logical sides, `currentColor`, gradients, border-image, fractional/percentage widths, animation, multiple origins, `!important` inversion, or browser border parity | carry existing `border` widths into the private width stream, resolve width after its own layer rollback, combine it with existing style/color only at computed-style composition, preserve source order, and verify border geometry/display/raster/hit/semantic consumers together; focused, full-native, library, strict, rustdoc, and two-crate gates passed locally |
| bounded border-style rollback | completed extension of the physical border owner with one-to-four-value `border-style`, physical style longhands, and private per-side `revert-layer` candidates while keeping width/color and no-style fallback independent | a third component stream and final component composition add bounded parser/cascade state; no `none`, logical sides, `currentColor`, gradients, border-image, unsupported styles, animation, multiple origins, `!important` inversion, or browser border parity | carry existing `border` styles into the private style stream, allow bounded width/style/color components to combine only after independent rollback, represent no-style as no border side, preserve source order, and verify border geometry/display/raster/hit/semantic consumers together; focused, full-native, library, strict, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally |
| bounded border-style none | completed extension of the physical border owner with explicit `none` in the one-to-four-value style shorthand and physical style longhands through a private no-paint sentinel | the private sentinel blocks lower style candidates while converting to the existing no-side/zero-width result before layout and artifacts; no public paint enum expansion | resolve `none` as a property-local winning style, allow `revert-layer` to expose lower paint, keep width/color unable to resurrect a no-style side, and verify no-side geometry, absent border commands, raster, hit, semantic order, public-surface stability, focused/full-native/library, strict, rustdoc, two-crate, static documentation, and bounded cleanup gates; all passed locally |
| bounded border-style hidden | completed extension of the physical border owner with explicit `hidden` in the one-to-four-value style shorthand and physical style longhands through a distinct private no-paint sentinel | the private sentinel preserves a future table-conflict distinction while current composition treats hidden like none; no public paint enum expansion and no table layout owner | resolve hidden as a property-local winning style, allow revert-layer to expose lower paint, keep width/color unable to resurrect a no-style side, and verify no-side geometry, absent border commands, raster, hit, semantic order, public-surface stability, and private/public separation; focused/full-native/library, strict, rustdoc, two-crate, static documentation, and bounded cleanup gates passed locally |
| bounded painted border styles | completed batched extension of the physical border owner with `double`, `groove`, `ridge`, `inset`, and `outset` public paint values and deterministic software replay | one batch expands the public enum and raster branch surface; integer-pixel stripe/shade rules are deliberately bounded and do not cover anti-aliasing, browser color/fidelity, or table conflict resolution | define explicit double stripe and shade/edge rules, reuse the existing physical cascade and display command, and test parser/public values, declaration precedence, raster pixels, clipping, capture, point hit, and semantic order together; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally |
| bounded omitted-component border none | completed exact `none` support for complete and physical border shorthands through a private declaration wrapper, preserving the existing no-paint style sentinel and all public/artifact schemas | exact-only syntax keeps the high-value no-paint gap bounded; it does not claim `medium`, `currentColor`, arbitrary style-only forms, CSS-wide reset machinery, or table conflict semantics; no synthetic width/color candidates means the existing bounded component rollback rule remains explicit | parse exact `none`, project it only into the private style stream with declaration order, preserve complete values and standalone rollback, and verify blocking/rollback, invalid preservation, no-side geometry, artifacts, point hit, semantic order, and public-surface stability; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally |
| bounded omitted-component border hidden | completed exact `hidden` support for complete and physical border shorthands through the existing private hidden style sentinel, preserving the future table-conflict distinction and all public/artifact schemas | exact-only syntax closes the remaining high-value omitted-component style gap without claiming arbitrary defaults, CSS-wide reset machinery, or current table conflict behavior; no synthetic width/color candidates keep the bounded component rollback rule explicit | parse exact `hidden`, project it only into the private hidden style stream with declaration order, preserve the hidden/none private distinction through current resolution, and verify blocking/rollback, invalid preservation, no-side geometry, artifacts, point hit, semantic order, and public-surface stability; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally |
| bounded complete border hidden | completed bounded complete `Npx hidden color` support for complete and physical border shorthands through a private complete-hidden declaration that carries declared width/color while projecting hidden style | retaining width/color privately preserves future table-conflict inputs, but current non-table composition suppresses the side and the public paint enum remains unchanged; no arbitrary omitted defaults or full table model | parse exact complete hidden values, project width/color at declaration order and hidden style through independent streams, preserve omitted hidden/none behavior, and verify private/public separation, rollback, invalid preservation, no-side current artifacts, and unchanged schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and static documentation gates passed locally |
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

| `native-engine-160` | completed bounded complete `Npx none color` values for `border` and physical border shorthands, preserving private declared width/color candidates while projecting none style through the existing private sentinel and keeping public/artifact schemas unchanged; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-159` | arbitrary omitted-component defaults, other CSS-wide reset keywords, logical sides, table layout/conflict resolution, `currentColor`, gradients, border-image, animation, multiple origins, `!important` inversion, and browser-wide CSS border conformance |
| `native-engine-161` | completed standalone physical `border-color` and physical color-longhand `currentColor` substitution through private declaration state, resolved from the existing local/inherited element color while preserving public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-160` | complete border-shorthand `currentColor`, arbitrary omitted defaults, CSS-wide reset machinery, logical sides, gradients, system colors, color spaces, percentages, animation, multiple origins, `!important` inversion, table conflict resolution, and browser-wide CSS color/border conformance |
| `native-engine-162` | completed complete physical `Npx <style> currentColor` values for painted, `none`, and `hidden` border shorthand forms through private deferred-color declaration state, preserving concrete public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates passed locally | `native-engine-161` | omitted width/style defaults, CSS-wide reset machinery, logical sides, gradients, system colors, color spaces, percentages, animation, multiple origins, `!important` inversion, collapsed-table conflict resolution, and browser-wide CSS color/border conformance |
| `native-engine-163` | completed bounded `background-color: currentColor` substitution through private deferred-color declaration state, resolved from the local or inherited `color` owner at computed-style construction while preserving concrete public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, package, formatting, static documentation, workspace all-target/all-feature, and bounded cleanup gates passed locally | `native-engine-162` | `color: currentColor`, gradients, images, system colors, color spaces, percentages, CSS-wide reset machinery, multiple origins, animation, and browser-wide CSS color conformance |
| `native-engine-164` | completed bounded local `text-decoration-color: currentColor` substitution through private deferred-color declaration state, resolved from the local or inherited `color` owner at computed-style construction while preserving concrete public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, package, formatting, static documentation, workspace all-target/all-feature, and bounded cleanup gates passed locally | `native-engine-163` | `color: currentColor`, gradients, images, system colors, color spaces, percentages, CSS-wide reset machinery, multiple origins, animation, and browser-wide text-color conformance |
| `native-engine-165` | completed bounded local `color: currentColor` substitution through private deferred-color declaration state, resolved from the already-computed inherited color or bounded initial black fallback while preserving concrete public/artifact schemas; focused/full-native/library, strict Clippy, rustdoc, two-crate, package, formatting, static documentation, workspace all-target/all-feature, and bounded cleanup gates passed locally | `native-engine-164` | gradients, images, system colors, color spaces, percentages, custom-property graphs, CSS-wide reset machinery beyond existing `revert-layer`, multiple origins, animation, and browser-wide color conformance |
| `native-engine-166` | completed bounded inherited `color` CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state while preserving concrete public/artifact schemas; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-165` | multiple origins, `!important` inversion, gradients, images, system colors, color spaces, percentages, custom-property graphs, animation, and browser-wide color conformance |
| `native-engine-167` | completed bounded non-inherited `background-color` CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only `inherit` copying the parent optional fill and reset forms preserving the existing no-fill `None` fallback; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-166` | multiple origins, `!important` inversion, gradients, images, system colors, color spaces, percentages, custom-property graphs, animation, and browser-wide background conformance |
| `native-engine-168` | completed bounded local `text-decoration-color` CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only explicit `inherit` copying the parent's effective decoration color and reset forms resolving to the current element color; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-167` | multiple origins, `!important` inversion, gradients, images, system colors, color spaces, percentages, custom-property graphs, animation, and browser-wide text-decoration conformance |
| `native-engine-169` | completed bounded local physical `border-color` and four physical color-longhand CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only explicit `inherit` copying the parent's effective per-side color, reset forms resolving to the current element color, and omission retaining the black fallback; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-168` | multiple origins, `!important` inversion, gradients, images, system colors, color spaces, percentages, custom-property graphs, animation, logical sides, table conflict resolution, and browser-wide border conformance |
| `native-engine-170` | completed bounded local physical `border-width` and four physical width-longhand CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only explicit `inherit` copying the parent's effective per-side width and zero-width omission/reset fallbacks; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-169` | multiple origins, `!important` inversion, percentages, medium/thin/thick defaults, gradients, images, animation, logical sides, table conflict resolution, and browser-wide border conformance |
| `native-engine-171` | completed bounded local physical `border-style` and four physical style-longhand CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only explicit `inherit` copying effective parent styles and reset/omission preserving the private no-style fallback; focused/full-native/library, strict, package, static, workspace, and cleanup gates passed locally | `native-engine-170` | multiple origins, `!important` inversion, logical sides, table conflict resolution, border conflict/collapse, gradients, border-image, animation, unsupported styles, and browser-wide border conformance |
| `native-engine-172` | completed bounded local physical `border-radius` CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` through private declaration state, with only explicit `inherit` copying an effective parent radius and reset/omission preserving the default zero-corner fallback; implementation `b0bbe45a` | `native-engine-171` | multiple origins, `!important` inversion, corner longhands, elliptical/percentage radii, logical sides, table conflict resolution, gradients, border-image, animation, and browser-wide border conformance |
| `native-engine-173` | completed bounded complete physical `border` and four side-border shorthand CSS-wide keyword family: case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert` projected through the existing private width/style/color component streams, preserving concrete and omitted-component forms, independent composition, geometry, and artifacts; implementation `f5f53cec` | `native-engine-172` | multiple origins, `!important` inversion, logical sides, table conflict resolution, gradients, border-image, animation, arbitrary omitted defaults, and browser-wide border conformance |
| `native-engine-174` | completed bounded logical `border-block`, `border-inline`, logical start/end shorthands, and their width/style/color component longhands projected into the existing physical streams through resolved horizontal-tb ltr/rtl direction; no public schema or crate change; implementation `f6953813`, strict-cascade cleanup `23b09864` | `native-engine-173` | vertical writing modes, logical radius, border-image, gradients, table conflict resolution, multiple origins, `!important` inversion, arbitrary values, and browser-wide logical-border conformance |
| `native-engine-175` | completed bounded physical `border-radius` corner longhands with private per-corner candidate streams, CSS-wide forms, shorthand/longhand source-order composition, and unchanged rounded consumers; implementation `2b082ddf`, resolver/test-shape correction `e6f3259d` | `native-engine-174` | logical corner longhands, writing-mode mapping, text orientation, percentages, elliptical radii, multiple origins, `!important` inversion, animation, and browser-wide logical-radius conformance |
| `native-engine-176` | completed bounded logical `border-start-start-radius`, `border-start-end-radius`, `border-end-start-radius`, and `border-end-end-radius` longhands projected through resolved horizontal-tb ltr/rtl direction into the physical per-corner streams; implementation `6543b2b6`; focused/full-native/library, strict, package, static, workspace, security/fuzz, and cleanup gates passed locally | `native-engine-175` | vertical writing modes, text orientation, percentages, elliptical radii, multiple origins, `!important` inversion, animation, and browser-wide logical-radius conformance |
| `native-engine-177` | completed bounded author-origin `!important` priority for the complete physical and horizontal-tb logical radius family, with important-over-normal ordering and reversed named-layer priority in a private doubled radius cascade partition; no public schema or consumer change; implementation `46f6499a`; focused/full-native/library, strict Clippy, and rustdoc gates passed locally | `native-engine-176` | generic `!important` for other properties, multiple origins, transitions, animations, vertical writing modes, text orientation, percentages, elliptical radii, and browser-wide logical-radius conformance |
| `native-engine-178` | completed bounded author-origin `!important` priority for `background-color`, inherited `color`, and `text-decoration-color`, using private reversed named-layer partitions with existing value/fallback/paint owners unchanged; implementation `1292538c`; focused/full-native/library, strict Clippy, and warning-denied rustdoc gates passed locally | `native-engine-177` | border color and other properties, multiple origins, transitions, animations, vertical writing modes, text orientation, percentages, elliptical radii, and browser-wide conformance |
| `native-engine-179` | completed bounded author-origin `!important` priority for standalone physical `border-color` and four physical color longhands, using private reversed named-layer partitions with existing four-side color/width/style/artifact owners unchanged; implementation `ed1cda27`; focused/full-native/library, strict Clippy, and warning-denied rustdoc gates passed locally | `native-engine-178` | complete/side border shorthands, logical border-color, border width/style, multiple origins, transitions, animations, vertical writing modes, text orientation, percentages, elliptical radii, and browser-wide conformance |
| `native-engine-180` | completed bounded author-origin `!important` priority for the six supported horizontal-tb logical border-color declarations, preserving `ltr`/`rtl` projection into physical sides through a private reversed named-layer partition; implementation `491f65fe`, strict-lint follow-up `100d1888`; focused/full-native/library, strict Clippy, and warning-denied rustdoc gates passed locally | `native-engine-179` | complete border shorthands, border width/style, vertical writing modes, multiple origins, transitions, animations, percentages, elliptical radii, and browser-wide conformance |

| `native-engine-181` | completed bounded author-origin `!important` priority for the standalone physical `border-width` shorthand and four physical width longhands, preserving per-side expansion/resolution and existing width/style/color consumers through a private reversed named-layer partition; implementation `436dd02a`; focused/full-native/library, strict Clippy, and warning-denied rustdoc gates passed locally | `native-engine-180` | complete/side border shorthands, logical border width, border style/color, vertical writing modes, multiple origins, transitions, animations, percentages, elliptical radii, and browser-wide conformance |

| `native-engine-182` | completed bounded author-origin `!important` priority for the standalone physical `border-style` shorthand and four physical style longhands, preserving per-side expansion/resolution, private `none`/`hidden` distinctions, and existing width/style/color consumers through a private reversed named-layer partition; implementation `008a5766`; focused/full-native/library, strict Clippy, warning-denied rustdoc, and current documentation gates passed locally | `native-engine-181` | complete/side border shorthands, logical border style, border width/color, vertical writing modes, multiple origins, transitions, animations, percentages, elliptical radii, and browser-wide conformance |

| `native-engine-183` | completed bounded author-origin `!important` priority for the six supported horizontal-tb logical border-width declarations, preserving `ltr`/`rtl` projection into physical width streams through a private reversed named-layer partition; implementation `6013d0c5`; focused/full-native/library, strict Clippy, warning-denied rustdoc, and current documentation gates passed locally | `native-engine-182` | complete/side border shorthands, logical border style, vertical writing modes, multiple origins, transitions, animations, percentages, elliptical radii, and browser-wide conformance |

| `native-engine-184` | completed bounded author-origin `!important` priority for the six supported horizontal-tb logical border-style declarations, preserving private `none`/`hidden` behavior and `ltr`/`rtl` projection into physical style streams through a private reversed named-layer partition; implementation `26fd347a`; focused/full-native/library, strict Clippy, warning-denied rustdoc, and current documentation gates passed locally | `native-engine-183` | complete/side border shorthands, logical border width/color, vertical writing modes, multiple origins, transitions, animations, percentages, elliptical radii, and browser-wide conformance |

| `native-engine-185` | completed bounded author-origin `!important` priority for complete physical `border` and four physical side-border shorthands by carrying one private importance bit per side into the existing doubled width/style/color component streams; preserved independent component composition, CSS-wide/omitted `none`/`hidden`/`revert-layer` behavior, public schemas, and the two-crate boundary; implementation `bfcb6dc9`; focused/full-native/library, strict Clippy, warning-denied rustdoc, current documentation, and cleanup evidence recorded locally | `native-engine-184` | logical complete shorthands, vertical writing modes, table conflict resolution, border-image, gradients, multiple origins, transitions, animations, percentages, elliptical radii, arbitrary CSS border values, and browser-wide conformance |

| `native-engine-186` | completed bounded author-origin `!important` priority for the six supported horizontal-tb logical complete/side border shorthands by carrying one private importance bit per logical side into the existing doubled width/style/color component streams before resolved `ltr`/`rtl` projection; preserved independent component composition, CSS-wide/omitted `none`/`hidden`/`revert-layer` behavior, public schemas, and the two-crate boundary; implementation `274441e1`, strict-lint helper correction `758b891a`; focused/full-native/library, strict Clippy, warning-denied rustdoc, current documentation, and cleanup evidence recorded locally | `native-engine-185` | vertical writing modes, logical radius, table conflict resolution, border-image, gradients, multiple origins, transitions, animations, percentages, elliptical radii, arbitrary logical-border values, and browser-wide conformance |
| `native-engine-187` | completed bounded author-origin `!important` priority for the supported text-flow and text-decoration declarations through one private doubled text cascade partition, preserving important-over-normal ordering, reversed named-layer priority, inline important precedence, invalid-later preservation, inherited/local fallbacks, and `revert-layer` rollback with existing layout, decoration, raster, capture, hit, diagnostics, schemas, and two-crate boundaries unchanged; implementation `65883117`; focused/full-native/library, strict Clippy, warning-denied rustdoc, current documentation, and cleanup evidence recorded locally | `native-engine-186` | display/visibility/opacity, flex/gap, dimensions/box model, overflow, dependencies, multiple origins, transitions, animations, vertical writing modes, and browser-wide CSS conformance |

| `native-engine-188` | completed bounded author-origin `!important` priority for local `display`, `visibility`, and `opacity` through one private doubled local cascade partition, preserving important-over-normal ordering, reversed named-layer priority, inline important precedence, invalid-later preservation, and `revert-layer` rollback through existing hidden-subtree, semantic, layout, display-list, raster, capture, and point-hit owners; implementation `ca0b47bd`; scoped check, focused/full-native integration, strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-187` | flex/gap, dimensions/box model, overflow, other properties, dependencies, multiple origins, transitions, animations, vertical writing modes, and browser-wide CSS conformance |

| `native-engine-189` | completed bounded author-origin `!important` priority for the normal-only flex and gap declarations through private doubled flex candidate arrays and important-aware gap partitions, carrying priority through `place-content`, `flex-flow`, and `flex` shorthand expansion while preserving important-over-normal ordering, reversed named-layer priority, inline precedence in the unlayered important bucket, invalid-later preservation, independent gap-axis source order, `revert-layer` rollback, existing layout/artifact consumers, public schemas, and the two-crate boundary; implementation `94724ab0`; scoped check, focused/full-native integration (227/227), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-188` | dimensions/box model, overflow priority, other properties, dependencies, multiple origins, transitions, animations, vertical writing modes, and browser-wide CSS conformance |

| `native-engine-190` | completed bounded author-origin `!important` priority for the six normal-only local dimension declarations through private doubled candidate arrays and per-property importance bits, preserving important-over-normal ordering, reversed named-layer priority, inline precedence in the unlayered important bucket, invalid-later preservation, independent dimension streams, `revert-layer` rollback, min/max constraints, box geometry, clipping, hit testing, display-list, raster, PNG capture, public schemas, and the two-crate boundary; implementation `d5cc6e6b`; scoped check, focused/full-native integration (228/228), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-189` | box-model declarations, overflow priority, other properties, dependencies, multiple origins, transitions, animations, vertical writing modes, and browser-wide CSS conformance |

| `native-engine-191` | completed bounded author-origin `!important` priority for normal-only physical `box-sizing`, padding, and margin shorthand/longhand edges through private doubled candidate streams and per-edge importance bits, preserving important-over-normal ordering, reversed named-layer priority, inline precedence in the unlayered important bucket, per-edge source order, invalid-later preservation, `auto` margin provenance, `revert-layer` rollback, content-box/border-box geometry, normal-flow/flex placement, overflow clipping, display-list, raster, PNG capture, point-hit, semantic/source order, public schemas, and the two-crate boundary; implementation `43e5f8c2`; scoped check, focused/full-native integration (229/229), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-190` | logical edges, overflow priority, other properties, dependencies, multiple origins, transitions, animations, vertical writing modes, and browser-wide CSS conformance |

| `native-engine-192` | completed bounded author-origin `!important` priority for normal-only `overflow`, `overflow-x`, and `overflow-y` through private doubled x/y candidate streams and shorthand/x/y importance bits, preserving important-over-normal ordering, reversed named-layer priority, inline precedence in the unlayered important bucket, invalid-later preservation, independent axis projection, `revert-layer !important` rollback, clip/root-overflow/layout/display-list/raster/PNG-capture/point-hit/semantic consumers, public schemas, and the two-crate boundary; implementation `fc2c461e`; scoped check, focused/full-native integration (230/230), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-191` | nested scrolling, scrollbars, visible/auto/scroll used-value parity, logical writing modes, other properties, dependencies, multiple origins, transitions, animations, and browser-wide CSS conformance |

| `native-engine-193` | completed bounded horizontal-tb logical `padding-block`, `padding-inline`, `margin-block`, and `margin-inline` shorthands plus block/inline start/end longhands projected through resolved `ltr`/`rtl` direction into the existing physical per-edge candidate streams, preserving important-over-normal ordering, reversed named-layer priority, inline-important precedence, invalid-later behavior, same-rule physical/logical source order, `auto` margin provenance, `revert-layer` rollback, geometry, normal-flow/flex, overflow, display-list, raster, PNG-capture, point-hit, semantic consumers, public schemas, and the two-crate boundary; implementation `3e78c246`; scoped check, focused/full-native integration (231/231), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-192` | logical `box-sizing`, vertical writing modes, percentages, negative lengths, margin collapsing, positioning, CSS-wide reset keywords, other properties, dependencies, multiple origins, transitions, animations, and browser-wide CSS conformance |

| `native-engine-194` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for `box-sizing`, physical padding/margin shorthands and longhands, and the horizontal-tb logical padding/margin family, normalizing to existing content-box and zero-edge fallbacks while preserving separate `revert-layer` rollback, important-over-normal ordering, reversed named-layer priority, inline-important precedence, invalid-later behavior, ltr/rtl projection, geometry, normal-flow/flex, overflow, display-list, raster, PNG-capture, point-hit, semantic consumers, public schemas, and the two-crate boundary; implementation `5477fb79`; scoped check, focused/full-native integration (232/232), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-193` | `inherit`, percentages, negative lengths, margin collapsing, positioning, vertical writing modes, additional logical properties, multiple origins, transitions, animations, and browser-wide CSS-wide conformance |

| `native-engine-195` | completed bounded standalone case-insensitive `inherit` for `box-sizing`, physical padding/margin shorthands and longhands, and the supported horizontal-tb logical padding/margin family; physical values copy parent effective edges, box-sizing, and private margin `auto` provenance, while logical values read the parent in its resolved `ltr`/`rtl` direction before projecting into the child; root fallbacks, omitted-property non-inheritance, important/source-order behavior, `revert-layer` rollback, geometry, normal-flow/flex, display-list, raster, PNG-capture, point-hit, semantic consumers, public schemas, and the two-crate boundary remain bounded; implementation `0caad64b`; scoped check, focused/full-native integration (233/233), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-194` | percentages, negative lengths, margin collapsing, positioning, vertical writing modes, additional logical properties, multiple origins, transitions, animations, and browser-wide CSS conformance |

| `native-engine-196` | completed bounded standalone case-insensitive `inherit` for `width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`; explicit values copy the parent's computed optional pixel value, including a parent `None`/auto result, while omitted dimensions remain local and do not inherit; important/source-order behavior, invalid-later preservation, `revert-layer`, min/max, content-box/border-box, normal-flow/flex, display-list, raster, PNG-capture, point-hit, semantic consumers, public schemas, and the two-crate boundary remain bounded; implementation `fd6ee415`; scoped check, focused unit/integration, full-native integration (234/234), strict Clippy, warning-denied rustdoc, formatting, static documentation truth/coverage/depth, feature parity, TUI shortcut, and version-sync passed locally | `native-engine-195` | percentages, negative lengths, intrinsic sizing, aspect ratio, margin collapsing, positioning, vertical writing modes, additional origins, transitions, animations, and browser-wide CSS sizing conformance |

| `native-engine-197` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for `width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`; winning reset candidates resolve to the existing optional `None`/auto fallback without falling through, while `revert-layer` remains the separate lower-layer rollback candidate; omission, explicit `inherit`, important/source-order behavior, invalid-later preservation, min/max, content-box/border-box, normal-flow/flex, display-list, raster, PNG capture, point-hit, semantic consumers, public schemas, and the two-crate boundary remain bounded; implementation `a0e102b5`; scoped check, focused unit/integration, full-native integration (235/235), strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-196` | percentages, negative lengths, intrinsic sizing, aspect ratio, margin collapsing, positioning, vertical writing modes, additional origins, transitions, animations, and browser-wide CSS sizing conformance |
| `native-engine-198` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited `white-space`, positive-pixel `line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`, `vertical-align`, `word-spacing`, and `letter-spacing`; parent/root fallback, terminal reset behavior, invalid-later preservation, existing cascade priority, `revert-layer` distinction, layout, display-list, raster, PNG capture, point-hit, semantic, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `95a988d0`; locked scoped check, focused unit/integration, full-native integration (236/236), feature-library, strict Clippy, warning-denied rustdoc, and formatting passed locally | `native-engine-197` | unitless/relative/percentage line-height, font metrics/shaping, Unicode/bidi/writing modes, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide text conformance |
| `native-engine-199` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited `text-align`, `text-align-last`, `text-justify`, and `direction`; parent/root fallback, terminal reset behavior, invalid-later preservation, logical `ltr`/`rtl` projection, `revert-layer` distinction, layout, display-list, raster, PNG capture, point-hit, semantic, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `4771f352`; locked scoped check, focused cascade/parser/integration, full-native integration (237/237), feature library (1,020 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package/install, security, static audits, and formatting passed locally | `native-engine-198` | bidi/shaping, vertical writing modes, additional origins, transitions, animations, generic CSS-wide machinery, other logical properties, and browser-wide alignment/direction conformance |
| `native-engine-200` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited `text-decoration-style`; parent/root fallback, `solid` initial behavior, terminal reset semantics, invalid-later preservation, finite decoration-style values, `revert-layer` distinction, display-list, fixed-cell raster, PNG capture, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `62525ec4`; locked scoped check, focused parser/cascade/integration, full-native integration (238/238), feature library (1,020 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace check, fuzz, package, security, static audits, and formatting passed locally | `native-engine-199` | decoration geometry, thickness/offset/skip behavior, multiple origins, transitions, animations, generic CSS-wide machinery, browser font metrics, and browser-wide text-decoration conformance |
| `native-engine-201` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited `text-decoration-thickness`; parent/root fallback, `1px` initial behavior, terminal reset semantics, invalid-later preservation, finite `1px|2px|3px|4px` values, `revert-layer` distinction, display-list, fixed-cell raster, PNG capture, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `ee7dae83`; locked scoped check, focused parser/cascade/integration, full-native integration (239/239), feature library (1,021 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-200` | `auto`, `from-font`, percentages, fractional/out-of-range lengths, decoration geometry changes, multiple origins, transitions, animations, generic CSS-wide machinery, browser font metrics, and browser-wide text-decoration conformance |
| `native-engine-202` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited `text-decoration-skip-ink`; parent/root fallback, finite `auto|none` behavior, terminal reset semantics, invalid-later preservation, `revert-layer` distinction, same-run glyph intersection, display-list, fixed-cell raster, PNG capture, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `65e3a76d`; locked scoped check, focused parser/cascade/integration, full-native integration (240/240), feature library (1,022 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-201` | `auto`/`none` heuristic expansion, font metrics, shaping, fragment-level ink metrics, additional origins, transitions, animations, generic CSS-wide machinery, layout/geometry changes, and browser-wide text-decoration conformance |
| `native-engine-203` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for the inherited `text-decoration-line` three-bit owner and existing bounded `text-decoration` shorthand route; parent/root fallback, finite line combinations, `none` initial behavior, terminal reset semantics, invalid-later preservation, `revert-layer` distinction, shorthand/longhand source order, display-list, fixed-cell raster, PNG capture, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `73c00616`; locked scoped check, focused parser/cascade/integration, full-native integration (241/241), feature library (1,023 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-202` | full text-decoration shorthand expansion, `all`, additional origins, transitions, animations, generic CSS-wide machinery, layout/geometry changes, font metrics, and browser-wide text-decoration conformance |
| `native-engine-204` | completed bounded standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin `revert` for inherited signed `text-underline-offset:-4px..=4px`; parent/root fallback, `0px` initial behavior, terminal reset semantics, invalid-later preservation, `revert-layer` distinction, important/source order, underline-only movement, overline/line-through preservation, display-list, fixed-cell raster, PNG capture, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `c82773e2`; locked scoped check, focused parser/cascade/integration, full-native integration (242/242), feature library (1,024 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-203` | `auto`, percentages, fractional/font-derived values, dimensions outside the bounded range, additional origins, transitions, animations, generic CSS-wide machinery, decoration-origin propagation, geometry/layout changes, and browser-wide text-decoration conformance |
| `native-engine-205` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `gap`, `row-gap`, and `column-gap`; zero-gap reset fallback, finite non-negative pixel values, `revert-layer` distinction, shorthand/longhand axis projection, important/source order, invalid-later preservation, flex placement, display-list, fixed-cell raster, PNG capture, hit testing, diagnostics, public schemas, and the two-crate boundary remain bounded; implementation `46128c2e`; locked scoped check, focused parser/cascade/integration, full-native integration (243/243), feature library (1,025 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-204` | `inherit`, parent-gap propagation, percentages, fractional/intrinsic values, dimensions outside the bounded range, additional origins, transitions, animations, generic CSS-wide machinery, grid track sizing, and browser-wide gap conformance |
| `native-engine-206` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `text-indent` and `text-overflow`; reset forms resolve to finite `0px` and `clip` fallbacks, preserving non-negative fixed-pixel indentation, `clip|ellipsis`, `revert-layer` distinction, important/source order, invalid-later preservation, first-line layout, eligible truncation, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `b7bd9ace`; locked scoped check, focused parser/cascade/integration, full-native integration (244/244), feature library (1,026 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, fuzz, package, security, static audits, and formatting passed locally | `native-engine-205` | `inherit`, parent propagation, negative/fractional/percentage indentation, additional origins, transitions, animations, generic CSS-wide machinery, ellipsis layout expansion, multi-line/nested-inline truncation, and browser-wide text/overflow conformance |
| `native-engine-207` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `flex`, `flex-grow`, `flex-shrink`, and `flex-basis`; reset forms expand through the existing private component streams to finite `0 1 auto`, preserving finite shorthand expansion, non-negative fixed-pixel basis values, `auto`, `revert-layer` distinction, important/source order, invalid-later preservation, flex placement, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `1a43c150`; locked scoped check, focused flex parser/cascade (25 tests), public integration, full-native integration (245/245), feature library (1,027 passed, 1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests, doctests, package, security, static audits, and formatting passed locally; the preceding task's six-target nightly fuzz certification remains current because this slice changes only CSS flex parsing/cascade | `native-engine-206` | `inherit`, parent propagation, percentages, negative/fractional/intrinsic basis values, additional origins, transitions, animations, generic CSS-wide machinery, direction/wrap/alignment/order, and browser-wide flex conformance |
| `native-engine-208` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `flex-direction`, `flex-wrap`, and `flex-flow`; reset forms project through the existing private component streams to finite `row`/`nowrap`, preserving finite row/column direction values, finite wrap modes, shorthand projection, `revert-layer` distinction, important/source order, invalid-later preservation, row/column mapping, wrapping, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `fcadc99c`; locked scoped check, focused flex parser/cascade (27 tests), public integration, full-native integration (246/246), feature library (1,029 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, and formatting passed locally | `native-engine-207` | `inherit`, parent propagation, additional origins, transitions, animations, generic CSS-wide machinery, justify/alignment/order, inherited `direction`, percentages, and browser-wide flex conformance |
| `native-engine-209` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local flex-item `order`; reset forms resolve through the existing local resolver to finite `0`, preserving signed `-1024..=1024`, `revert-layer`, important/source order, invalid-later preservation, stable visual/source order, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `0bfbc8f9`; locked scoped check, focused flex parser/cascade (28 tests), public integration, full-native integration (247/247), feature library (1,030 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, and formatting passed locally | `native-engine-208` | `inherit`, parent propagation, percentages, fractional values, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-210` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `justify-content`; reset forms reuse the existing bounded `flex-start` fallback, preserving finite distribution values, `revert-layer`, important/source order, invalid-later preservation, free-space distribution, row/column mapping, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `e9ac0b1c`; locked scoped check, focused flex parser/cascade (29 tests), public integration, full-native integration (248/248), feature library (1,031 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-209` | `inherit`, percentages, `place-content`, other alignment owners, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-211` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `align-items`; reset forms reuse the existing bounded `flex-start` fallback, preserving finite cross-axis values, `revert-layer`, important/source order, invalid-later preservation, cross-axis placement, `align-self` overrides, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `29e23b4a`; locked scoped check, focused flex parser/cascade (30 tests), public integration, full-native integration (249/249), feature library (1,032 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-210` | `inherit`, percentages, baseline/safe/unsafe forms, `align-self`, `align-content`, `place-content`, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-212` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `align-self`; reset forms reuse the existing bounded `auto` fallback and continue through parent `align-items`, preserving finite item values, `revert-layer`, important/source order, invalid-later preservation, complete-subtree movement, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `0a624642`; locked scoped check, focused flex parser/cascade (30 tests) plus dedicated align-self cascade (1 test), public integration, full-native integration (250/250), feature library (1,033 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-211` | `inherit`, percentages, baseline/safe/unsafe forms, `align-content`, `place-content`, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-213` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `align-content`; reset forms reuse the existing bounded `flex-start` fallback, preserving finite wrapped-line distribution values, `revert-layer`, terminal reset behavior, invalid-later preservation, important/source order, wrapped-line distribution, item alignment, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `337da7c5`; locked scoped check, focused flex parser/cascade (31 tests), public integration, full-native integration (251/251), feature library (1,034 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-212` | `inherit`, percentages, baseline/safe/unsafe forms, `place-content`, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-214` | completed bounded standalone case-insensitive `initial`, `unset`, and one-author-origin `revert` for local `place-content`; reset forms project through the existing bounded `flex-start` fallbacks for both `align-content` and `justify-content`, preserving finite one-/two-value expansion, `revert-layer`, terminal reset behavior, invalid-later preservation, important/source order, wrapped-line distribution, main-axis placement, item alignment, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, public schemas, and the two-crate boundary; implementation `24ae15a7`; locked scoped check, focused flex parser/cascade (31 tests) plus dedicated place-content cascade (1 test), public integration, full-native integration (252/252), feature library (1,035 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-213` | mixed reset/finite tokens, `inherit`, percentages, baseline/safe/unsafe forms, `place-items`, additional origins, transitions, animations, generic CSS-wide machinery, and browser-wide flex conformance |
| `native-engine-215` | completed bounded standalone case-insensitive `align-content:inherit` through the existing private parent-style chain; omitted `align-content` remains non-inherited with the bounded `flex-start` fallback, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, wrapped-line placement, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `a66f474b`; locked scoped check, focused align-content parser/cascade (4 tests), focused flex parser/cascade (31 tests), public integration, full-native integration (253/253), feature library (1,036 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-214` | percentages, baseline/safe/unsafe forms, other alignment owners, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-216` | completed bounded standalone case-insensitive `justify-content:inherit` through the existing private parent-style chain; omitted `justify-content` remains non-inherited with the bounded `flex-start` fallback, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, main-axis placement, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `c3dc1faf`; locked scoped check, focused justify-content parser/cascade (4 tests), public reset/inheritance integration, full-native integration (254/254), feature library (1,037 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-215` | percentages, baseline/safe/unsafe forms, other alignment owners, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-217` | completed bounded standalone case-insensitive `align-items:inherit` through the existing private parent-style chain; omitted `align-items` remains non-inherited with the bounded `flex-start` fallback, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, cross-axis placement, `align-self` overrides, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `feb6b607`; locked scoped check, focused align-items parser/cascade (4 tests), public reset/inheritance integration (7 tests), full-native integration (255/255), feature library (1,038 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-216` | percentages, baseline/safe/unsafe forms, other alignment owners, additional origins, transitions, animations, generic CSS-wide machinery, `align-self` inheritance, `place-items`, grid, browser parity, and browser-wide flex conformance |
| `native-engine-218` | completed bounded standalone case-insensitive `align-self:inherit` through the existing private parent-style chain; omitted `align-self` remains local `auto`, explicit `auto` continues to delegate through the containing `align-items`, and inherited finite values remain explicit item overrides, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, cross-axis placement, complete-subtree movement, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `52adc7d1`; locked scoped check, focused align-self parser/cascade (4 tests), public reset/inheritance integration (5 tests), full-native integration (256/256), feature library (1,039 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-217` | percentages, baseline/safe/unsafe forms, other alignment owners, additional origins, transitions, animations, generic CSS-wide machinery, `place-items`, grid, browser parity, and browser-wide flex conformance |
| `native-engine-219` | completed bounded standalone case-insensitive `place-content:inherit` through the existing private `align-content` and `justify-content` parent-style owners; both computed parent components copy only when the shorthand is explicitly authored while omitted `place-content` remains local, preserving finite one-/two-value expansion, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, wrapped-line distribution, main-axis placement, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `23703765`; locked scoped check, focused place-content parser/cascade (5 tests), public inheritance integration (1 test), full-native integration (257/257), feature library (1,040 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-218` | percentages, baseline/safe/unsafe forms, `place-items`, grid, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |

| `native-engine-220` | completed bounded standalone case-insensitive `flex-direction:inherit` through the existing private parent-style chain; the computed parent direction copies only when explicitly authored while omitted `flex-direction` remains local with the bounded `row` fallback, preserving finite row/column mapping, `flex-flow`/longhand component precedence, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, wrapping eligibility, gap and margin mapping, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `1f64f232`; locked scoped check, focused flex-direction parser/cascade (4 tests), public inheritance integration (1 test), full-native integration (258/258), feature library (1,041 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-219` | percentages, `flex-wrap:inherit`, `flex-flow:inherit`, `place-items`, grid, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-221` | completed bounded standalone case-insensitive `flex-wrap:inherit` through the existing private parent-style chain; the computed parent wrap mode copies only when explicitly authored while omitted `flex-wrap` remains local with the bounded `nowrap` fallback, preserving finite wrap modes, `flex-flow`/longhand component precedence, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, row/column wrapping eligibility, line formation and reverse stacking, line sizing, gap and margin mapping, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `ebeb8443`; locked scoped check, focused flex-wrap parser/cascade (3 tests), public inheritance integration (1 test), full-native integration (259/259), feature library (1,042 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-220` | `flex-flow:inherit`, percentages, `place-items`, grid, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-222` | completed bounded standalone case-insensitive `flex-flow:inherit` by projecting to the existing private `flex-direction` and `flex-wrap` inheritance owners; both computed parent components copy only when the shorthand is explicitly authored while omitted `flex-flow` remains local with bounded `row`/`nowrap` fallbacks, preserving finite one-/two-value expansion, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, longhand/component precedence, row/column mapping, wrapping eligibility, line formation and reverse stacking, gap and margin mapping, flex sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `87465d23`; locked scoped check, focused flex-flow parser/cascade (6 tests), public inheritance integration (1 test), full-native integration (260/260), feature library (1,043 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-221` | percentages, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-223` | completed bounded standalone case-insensitive `flex:inherit` by projecting to the existing private `flex-grow`, `flex-shrink`, and `flex-basis` inheritance owners; all three computed parent components copy only when the shorthand is explicitly authored while omitted `flex` remains local with the bounded `0 1 auto` fallbacks, preserving finite shorthand expansion, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, longhand/component precedence, row/column and wrapped sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `c94b6006`; locked scoped check, focused flex-shorthand parser (3 tests) and inheritance cascade (1 test), public inheritance integration (1 test), full-native integration (261/261), feature library (1,044 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-222` | percentages, intrinsic sizing, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-224` | completed bounded standalone case-insensitive `flex-grow:inherit` through the existing private grow component and ancestor-style chain; the computed parent grow value copies only when explicitly authored while omitted `flex-grow` remains local with the bounded `0` fallback, preserving finite longhand/shorthand precedence, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, row/column and wrapped sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `bc8de568`; locked scoped check, focused flex parser/cascade (35 tests), public inheritance integration (1 test), full-native integration (262/262), feature library (1,044 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-223` | direct `flex-shrink:inherit`, direct `flex-basis:inherit`, percentages, intrinsic sizing, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-225` | completed bounded standalone case-insensitive `flex-shrink:inherit` through the existing private shrink component and ancestor-style chain; the computed parent shrink value copies only when explicitly authored while omitted `flex-shrink` remains local with the bounded `1` fallback, preserving finite longhand/shorthand precedence, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, row/column and wrapped sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `b4f465db`; locked scoped check, focused flex parser/cascade (35 tests), public inheritance integration (1 test), full-native integration (263/263), feature library (1,044 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-224` | direct `flex-basis:inherit`, percentages, intrinsic sizing, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-226` | completed bounded standalone case-insensitive `flex-basis:inherit` through the existing private basis component and ancestor-style chain; the computed parent basis value copies only when explicitly authored while omitted `flex-basis` remains local with the bounded `auto` fallback, preserving finite longhand/shorthand precedence, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, row/column and wrapped sizing, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `4ee2e1ed`; locked scoped check, focused flex parser/cascade (35 tests), public inheritance integration (1 test), full-native integration (264/264), feature library (1,044 passed, 1 ignored), workspace all-target/all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and formatting passed locally | `native-engine-225` | `order:inherit`, percentages, intrinsic sizing, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-227` | completed bounded standalone case-insensitive `order:inherit` through the existing private parent-style chain; the computed parent order copies only when explicitly authored while omitted `order` remains local with the bounded `0` fallback, preserving finite signed values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, stable visual `(order, source_index)` sorting, semantic/source order, row/column placement, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, and the two-crate boundary; implementation `6dde525a` with compatibility fixture follow-up `1e7663c1`; locked scoped check, focused flex parser/cascade (35 tests), public inheritance integration (1 test), full-native integration (265/265), feature library (1,044 passed, 1 ignored), workspace check, strict Clippy, paired crate builds, warning-denied rustdoc, and formatting passed locally | `native-engine-226` | percentages, intrinsic sizing, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide flex conformance |
| `native-engine-228` | completed bounded standalone case-insensitive `gap:inherit` through the existing private parent-style chain; computed parent row and column gap components copy only when explicitly authored while omitted `gap` remains local with the bounded `0` fallback, preserving finite one-/two-value shorthand forms, CSS-wide resets, `revert-layer`, mixed-invalid preservation, important/source order, shorthand/longhand precedence, wrapped/column placement, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; direct `row-gap:inherit` and `column-gap:inherit` remain outside the shorthand-only boundary; implementation `978ae3b5` with compatibility fixture follow-up `b645585b`; locked scoped check, focused gap parser/cascade (11 tests), public inheritance integration (1 test), full-native integration (266/266), feature library (1,045 passed, 1 ignored), workspace check, strict Clippy, paired crate builds, warning-denied rustdoc, and formatting passed locally | `native-engine-227` | percentages, intrinsic values, direct row/column-gap inheritance, additional origins, transitions, animations, grid track sizing, generic CSS-wide machinery, browser parity, and browser-wide gap conformance |
| `native-engine-229` | completed bounded standalone case-insensitive `row-gap:inherit` and `column-gap:inherit` through the existing private parent-style chain; each computed parent gap component copies only when explicitly authored while omitted longhands remain local with the bounded `0` fallback, preserving independent-axis cascade, layer/importance/source-order precedence, shorthand/longhand interaction, finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, wrapped/column placement, display-list, fixed-cell raster, PNG capture, point-hit, semantics, diagnostics, and the two-crate boundary; implementation `6c4116e4` (design `0e2916c9`); locked scoped check, focused gap parser/cascade (12 tests), public independent-axis integration (1 test), full-native integration (267/267), feature library (1,046 passed, 1 ignored), workspace check, strict Clippy, paired crate builds, warning-denied rustdoc, and formatting passed locally | `native-engine-228` | percentages, intrinsic values, additional origins, transitions, animations, grid track sizing, generic CSS-wide machinery, browser parity, and browser-wide gap conformance |
| `native-engine-230` | completed bounded standalone case-insensitive `text-indent:inherit` through the existing private parent-style chain; an explicit declaration copies the computed parent first-line indent while omitted `text-indent` remains local with the bounded `0` fallback, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, source-order/important precedence, first-line wrapping, text fragments, display-list, fixed-cell raster, PNG capture, point-hit, semantic/source order, diagnostics, and the two-crate boundary; implementation `5d214fee` (design `3782eb4b`); locked scoped check, focused parser/cascade (3 tests), public inheritance/artifact integration (1 test), full-native integration (268/268), feature library (1,047 passed, 1 ignored), and formatting/diff checks passed locally | `native-engine-229` | `text-overflow:inherit`, negative or hanging indentation, percentages, font-relative units, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide text conformance |
| `native-engine-231` | completed bounded standalone case-insensitive `text-overflow:inherit` through the existing private parent-style chain; an explicit declaration copies the computed parent `clip|ellipsis` value while omitted `text-overflow` remains local with the bounded `clip` fallback, preserving finite values, CSS-wide resets, `revert-layer`, mixed-invalid preservation, source-order/important precedence, eligible clipped-nowrap truncation, text fragments, display-list, fixed-cell raster, PNG capture, overflow, point-hit, semantic/source order, diagnostics, and the two-crate boundary; implementation `9a9ef2c7` (design `57e37dfe`); locked scoped check, focused parser/cascade (3 tests), public truncation/artifact integration (1 test), full-native integration (269/269), feature library (1,048 passed, 1 ignored), and formatting/diff checks passed locally | `native-engine-230` | `overflow:inherit`, custom ellipsis/fade, multi-line or nested-inline truncation, percentages, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide overflow conformance |
| `native-engine-232` | completed bounded standalone case-insensitive `overflow:inherit`, `overflow-x:inherit`, and `overflow-y:inherit` through the existing private parent-style chain; each explicit declaration copies the parent's effective bounded clip/no-clip axis projection while omitted overflow remains local with the visible/no-clip fallback, preserving shorthand/longhand and important precedence, mixed-invalid preservation, axis-specific clip/scroll projection, display-list, fixed-cell raster, PNG capture, point-hit, semantic/source order, diagnostics, and the two-crate boundary; implementation `1a7df31b` (design `0a617b8c`, contract clarification `2e8bcf9a`); scoped native-feature check, focused batch (9 library-target tests and 19 overflow-matching native integration tests), and formatting/diff checks passed locally | `native-engine-231` | CSS-wide reset forms, nested scrolling, scrollbars, visible/auto/scroll used-value parity, custom overflow behavior, additional origins, transitions, animations, generic CSS-wide machinery, browser parity, and browser-wide overflow conformance |
| `native-engine-233` | completed bounded standalone case-insensitive `overflow:initial`, `overflow:unset`, and one-author-origin `overflow:revert` reset forms across the shorthand and longhands; each winning reset resolves the affected axis to the existing visible/no-clip fallback while explicit `inherit`, named-layer `revert-layer`, important/source order, mixed-invalid preservation, axis-specific clip/scroll projection, display-list, fixed-cell raster, PNG capture, point-hit, semantic/source order, diagnostics, and the two-crate boundary remain bounded; implementation `7b31b72e` (design `70f4f924`); scoped native-feature check, focused reset batch (1 library-target test and 1 native integration test), and formatting/diff checks passed locally | `native-engine-232` | nested scrolling, scrollbars, visible/auto/scroll used-value parity, other CSS-wide machinery, additional origins, transitions, animations, browser parity, and browser-wide overflow conformance |
| `native-engine-234` | completed bounded standalone case-insensitive finite `visible`, `auto`, and `scroll` acceptance for `overflow`, `overflow-x`, and `overflow-y`; all values reuse the existing visible/no-clip `OverflowValue::Other` projection without nested scroll containers or scrollbar artifacts, preserving explicit `inherit`, CSS-wide resets, named-layer `revert-layer`, priority, independent axes, mixed-invalid preservation, root scroll projection, display-list, fixed-cell raster, PNG capture, point-hit, semantic/source order, diagnostics, and the two-crate boundary; implementation `8a96f56b` (design `b2119e5f`); scoped native-feature check, focused no-clip pair (1 library-target test and 1 native integration test), overflow regression batch (11 library-target tests and 21 native integration tests), and formatting/diff checks passed locally | `native-engine-233` | nested scrolling, scrollbars, scroll-container used-value behavior, overflow propagation beyond the root owner, other CSS-wide machinery, additional origins, transitions, animations, browser parity, and browser-wide overflow conformance |

## Delivery evidence

The task file for each slice owns its touched paths and verification commands;
`docs/plan/tasks/native-engine-234.md` is the latest completed task;
`docs/plan/tasks/native-engine-233.md` is the preceding completed task;
`docs/plan/tasks/native-engine-232.md` is the preceding completed task;
`docs/plan/tasks/native-engine-231.md` is the preceding completed task;
`docs/plan/tasks/native-engine-230.md` is the preceding completed task;
`docs/plan/tasks/native-engine-229.md` is the preceding completed task;
`docs/plan/tasks/native-engine-228.md` is the preceding completed task;
`docs/plan/tasks/native-engine-226.md` is the preceding completed task;
`docs/plan/tasks/native-engine-225.md` is the preceding completed task;
`docs/plan/tasks/native-engine-224.md` is the preceding completed task;
`docs/plan/tasks/native-engine-223.md` is the preceding completed task;
`docs/plan/tasks/native-engine-222.md` is the preceding completed task;
`docs/plan/tasks/native-engine-221.md` is the preceding completed task;
`docs/plan/tasks/native-engine-220.md` is the preceding completed task;
`docs/plan/tasks/native-engine-219.md` is the preceding completed task;
`docs/plan/tasks/native-engine-218.md` is the preceding completed task;
`docs/plan/tasks/native-engine-217.md` is the preceding completed task;
`docs/plan/tasks/native-engine-216.md` is the preceding completed task;
`docs/plan/tasks/native-engine-215.md` is the preceding completed task;
`docs/plan/tasks/native-engine-214.md` is the preceding completed task;
`docs/plan/tasks/native-engine-213.md` is the preceding completed task;
`docs/plan/tasks/native-engine-212.md` is the preceding completed task;
`docs/plan/tasks/native-engine-211.md` is the preceding completed task;
`docs/plan/tasks/native-engine-210.md` is the preceding completed task;
`docs/plan/tasks/native-engine-209.md` is the preceding completed task;
`docs/plan/tasks/native-engine-208.md` is the preceding completed task;
`docs/plan/tasks/native-engine-207.md` is the preceding completed task;
`docs/plan/tasks/native-engine-206.md` is the preceding completed task;
`docs/plan/tasks/native-engine-205.md` is the preceding completed task;
`docs/plan/tasks/native-engine-204.md` is the preceding completed task;
`docs/plan/tasks/native-engine-203.md` is the preceding completed task;
`docs/plan/tasks/native-engine-202.md` is the preceding completed task;
`docs/plan/tasks/native-engine-201.md` is the preceding completed task;
`docs/plan/tasks/native-engine-200.md` is the preceding completed task;
`docs/plan/tasks/native-engine-199.md` is the preceding completed task;
`docs/plan/tasks/native-engine-198.md` is the preceding completed task;
`docs/plan/tasks/native-engine-197.md` is the preceding completed task;
`docs/plan/tasks/native-engine-196.md` is the preceding completed task;
`docs/plan/tasks/native-engine-195.md` is the preceding completed task;
`docs/plan/tasks/native-engine-194.md` is the preceding completed task;
`docs/plan/tasks/native-engine-193.md` is the preceding completed task;
`docs/plan/tasks/native-engine-192.md` is the preceding completed task;
`docs/plan/tasks/native-engine-191.md` is the preceding completed task;
`docs/plan/tasks/native-engine-190.md` is the preceding completed task;
`docs/plan/tasks/native-engine-189.md` is the preceding completed task;
`docs/plan/tasks/native-engine-188.md` is the preceding completed task;
`docs/plan/tasks/native-engine-187.md` is the preceding completed task;
`docs/plan/tasks/native-engine-186.md` is the preceding completed task;
`docs/plan/tasks/native-engine-185.md` is the preceding completed task;
`docs/plan/tasks/native-engine-183.md` is the preceding completed task;
`docs/plan/tasks/native-engine-180.md` is the preceding completed task;
`docs/plan/tasks/native-engine-177.md` is the preceding completed task;
`docs/plan/tasks/native-engine-173.md` is the preceding completed task;
`docs/plan/tasks/native-engine-171.md` is the preceding completed task;
`docs/plan/tasks/native-engine-169.md` is the preceding completed task;
`docs/plan/tasks/native-engine-168.md` is the preceding completed task;
`docs/plan/tasks/native-engine-167.md` is the preceding completed task;
`docs/plan/tasks/native-engine-166.md` is the preceding completed task;
`docs/plan/tasks/native-engine-165.md` is the preceding completed task;
`docs/plan/tasks/native-engine-164.md` is the preceding completed task;
`docs/plan/tasks/native-engine-163.md` is the preceding completed task;
`docs/plan/tasks/native-engine-159.md` is the preceding completed task;
`docs/plan/tasks/native-engine-158.md` is the preceding completed task;
`docs/plan/tasks/native-engine-157.md` is the preceding completed task;
`docs/plan/tasks/native-engine-156.md` is the preceding completed task;
`docs/plan/tasks/native-engine-155.md` is the preceding completed task;
`docs/plan/tasks/native-engine-154.md` is the preceding completed task;
`docs/plan/tasks/native-engine-153.md` is the preceding completed task;
`docs/plan/tasks/native-engine-152.md` is the preceding completed task;
`docs/plan/tasks/native-engine-151.md` is the preceding completed task;
`docs/plan/tasks/native-engine-150.md` is the preceding completed task;
`docs/plan/tasks/native-engine-148.md` is the preceding completed task;
`docs/plan/tasks/native-engine-146.md` is the preceding completed task;
`docs/plan/tasks/native-engine-145.md` is the preceding completed task;
`docs/plan/tasks/native-engine-144.md` is the preceding completed task;
`docs/plan/tasks/native-engine-143.md` is the preceding completed task;
`docs/plan/tasks/native-engine-142.md` is the preceding completed task;
`docs/plan/tasks/native-engine-141.md` is the preceding completed task;
`docs/plan/tasks/native-engine-140.md` is the preceding completed task;
`docs/plan/tasks/native-engine-139.md` is the preceding completed task;
`docs/plan/tasks/native-engine-137.md` is the preceding completed task;
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
`docs/plan/tasks/native-engine-118.md` is an earlier completed checkpoint;
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
The current completed checkpoint is recorded for
`docs/plan/tasks/native-engine-234.md`: implementation is `8a96f56b` (design
`b2119e5f`). It accepts bounded standalone case-insensitive finite
`overflow: visible|auto|scroll`, `overflow-x: visible|auto|scroll`, and
`overflow-y: visible|auto|scroll` values as the existing visible/no-clip
projection, without nested scroll containers or scrollbar artifacts. It
preserves explicit `inherit`, CSS-wide resets, named-layer `revert-layer`,
priority, independent axes, mixed-invalid preservation, root scroll projection,
display-list, raster/PNG, point-hit, semantics, diagnostics, and the two-crate
boundary. The scoped check passed; the focused no-clip pair passed 1
library-target test and 1 native integration test; the overflow regression
batch passed 11 library-target tests and 21 native integration tests; and
formatting/diff checks pass locally. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-233.md`: implementation is `7b31b72e` (design
`70f4f924`). It adds standalone, case-insensitive `overflow: initial`,
`overflow: unset`, and one-author-origin `overflow: revert` reset forms across
the shorthand and longhands, resolving each affected axis to the existing
visible/no-clip fallback while preserving explicit `inherit`, named-layer
`revert-layer`, priority, mixed-invalid preservation, axis-specific clipping,
root scroll projection, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary. The scoped check passed; the focused
reset batch passed 1 library-target test and 1 native integration test; and
formatting/diff checks pass locally. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-232.md`: implementation is `1a7df31b` (design
`0a617b8c`, contract clarification `2e8bcf9a`). It adds standalone,
case-insensitive `overflow: inherit`, `overflow-x: inherit`, and
`overflow-y: inherit` through the existing private parent-style chain, copying
the parent's effective bounded clip/no-clip axis projection only when
explicitly authored while omitted overflow remains local with bounded
visible/no-clip fallback. Shorthand/longhand and important precedence,
mixed-invalid preservation, axis-specific clip/scroll projection,
display-list, raster/PNG, point-hit, semantics, diagnostics, and the two-crate
boundary remain bounded. The scoped native-feature check passed; the focused
batch passed 9 library-target tests and 19 overflow-matching native integration
tests; formatting and diff checks pass locally. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary, and promotion claims
are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-231.md`: implementation is `9a9ef2c7` (design
`57e37dfe`). It adds standalone, case-insensitive `text-overflow: inherit`
through the existing private parent-style chain, copying the computed parent
`clip|ellipsis` value only when explicitly authored while omitted
`text-overflow` remains local with bounded `clip` fallback. Finite values,
CSS-wide resets, `revert-layer`, mixed-invalid preservation, source-order/
important precedence, eligible clipped-nowrap truncation, text fragments,
display-list, raster/PNG, overflow, point-hit, semantics, diagnostics, and the
two-crate boundary remain bounded. Focused parser/cascade coverage (3 tests),
public truncation/artifact integration (1 test), full-native integration
(269/269), feature library (1,048 passed, 1 ignored), and the locked scoped
check pass locally. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-230.md`: implementation is `5d214fee` (design
`3782eb4b`). It adds standalone, case-insensitive `text-indent: inherit`
through the existing private parent-style chain, copying the computed parent
first-line indent only when explicitly authored while omitted `text-indent`
remains local with bounded `0` fallback. Finite values, CSS-wide resets,
`revert-layer`, mixed-invalid preservation, source-order/important precedence,
first-line wrapping, text fragments, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Focused
parser/cascade coverage (3 tests), public inheritance/artifact integration (1
test), full-native integration (268/268), feature library (1,047 passed, 1
ignored), and the locked scoped check pass locally. Remote CI, push, release,
tag, registry publication, browser-parity, security-boundary, and promotion
claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-229.md`: implementation is `6c4116e4` (design
`0e2916c9`). It adds standalone, case-insensitive `row-gap: inherit` and
`column-gap: inherit` through the existing private parent-style chain, copying
the corresponding computed parent gap component only when explicitly authored
while omitted longhands remain local with bounded `0` fallback. Independent
axis cascade, layer/importance/source-order precedence, shorthand/longhand
interaction, reset and `revert-layer` behavior, mixed-invalid preservation,
wrapped/column placement, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Focused gap
parser/cascade coverage (12 tests), public integration (1 test), full-native
integration (267/267), feature library (1,046 passed, 1 ignored), and the
locked scoped check pass locally. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-228.md`: implementation is `978ae3b5` (design
`8241ef68`), with compatibility fixture follow-up `b645585b`. It adds
standalone, case-insensitive `gap: inherit` through the existing private
parent-style chain, copying computed parent row and column gap components only
when explicitly authored while omitted `gap` remains local with bounded `0`
fallback. Direct `row-gap: inherit` and `column-gap: inherit` remain outside
the shorthand-only boundary; mixed-invalid forms, important/source order,
shorthand/longhand precedence, reset semantics, `revert-layer`, wrapped/
column placement, display-list, raster/PNG, point-hit, diagnostics, and the
two-crate boundary remain bounded. The locked scoped native check, focused gap
parser/cascade coverage (11 tests), public inheritance integration (1 test),
full-native integration (266/266), feature library (1,045 passed, 1 ignored),
workspace check, strict Clippy, paired crate builds, warning-denied rustdoc,
and formatting/diff checks passed. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-227.md`: implementation is `6dde525a` (design
`330680b6`), with compatibility fixture follow-up `1e7663c1`. It adds
standalone, case-insensitive `order: inherit` through the existing private
parent-style chain, copying the computed parent order only when explicitly
authored while omitted `order` remains local with bounded `0` fallback. Visual
`(order, source_index)` sorting remains separate from semantic/source order;
mixed-invalid forms, important/source order, reset semantics, `revert-layer`,
display-list, raster/PNG, point-hit, diagnostics, and the two-crate boundary
remain bounded. The locked scoped native check, focused flex parser/cascade
coverage (35 tests), public inheritance integration (1 test), full-native
integration (265/265), feature library (1,044 passed, 1 ignored), workspace
check, strict Clippy, paired crate builds, warning-denied rustdoc, and
formatting/diff checks passed. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-226.md`: implementation is `4ee2e1ed` (design
`c52398fd`). It adds standalone, case-insensitive `flex-basis: inherit`
through the existing private basis component and ancestor-style chain, copying
the computed parent basis value only when explicitly authored while omitted
`flex-basis` remains local with bounded `auto` fallback. The locked scoped
native check, focused flex parser/cascade coverage (35 tests), public
inheritance integration (1 test), full-native integration (264/264), feature
library (1,044 passed, 1 ignored), workspace all-target/all-feature check,
strict Clippy, paired binaries, warning-denied rustdoc, and formatting/diff
checks passed. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-225.md`: implementation is `b4f465db` (design
`7064801b`). It adds standalone, case-insensitive `flex-shrink: inherit`
through the existing private shrink component and ancestor-style chain,
copying the computed parent shrink value only when explicitly authored while
omitted `flex-shrink` remains local with bounded `1` fallback. The locked
scoped native check, focused flex parser/cascade coverage (35 tests), public
inheritance integration (1 test), full-native integration (263/263), feature
library (1,044 passed, 1 ignored), workspace all-target/all-feature check,
strict Clippy, paired binaries, warning-denied rustdoc, and formatting/diff
checks passed. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-224.md`: implementation is `bc8de568` (design
`4402c331`). It adds standalone, case-insensitive `flex-grow: inherit` through
the existing private grow component and ancestor-style chain, copying the
computed parent grow value only when explicitly authored while omitted
`flex-grow` remains local with bounded `0` fallback. The locked scoped native
check, focused flex parser/cascade coverage (35 tests), public inheritance
integration (1 test), full-native integration (262/262), feature library
(1,044 passed, 1 ignored), workspace all-target/all-feature check, strict
Clippy, paired binaries, warning-denied rustdoc, and formatting/diff checks
passed. Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary, and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-223.md`: implementation is `c94b6006` (design
`f97ac088`). It adds standalone, case-insensitive `flex: inherit` by
projecting to the existing private `flex-grow`, `flex-shrink`, and `flex-basis`
inheritance owners, copying all three computed parent components only when
explicitly authored while omitted `flex` remains local with bounded `0 1 auto`
fallbacks. The locked scoped native check, focused flex-shorthand parser (3
tests) and inheritance cascade (1 test), public inheritance integration (1
test), full-native integration (261/261), feature library (1,044 passed, 1
ignored), workspace all-target/all-feature check, strict Clippy, paired
binaries, warning-denied rustdoc, and formatting/diff checks passed. Remote CI,
push, release, tag, registry publication, browser-parity, security-boundary,
and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-222.md`: implementation is `87465d23` (design
`cae84efb`). It adds standalone, case-insensitive `flex-flow: inherit` by
projecting to the existing private `flex-direction` and `flex-wrap` inheritance
owners, copying both computed parent components only when explicitly authored
while omitted `flex-flow` remains local with bounded `row`/`nowrap` fallbacks.
The locked scoped native check, focused flex-flow parser/cascade coverage (6
tests), public inheritance integration (1 test), full-native integration
(260/260), feature library (1,043 passed, 1 ignored), workspace all-target/
all-feature check, strict Clippy, paired binaries, warning-denied rustdoc, and
formatting/diff checks passed. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-221.md`: implementation is `ebeb8443` (design
`2925cc3a`). It adds standalone, case-insensitive `flex-wrap: inherit` while
omitted `flex-wrap` remains local with the bounded `nowrap` fallback.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-220.md`: implementation is `1f64f232` (design
`8ffe1f17`). It adds standalone, case-insensitive `flex-direction: inherit`
through the existing private parent-style chain while omitted
`flex-direction` remains local with the bounded `row` fallback.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-219.md`: implementation is `23703765` (design
`0b9e784b`). It adds standalone, case-insensitive `place-content: inherit`
through the existing private `align-content` and `justify-content` parent-style
owners while omitted `place-content` remains local.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-218.md`: implementation is `52adc7d1` (design
`ca9afd5f`). It adds standalone, case-insensitive `align-self: inherit`
through the existing private ancestor-style chain while omitted
`align-self` remains local `auto`; explicit `auto` continues to delegate to the
containing flex parent's `align-items`.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-217.md`: implementation is `feb6b607` (design
`5ee75fcb`). It adds standalone, case-insensitive `align-items: inherit`
through the existing private ancestor-style chain while omitted
`align-items` remains non-inherited with the bounded `flex-start` fallback.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-216.md`: implementation is `c3dc1faf` (design
`485c5750`). It adds standalone, case-insensitive `justify-content: inherit`
through the existing private ancestor-style chain while omitted
`justify-content` remains non-inherited with the bounded `flex-start` fallback.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-215.md`: implementation is `a66f474b` (design
`0cd2040d`). It adds standalone, case-insensitive `align-content: inherit`
through the existing private ancestor-style chain while omitted
`align-content` remains non-inherited with the bounded `flex-start` fallback.
The locked scoped native check, focused align-content parser/cascade coverage
(4 tests), focused flex parser/cascade coverage (31 tests), public nested-flex
fixture, full-native integration (253/253), feature library (1,036 passed, 1
ignored), workspace all-target/all-feature check, strict Clippy, paired
binaries, warning-denied rustdoc, and formatting/diff checks passed. Remote CI,
push, release, tag, registry publication, browser-parity, security-boundary,
and promotion claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-214.md`: implementation is `24ae15a7` (design
`9f26239d`). It extends the local `place-content` shorthand projection with
standalone, case-insensitive `initial`, `unset`, and one-author-origin
`revert`, retaining finite one-/two-value expansion and named-layer
`revert-layer`. Reset forms project through the bounded `flex-start` fallbacks
for both `align-content` and `justify-content` while terminal reset,
invalid-later preservation, important/source-order behavior, wrapped-line
distribution, main-axis placement, item alignment, flex sizing, display-list,
fixed-cell raster, PNG capture, point-hit, diagnostics, schema, and two-crate
owners remain bounded. The locked scoped native check, focused flex
parser/cascade coverage (31 tests) plus dedicated place-content cascade (1
test), public integration, full native integration (252/252), feature library
(1,035 passed, 1 ignored), workspace all-target/all-feature check, strict
Clippy, paired binaries, warning-denied rustdoc, and formatting/diff checks
passed. The preceding task's six-target nightly fuzz certification remains
current because this slice changes only CSS flex shorthand parsing/cascade.
Package and security evidence is retained from the preceding dependency-stable
task until the next issue-level validation boundary. Remote CI, push, release,
tag, registry publication, browser-parity, security-boundary, or promotion
claims are not made.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-213.md`: implementation is `337da7c5` (design
`e82eea50`); its local gate evidence is retained in the task record.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-202.md`: implementation is `65e3a76d` (design
`d435032c`); its local gate evidence is retained in the task record.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-200.md`: implementation is `62525ec4` (design
`9eafebc9`); its local gate evidence is retained in the task record.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-199.md`: implementation is `4771f352` (design
`4b02b41b`). It adds standalone case-insensitive `inherit`, `initial`, `unset`,
and one-author-origin `revert` to inherited `text-align`, `text-align-last`,
`text-justify`, and `direction`; its local gate evidence is retained in the
task record.
The preceding completed checkpoint remains recorded for
`docs/plan/tasks/native-engine-197.md`: implementation is `a0e102b5` (design
`7d71a50c`). It adds standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert` to the six local dimension declarations. Winning
reset candidates resolve to the existing optional `None`/auto fallback without
falling through; `revert-layer` remains the separate lower-layer rollback
candidate.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-196.md` at `fd6ee415` (design `9231d17f`).
It adds standalone case-insensitive `inherit` to the six local dimension
declarations while keeping omitted dimensions local and preserving explicit
parent `None`/auto fallbacks.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-195.md` at `0caad64b`.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-194.md` at `5477fb79`.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-193.md` at `3e78c246`.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-192.md` at `fc2c461e`.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-191.md` at `43e5f8c2`.
The preceding checkpoint remains recorded for
`docs/plan/tasks/native-engine-190.md` at `d5cc6e6b`.
The historical checkpoint remains recorded for
`docs/plan/tasks/native-engine-173.md`: the implementation is `f5f53cec`. It
adds the bounded complete physical `border` and four side-border shorthand
CSS-wide keyword family, projecting explicit `inherit` and reset values into
the existing width/style/color streams while preserving concrete and
omitted-component forms, `currentColor`, `revert-layer`, independent
composition, geometry, and all artifact consumers. Local evidence currently
includes focused parser/cascade 2/2, focused consumer 1/1, border library
regressions 23/23, border integration regressions 25/25, full native
integration 211/211, and feature-enabled `glass-browser` library 975 passed
with 1 ignored. Strict Clippy, warning-denied rustdoc, paired `glass-dev`
check/build, package, static documentation, workspace all-target/all-feature,
and bounded cleanup gates remain to be recorded in the task. No remote CI,
push, release, tag, registry publication, browser-parity, security-boundary,
or promotion claim is made.
The preceding 169 checkpoint remains recorded in
`docs/plan/tasks/native-engine-169.md` with design `b88177dc`, implementation
`4d9e6979`, test-fixture corrections `a0e77c84` and `801f7b19`, synchronized
product documentation `07ba3dc4`, and documentation/cleanup closeout
`e9a77fc0`. The preceding 168 checkpoint remains recorded in
`docs/plan/tasks/native-engine-168.md` with design `0bb67e8b`, implementation
`4521f151`, synchronized product documentation `2184d98`, and documentation/
cleanup closeout `7a09b10f`. The
preceding 167 checkpoint remains recorded in
`docs/plan/tasks/native-engine-167.md` with design `9e143200`, implementation
`1ae1f103`, synchronized product documentation `debd6ab4`, and documentation/
cleanup closeout `0bb67e8b`. The
preceding 166 checkpoint remains recorded in
`docs/plan/tasks/native-engine-166.md` with design `d148f766`, implementation
`b57ba2b8`, synchronized product documentation `d26a9059`, and documentation/
cleanup closeout `9e143200`. The
preceding 165 checkpoint remains recorded in
`docs/plan/tasks/native-engine-165.md` with design `75544c39`, implementation
`f7b5fd4e`, synchronized product documentation `01438316`, and documentation/
cleanup closeout `d148f766`. The preceding 164
checkpoint remains recorded in `docs/plan/tasks/native-engine-164.md` with
design `e11821c5`, implementation `cceb61bf`, diagnostics follow-up `005083c3`,
product documentation `2960ecc5`, and documentation/cleanup closeout
`75544c39`. The preceding 163 checkpoint remains recorded in
`docs/plan/tasks/native-engine-163.md` with design `764e0c86`, implementation
`7a406855`, and documentation/cleanup closeout `5b426826`. The preceding 158
checkpoint remains recorded in `docs/plan/tasks/native-engine-158.md` with
design `6041a479`, implementation `f04623fc`, and documentation/cleanup closeout `e885332c`. The
preceding 157 checkpoint remains recorded in
`docs/plan/tasks/native-engine-157.md` with design `ab9d6628`, implementation
`fb2c56a2`, and documentation/cleanup closeout `30ed10ca`. The preceding 156
checkpoint remains recorded in `docs/plan/tasks/native-engine-156.md` with
design `13c8e90f`, implementation `5270d013`, and documentation/cleanup
closeout `e7e744b9`. The preceding 155 checkpoint remains
recorded in `docs/plan/tasks/native-engine-155.md` with design `37126fa1`,
implementation `2b07f109`, and documentation/cleanup closeout `4e766f29`. The
preceding 154 checkpoint remains
recorded in `docs/plan/tasks/native-engine-154.md` with design `e7c9ad40`,
implementation `875cdad8`, and documentation/cleanup closeout `01a141ab`.
The preceding 153 checkpoint remains
recorded in `docs/plan/tasks/native-engine-153.md` with design `77d81fdc`,
implementation `6169cabc`, and documentation/cleanup closeout `73261116`.
The preceding 152 checkpoint remains recorded in
`docs/plan/tasks/native-engine-152.md` with design `ff4d7803`, implementation
`7dfcc7f5`, and documentation/cleanup closeout `22ca1c56`.
Focused/full-native, affected-library, strict-Clippy, rustdoc, two-crate
binary, formatting, documentation/release, and bounded cleanup gates remain
local evidence. Remote CI remains pending because the branch is local-only.
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
