# Glass delivery plans

## Current source line: Glass 0.3.14

Status: Current 0.3.14 source behavior for the 2026-08-29 release record.
The `0.3.13` release notes and migration guide are historical records for the
earlier tagged release; they are not current-source instructions. The
[documentation index](../INDEX.md#glass-documentation) is the navigation map
for this source line.

The [0.3.14 release notes](../releases/0.3.14.md) and
[0.3.14 migration guide](../migration/0.3.14.md) route the current release
contract. The [0.3.14 release evidence](../release-evidence.md#0.3.14-release-evidence)
holds the exact-source records and closed publication evidence. The
historical [0.3.13 release notes](../releases/0.3.13.md) and
[0.3.13 migration guide](../migration/0.3.13.md) retain their original
version claims.

| Current source area | Status and reference |
|---|---|
| TUI onboarding and development-suite launch | 0.3.14; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Editor source/diff rendering, soft-wrap, cursor synchronization, and review state | 0.3.14; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Actor-attributed editor collaboration | 0.3.14; [Development Runtime](../development-runtime.md) and [MCP tool catalog](../mcp-tools.md) |
| Kitty/live browser presentation | 0.3.14; [Mobile and remote](../mobile-remote.md) and [browser connection](../architecture/browser-connection.md) |
| Pi runtime and external harness workflow | Current checkout with Pi SDK 0.84.4; [Native Pi SDK runtime](../pi-sdk-runtime.md), [Development Runtime](../development-runtime.md), and [CLI](../cli.md) |

## Active plan: Glass native browser engine (issue #40)

Status: Phase 0/Phase 1 checkpoint and the first Phase 2 semantic/action slices
are complete locally; later engine phases remain in progress. The
authoritative epic is
[issue #40](https://github.com/wanazhar/glass/issues/40). The design contract,
module decomposition, integration enumeration, and tradeoffs are in the
[native-engine architecture](../architecture/native-engine.md) and
[native-engine analysis](analysis/native-engine.md).

The first dependency-ordered checkpoint is
[native-engine-001](tasks/native-engine-001.md): a default-off,
fixture/data-URL-only, one-context engine kernel and explicit semantic backend.
It does not claim browser parity or remote-content safety.

The completed dependency-ordered slices are
[native-engine-002](tasks/native-engine-002.md), which adds bounded semantic
DOM projection and revision-bound locators;
[native-engine-003](tasks/native-engine-003.md), which adds the first
revisioned click/type/focus mutation path and effects signal;
[native-engine-004](tasks/native-engine-004.md), which adds deterministic
single-select/option state;
[native-engine-005](tasks/native-engine-005.md), which adds a bounded
visibility/actionability gate; and
[native-engine-006](tasks/native-engine-006.md), which hardens raw-text and
RCDATA handling. They remain semantic-only: CSS/layout hit testing,
JavaScript, network, and raw form-value evidence are not claimed.
[native-engine-007](tasks/native-engine-007.md), which adds a narrow
CSS-presentation model for selector-driven `display`/`visibility` state.
General CSS, scrolling/stacking layout, and screenshot/capture paint remain
unimplemented; later native slices add only bounded display-list and
software-surface artifacts.

The completed runtime integration slice is
[native-engine-008](tasks/native-engine-008.md). It adds a feature-gated
`BrowserRuntime::Native`, an explicit Rust session constructor, and a local
one-shot CLI path for navigate/click/type/text/observe/targets. The CLI default
configuration at that boundary accepted only `about:blank` and bounded
percent-decoded `data:text/html`; it did not register fixtures or contact
endpoints. The later 036 slice below adds bounded standard padded-base64
navigation. Unsupported flags, remote URLs, script/evaluate, MCP, and TUI
remain fail-closed.

The completed layout/input slice is
[native-engine-009](tasks/native-engine-009.md). It owns bounded integer-pixel
normal-flow geometry, Rust-only layout inspection, deterministic point
hit-testing, and the native `point=x,y` click-target extension. It does not
add screenshots, capture, scrolling, general CSS, or geometry to the stable
transport evidence contract.

The completed display-list slice is
[native-engine-010](tasks/native-engine-010.md). It adds a deterministic,
Rust-only clear/fill/text display list from the current layout revision and a
bounded solid-color CSS subset. It does not add screenshots, fonts, images, or
a paint capability to the stable backend contract.

The completed software-surface slice is
[native-engine-011](tasks/native-engine-011.md). It consumes that list into a
bounded logical RGBA surface with a small built-in glyph subset. It does not
add PNG/screenshots, font loading, GPU/window APIs, or a capture capability to
the stable backend contract. The next renderer slice must be documented and
committed separately before it expands this boundary.

The completed style-inheritance slice is
[native-engine-012](tasks/native-engine-012.md). It resolves inherited text
color through the bounded DOM chain and feeds the existing display-list and
software-surface artifacts. It does not add general CSS, inherited layout,
fonts, images, screenshot/capture transport, or stable backend capabilities.
The next renderer slice must be documented and committed separately before it
expands this boundary.

The completed paint-clipping slice is
[native-engine-013](tasks/native-engine-013.md). It adds bounded
`overflow:hidden` clip rectangles to fill/text display commands and enforces
them during Rust-only surface replay. It does not add nested scrolling, stacking,
borders, transforms, screenshots, or capture transport. The next renderer slice
must be documented and committed separately before it expands this boundary.

The completed uniform-border slice is
[native-engine-014](tasks/native-engine-014.md). It adds a bounded
`border:Npx solid <color>` CSS declaration, a revisioned `BorderRect` display
command, and inside-the-box software replay using the existing clip and
source-over rules. It does not add border box-model geometry, padding,
box-sizing, individual sides, non-solid styles, scrolling, transforms, or
screenshot evidence. Capture transport is separately owned by
[native-engine-015](tasks/native-engine-015.md).

The current capture slice is
[native-engine-015](tasks/native-engine-015.md). It adds bounded PNG encoding
for the existing logical RGBA surface and exposes explicit
`CaptureFormat::Png` through the native backend while keeping screenshot-
containing evidence, JPEG/PDF, physical pixels, and native CLI screenshots
unsupported. It must be verified and committed before the next renderer or
resource expansion changes this boundary.

The completed box-model slice is
[native-engine-016](tasks/native-engine-016.md). It adds bounded uniform
padding and margin, explicit content-box/border-box sizing, outer/content
layout rectangles, and content-origin child/text placement. General box-model
and layout behavior remains explicitly outside the native capability claim.

The completed viewport-scroll slice is
[native-engine-017](tasks/native-engine-017.md) adds bounded vertical root
scrolling across layout hit testing,
display-list replay, capture, and revisioned action effects; horizontal,
nested, smooth, and keyboard scrolling remain outside the current claim.

The completed side-specific-border slice is
[native-engine-018](tasks/native-engine-018.md). It adds independently
cascaded physical solid borders, side-aware box-model insets, and deterministic
clipped/scrolled display replay; other border styles, radii, logical
writing-mode sides, and browser corner-join fidelity remain outside the
current claim.

The completed bounded-pattern-border slice is
[native-engine-019](tasks/native-engine-019.md). It implements
typed `solid`/`dashed`/`dotted` physical border styles with deterministic
integer patterns; other border styles, radius/images/gradients, standalone
style properties, logical sides, and browser dash/corner fidelity remain
outside the current claim.

The completed bounded-corner-radius slice is
[native-engine-020](tasks/native-engine-020.md). It adds bounded physical
one-to-four-value `border-radius` shorthand expansion, conservative corner
normalization, rounded fill/border replay, and rounded point hit testing.
Percentages, elliptical radii, corner longhands, rounded descendant clips,
anti-aliasing, and browser corner fidelity remain outside the current claim.

The completed bounded-inline-flow slice is
[native-engine-021](tasks/native-engine-021.md). It adds preflight inline-box
line placement using the same bounded integer outer-width calculation as final
layout, preserving deterministic line height and hit/paint coordinates while
leaving typography and general inline formatting unsupported.

The completed bounded line-height slice is
[native-engine-022](tasks/native-engine-022.md). It adds a positive fixed pixel
`line-height` property as a local flow minimum while preserving explicit height
precedence and leaving font metrics, general inheritance, and browser
line-layout behavior unsupported.

The completed bounded direct-text-flow slice is
[native-engine-023](tasks/native-engine-023.md). It carries collapsed direct
text fragments from the shared flow cursor into source-ordered display paint,
repairing mixed text/inline origins while leaving typography and CSS whitespace
behavior unsupported.

The completed bounded word-wrap slice is
[native-engine-024](tasks/native-engine-024.md). It keeps collapsed words
together when the fixed line fits and splits only over-wide words, while
retaining source-ordered fragments and the existing typography limitations.

The completed physical box-edges slice is
[native-engine-025](tasks/native-engine-025.md). It expands bounded one-to-four
value physical `padding`/`margin` shorthands, supports their top/right/bottom/
left longhands with independent cascade, and feeds side-aware values through
content origins and normal-flow margins. Logical sides, invalid/negative/
percentage/`auto` values, margin collapsing, and general CSS layout remain
outside the native capability claim.

The completed whitespace-boundaries slice is
[native-engine-026](tasks/native-engine-026.md). It preserves bounded source
whitespace boundaries across sibling direct text, `display:contents`, and
supported inline flow items, paints consumed separators through the existing
text-fragment path, and drops separators at wrapped line starts. CSS
`white-space` modes, typography, and cross-owner inline parity remain outside
the native capability claim.

The completed overflow hit-test/projection slice is
[native-engine-027](tasks/native-engine-027.md). It shares the bounded
rectangular `overflow:hidden` ancestor intersection across software paint,
viewport rectangle projection, and point hit-testing, including nested clips in
document coordinates before root-scroll translation. Visible overflow,
axis-specific/nested scrolling, rounded descendant clips, and general CSS
hit-testing remain outside the native capability claim.

The completed unsupported-CSS-diagnostics slice is
[native-engine-028](tasks/native-engine-028.md). It makes ignored selectors,
properties, values, and malformed CSS observable through a bounded revisioned
Rust API without changing stable backend evidence or echoing raw stylesheet
content. Existing deterministic CSS omission/fallback behavior remains intact.

The completed pixel-golden-capture slice is
[native-engine-029](tasks/native-engine-029.md). It certifies the existing
bounded logical surface and decoded PNG bytes against one complete fixed
fixture golden without changing screenshot evidence or renderer scope. Its
focused golden, native integration, native unit, strict lint, full locked
all-target/all-feature, doctest, formatting, and documentation gates are
recorded in the task file and issue #40.

The completed descendant-selector slice is
[native-engine-030](tasks/native-engine-030.md). It extends the bounded CSS
grammar with ancestor-scoped compound selectors while preserving explicit
limits, diagnostics, cascade precedence, and the no-general-CSS boundary. Its
implementation and full validation evidence are recorded in the task file and
issue #40.

The completed overflow-clip slice is
[native-engine-031](tasks/native-engine-031.md). It accepts non-scrolling
`overflow: clip` through the existing bounded rectangular clip path while
preserving root-scroll, hit-testing, diagnostics, and no-general-CSS limits.
Its implementation and full validation evidence are recorded in the task file
and issue #40.

The completed hard-line-break slice is
[native-engine-032](tasks/native-engine-032.md). It treats visible `<br>`
elements as bounded hard line breaks in the existing inline-flow cursor while
preserving hidden-state handling, text ownership, fixed line-height, and the
no-general-CSS boundary. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed pre-line-break slice is
[native-engine-033](tasks/native-engine-033.md). It supports inherited
`white-space: pre-line` source newline breaks through the same bounded flow
cursor while retaining whitespace collapsing, CRLF normalization, and the
no-general-CSS boundary. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed preformatted-whitespace slice is
[native-engine-034](tasks/native-engine-034.md). It adds inherited
`white-space: pre` with literal fixed-cell source whitespace, LF/CR/CRLF hard
breaks, and no soft wrapping, while retaining explicit limits for wide-line
overflow, tab stops, font metrics, text alignment, and general CSS whitespace
conformance.

The completed pre-wrap-whitespace slice is
[native-engine-035](tasks/native-engine-035.md). It extends that inherited
source-whitespace path with fixed-cell soft wrapping for `white-space: pre-wrap`,
while retaining explicit limits for browser line breaking, tab
stops, font metrics, shaping, bidi, and general CSS conformance.

The completed base64-data-url slice is
[native-engine-036](tasks/native-engine-036.md). It adds bounded standard
base64 `data:text/html` loading through the existing native navigation and
dispatcher path, while retaining explicit local-only, UTF-8, size, and
non-network limits. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed local-navigation-history slice is
[native-engine-037](tasks/native-engine-037.md). It adds bounded raw-fragment
same-document navigation and explicit Rust back/forward traversal for local
resources, while retaining parse-before-commit, revision, history, scroll, and
non-network limits. Its implementation and validation evidence are recorded in
the task file and issue #40.

The completed local-link-activation slice is
[native-engine-038](tasks/native-engine-038.md). It wires semantic local anchor
clicks through the existing action and navigation owner, while retaining
fragment, parse-before-commit, revision, history, scroll, and non-network
limits. Its implementation and validation evidence are recorded in the task
file and issue #40.

The completed fragment-target-scroll slice is
[native-engine-039](tasks/native-engine-039.md). It adds exact visible local
fragment-target scrolling and saved root-scroll restoration across bounded
history traversal, while retaining raw-fragment, parse-before-commit, and
non-network limits. Its implementation and validation evidence are recorded in
the task file and issue #40.

The completed fixture-relative-link slice is
[native-engine-040](tasks/native-engine-040.md). It adds bounded same-host
relative resolution for registered fixtures while retaining explicit local
resource loading, parse-before-commit, fragment scrolling, and failure-atomic
navigation limits.

The completed percent-decoded-fragment-target slice is
[native-engine-041](tasks/native-engine-041.md). It decodes bounded UTF-8
percent escapes before exact visible local `id` matching while retaining the
existing duplicate-safe root scrolling, history restoration, local-only
resource, and malformed-target failure behavior.

The completed legacy-name-fragment-target slice is
[native-engine-042](tasks/native-engine-042.md). It adds a bounded exact
legacy `<a name>` fallback after decoded `id` lookup while retaining ID
precedence, duplicate-safe scrolling, and the existing local navigation and
history boundaries.

The completed bounded-text-fragment-target slice is
[native-engine-043](tasks/native-engine-043.md). It adds a bounded
`#:~:text=start[,end]` match against the first visible non-truncated text run
with per-term UTF-8 decoding while retaining the existing fragment, scroll,
history, and local-resource limits. The completed follow-on
[native-engine-044](tasks/native-engine-044.md) adds bounded exact prefix and
suffix affixes around that one-run matcher while retaining the current raw
comma grammar, per-term decoding, scroll, history, and fail-closed boundaries.
The completed follow-on [native-engine-045](tasks/native-engine-045.md) adds
bounded root horizontal scrolling from measured overflow width while retaining
independent clamping and the existing viewport, hit-test, display-list, raster,
and history contracts. The completed follow-on
[native-engine-046](tasks/native-engine-046.md) adds bounded inherited
`white-space: nowrap` through the same fixed-cell flow and measured root
horizontal scroll path. Its implementation and validation evidence are
recorded in the task file and issue #40.

The completed dependency-ordered slice
[native-engine-047](tasks/native-engine-047.md) propagated the existing
positive pixel `line-height` floor through the DOM style walk while preserving
explicit child declarations and height precedence. The completed follow-on
[native-engine-048](tasks/native-engine-048.md) makes measured root horizontal
overflow consume the existing `overflow:hidden`/`overflow:clip` intersection
so fully clipped text cannot create a false scroll range.

The completed dependency-ordered slice
[native-engine-049](tasks/native-engine-049.md) adds independently cascaded
`overflow-x:hidden`/`clip` and `overflow-y:hidden`/`clip` rectangles through
the existing paint, viewport, hit-test, and root-overflow owners.

The completed dependency-ordered slice
[native-engine-050](tasks/native-engine-050.md) adds bounded physical
`min-width`/`max-width`/`min-height`/`max-height` constraints through the
existing content-box and border-box geometry owner.

The completed dependency-ordered slice
[native-engine-051](tasks/native-engine-051.md) adds bounded CSS opacity groups
through the existing display-list and software-rasterizer owners. Subtree
compositing is explicit and bounded; layout and hit-testing do not treat
opacity as visibility. The completed follow-on dependency-ordered slice
[native-engine-052](tasks/native-engine-052.md) adds bounded inherited
`text-align` for fixed-cell direct text and supported inline flow. The completed
follow-on dependency-ordered slice
[native-engine-053](tasks/native-engine-053.md) extends the existing bounded
color grammar with fixed-point `rgba(R, G, B, A)` alpha for background, border,
and text paint while preserving the current display-list and raster owners.
Its implementation and validation evidence are recorded in the task file and
issue #40. No general CSS Color 4 or color-management parity is implied. The
completed follow-on dependency-ordered slice
[native-engine-054](tasks/native-engine-054.md) adds inherited fixed-cell
`text-decoration: none|underline` to text display commands and software
replay without changing layout or hit-testing ownership. Its implementation
and validation evidence are recorded in the task file and issue #40. The
completed follow-on [native-engine-055](tasks/native-engine-055.md) task adds inherited
ASCII `text-transform: none|uppercase|lowercase` during fixed-cell layout so
wrapping, text-fragment matching, display-list projection, and root-overflow
measurement share one transformed output; semantic source text remains
unchanged and Unicode/locale/font parity remains outside the boundary. Its
implementation and validation evidence are recorded in the task file and issue
#40. The completed follow-on [native-engine-056](tasks/native-engine-056.md)
task adds non-negative fixed-pixel `text-indent` to the first line of block
containers, clamps it to retain one fixed cell, and leaves inline and
`display:contents` elements on their containing block's flow. Its local
implementation and validation evidence are recorded in the task file and
issue #40; remote CI remains pending until this branch is pushed.
The completed follow-on [native-engine-057](tasks/native-engine-057.md) task
adds bounded inherited non-negative fixed-pixel `word-spacing` across the
existing collapsed and supported preformatted ASCII-space flow. Its measured
advance is shared by wrapping, text fragments, alignment, display-list
projection, raster replay, hit testing, and root-overflow measurement; the
implementation and validation evidence are recorded in the task file and
issue #40. Remote CI remains pending until this branch is pushed.
The completed follow-on [native-engine-058](tasks/native-engine-058.md) task
defines bounded inherited non-negative fixed-pixel `letter-spacing` after every
rendered fixed-cell character in each emitted fragment, composed with
`word-spacing` on ASCII spaces. Its measured advance is shared by wrapping,
preformatted chunking, text fragments, alignment, display-list projection,
raster replay, hit testing, and root-overflow measurement; browser
pair-boundary, Unicode, font-metric, negative, relative, percentage, and
`normal` semantics remain outside the boundary. Implementation and local
validation evidence are recorded in the task file and issue #40; remote CI
remains pending until this branch is pushed.
The completed follow-on [native-engine-059](tasks/native-engine-059.md) task
adds inherited `font-weight: normal|bold|400|700` to the fixed-cell text
presentation path. `normal`/`400` retain the current glyph replay and
`bold`/`700` add a clipped one-pixel horizontal dilation without changing
advances, layout, semantics, hit testing, overflow, or text-fragment
coordinates. Font selection, metrics, shaping, variable weights, and browser
text-rendering parity remain outside the boundary. Implementation and local
validation evidence are recorded in the task file and issue #40; remote CI
remains pending until this branch is pushed.
The completed follow-on [native-engine-060](tasks/native-engine-060.md) task
adds inherited `font-style: normal|italic` to the fixed-cell text presentation
path. `normal` retains the current glyph replay and `italic` applies a
deterministic clipped row-dependent horizontal shear without changing
advances, layout, semantics, hit testing, overflow, or text-fragment
coordinates. Bold dilation, underline, spacing, opacity, scrolling, and
capture compose through the existing immutable text-command and raster owners.
Oblique forms, angles, font selection/loading/metrics, shaping, anti-aliasing,
and browser text-rendering parity remain outside the boundary. Implementation
and local validation evidence are recorded in the task file and issue #40;
remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-061](tasks/native-engine-061.md) slice
adds inherited `word-break: normal|break-all` to the bounded collapsed
fixed-cell flow path. `normal` retains word-aware wrapping; `break-all` allows
deterministic character-boundary splitting for every collapsed word while
preserving the existing separator, spacing, fragment, overflow, and semantic
owners. `pre`, `pre-wrap`, and `nowrap` retain their established behavior.
Unicode line-breaking, grapheme policy, hyphenation, `overflow-wrap`, bidi,
writing modes, font metrics, and browser conformance remain outside the
boundary. The design is `14d7fc4`, the implementation is `479f3a3`, and the
documentation closeout is `433d6fd`; local validation evidence is recorded in
the task file and remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-062](tasks/native-engine-062.md) slice
adds local `text-overflow: clip|ellipsis` to the bounded single-line
fixed-cell path. `clip` retains the existing full visual run under a horizontal
overflow clip; eligible `ellipsis` blocks replace an overflowing suffix with a
spacing-aware fixed-cell ASCII `...` marker while preserving the full semantic
source text. The implementation is restricted to one direct text child in a
rendered `nowrap` block with finite horizontal clipping; multi-line
truncation, nested inline formatting, Unicode ellipsis behavior, and browser
conformance remain outside the boundary. The design is `7e488aa`, the
implementation is `e4c5bb1`, and local validation evidence is recorded in the
task file; remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-063](tasks/native-engine-063.md) slice
adds inherited `vertical-align: baseline|top|middle|bottom` to the existing
fixed-cell inline-flow line-item owner. `baseline` preserves the current
top-origin behavior; `top`, `middle`, and `bottom` apply bounded integer
offsets within the existing line box and move an inline item's boxes and text
artifacts together. Font metrics, typographic baselines, lengths, percentages,
bidi, writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. The design is `7721df2` and the implementation is
`facd2f6`; local validation evidence is recorded in the task file and remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-064](tasks/native-engine-064.md) slice
adds bounded block-level `display: flex` single-row placement for eligible
direct element children. Items retain source order, fixed explicit/intrinsic
widths, and margins without grow, shrink, wrap, reverse, gap, or cross-axis
distribution. Containers with meaningful direct text, `display: contents`, or
visible `<br>` children use the existing normal-flow fallback so content is not
dropped. The design is `a05bdd6`, the implementation is `7c38354`, and local
validation evidence is recorded in the task file. Remote CI remains pending
until this branch is pushed.

The completed follow-on [native-engine-065](tasks/native-engine-065.md) slice
adds one non-negative fixed-pixel `gap` between visible direct element items in
an eligible bounded flex row. Hidden and `display:none` items do not consume a
gap position; ineligible containers retain normal-flow fallback. Multi-value
and percentage gap grammar, `row-gap`, `column-gap`, flex distribution,
wrapping, and general Flexbox remain outside the boundary. The contract is
recorded in the task file. The design is `062998c`, the implementation is
`28735c4`, and local validation evidence is recorded there. Remote CI remains
pending until this branch is pushed.

The completed follow-on [native-engine-066](tasks/native-engine-066.md) slice
adds bounded `justify-content:flex-start|center|flex-end|space-between` to
eligible fixed-width flex rows. Positive free space is placed before the row
or distributed across its existing gaps with deterministic integer rounding;
overflow is never moved to a negative coordinate. Flex growth/shrink, wrapping,
direction, cross-axis alignment, `space-around`, `space-evenly`, and general
Flexbox remain outside the boundary. The design is `a53b10f`, the implementation
is `3fe5306`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-067](tasks/native-engine-067.md) slice
adds bounded non-inherited signed `order` values to eligible fixed-width flex
rows. Items sort by ascending order with stable source-order ties before the
existing gap and `justify-content` distribution; semantic DOM/source order
remains unchanged. The `-1024..=1024` integer bound, visual-only behavior,
normal-flow fallback, and exclusions are recorded in the task file. The design
is `09f3b00`, the implementation is `a713b6e`, and local validation evidence is
recorded there. Remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-068](tasks/native-engine-068.md) slice
adds bounded non-inherited `align-items:flex-start|center|flex-end` to eligible
fixed-width single-row flex rows. It aligns complete visual item subtrees
within an explicit content height or the auto row's maximum item outer height
using deterministic integer offsets, while preserving horizontal and
semantic/source order. The design is `a0488ef`, the implementation is
`6b55b9c`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-069](tasks/native-engine-069.md) slice
adds bounded non-inherited `flex-direction:row|row-reverse` to eligible
fixed-width single-row flex rows. `row` remains equivalent to 068;
`row-reverse` lays the order-sorted visual sequence from the physical right
edge while preserving physical margins, gap, justification, cross-axis
alignment, shared subtree artifacts, non-negative coordinates, root horizontal
scrolling, and semantic/source order. The design is `7fea901`, the
implementation is `be11f49`, and local validation evidence is recorded in the
task file. Remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-070](tasks/native-engine-070.md) slice
adds bounded non-inherited `flex-wrap:nowrap|wrap` to the same eligible
fixed-width flex rows. `nowrap` remains equivalent to 069; `wrap` forms
deterministic physical lines from measured item outer widths and the existing
gap, reuses per-line justification, row/reverse direction, and cross-axis
alignment, and preserves complete subtree artifacts, root overflow, and
semantic/source order. The design is `8772a6a`, the implementation is
`5c16185`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-071](tasks/native-engine-071.md)
slice adds bounded non-inherited
`align-content:flex-start|center|flex-end|space-between` to wrapped flex rows.
It distributes only positive cross-axis free space in an explicit content box
after 070 line formation, preserving line membership, per-line item alignment,
shared artifact coordinates, and the default `flex-start` fallback. The design
is `71bd380`, the implementation is `cc1d602`, the final single-line coverage
test is `050d41b`, and local validation and cleanup evidence are recorded in
the task file. `stretch`, around-line distribution,
cross-axis gaps, column directions, `wrap-reverse`, and general Flexbox remain
outside the contract. Remote CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-072](tasks/native-engine-072.md)
slice adds bounded non-inherited `align-content:space-around` to wrapped flex
rows. It places each formed line at a deterministic integer slot center using
only positive explicit content-box remainder, preserving 071 line membership,
item alignment, shared artifact coordinates, and semantic/source order. The
design is `c8e5170`, the implementation is `a1c8b56`, and local validation and
cleanup evidence are recorded in the task file. `space-evenly`, `stretch`,
cross-axis gaps, column directions, `wrap-reverse`, and general Flexbox remain
outside the contract. Remote CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-073](tasks/native-engine-073.md)
slice adds bounded non-inherited `align-content:space-evenly` to wrapped flex
rows. It places equal integer slots before, between, and after formed lines
using only positive explicit content-box remainder, preserving 072 line
membership, item alignment, shared artifact coordinates, and semantic/source
order. The design is `23b62a3`, the implementation is `b4833a9`, and local
validation and cleanup evidence are recorded in the task file. The default
stack overflow in one existing large-Clap parser test is documented there;
the full native library suite passes with an explicit 8 MiB test-thread stack.
`stretch`, `place-content`, cross-axis gaps, column directions, `wrap-reverse`,
and general Flexbox remain outside the contract. Remote CI remains pending
until this branch is pushed.

The completed dependency-ordered [native-engine-074](tasks/native-engine-074.md)
slice adds bounded non-inherited `flex-wrap:wrap-reverse` to the same eligible
fixed-width flex rows. It preserves source-order line formation while placing
formed lines from the physical cross-axis end, reusing every bounded
`align-content` value and translating complete line artifact ranges with a
signed document-pixel delta. The design is `5f6c61a`, the implementation is
`96fd15c`, and local validation and cleanup evidence are recorded in the task
file. The previously documented default-stack issue in one large-Clap parser
test remains a harness follow-up; the full native library suite passes with an
explicit 8 MiB test-thread stack. Remote CI remains pending until this branch
is pushed.

The completed dependency-ordered [native-engine-075](tasks/native-engine-075.md)
slice adds bounded explicit non-inherited `align-content:stretch` to wrapped
fixed-width flex rows. It expands formed line heights by deterministic integer
shares of positive explicit content-box remainder, then reuses per-line
`align-items` and complete artifact translation for normal and wrap-reverse
stacking. The design is `4637863`, implementation is `e26c0a4` with the
diagnostics-fixture correction in `cc8b538`, and local validation and cleanup
evidence are recorded in the task file. The default-stack issue in one
existing large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-076](tasks/native-engine-076.md)
slice adds the explicit non-inherited `align-content:normal` keyword to the
same wrapped fixed-width rows. In this bounded engine it aliases the completed
075 line-box stretch owner for positive explicit cross-axis remainder, while
the established omitted-value `flex-start` fallback remains unchanged. The
design is `b6c647e`, implementation is `e3933b3`, and local validation and
cleanup evidence are recorded in the task file. The default-stack issue in one
existing large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-077](tasks/native-engine-077.md)
slice adds explicit non-inherited `row-gap` spacing between adjacent formed
lines in eligible wrapped fixed-width flex rows. It includes that gap once in
the existing `align-content` occupied-size/free-space owner and preserves the
current main-axis-only `gap` behavior. The design is `99d70ef`, implementation
is `4b27b15`, and local validation and cleanup evidence are recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-078](tasks/native-engine-078.md)
slice completes the bounded flex gap family: one- and two-value integer-pixel
`gap`, `row-gap`, and `column-gap` with declaration-order-aware shorthand and
longhand cascade. One-value `gap` intentionally supplies both axes, and the
affected wrapped-row goldens move with that implementation. The design is
`c6ebecd`, implementation is `1bca33f`, and local validation and cleanup
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-079](tasks/native-engine-079.md)
slice adds bounded non-inherited integer `flex-grow` weights to the existing
row-flex owner. Positive free space is allocated before justification with a
deterministic prefix-floor policy, max-width caps freeze and redistribute
remainder, and negative free space remains an explicit no-shrink overflow
case. The design is `a45fb01`, implementation is `1a930a3`, and local
validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-080](tasks/native-engine-080.md)
slice adds bounded non-inherited integer `flex-shrink` weights to the same
row-flex owner. Negative line free space is allocated by original-base-width
weighted prefix-floor shares, effective `min-width` floors freeze and
redistribute the deficit, and zero-factor or minimum-exhausted rows retain
explicit overflow. The design is `46227de5`, implementation is `b1414931`, and
local validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-081](tasks/native-engine-081.md)
slice defines bounded `flex-basis:auto|Npx` sizing for the same row-flex owner.
Explicit bases override item `width`, use the existing box-sizing and min/max
helpers, remain unclamped before line formation so the completed grow/shrink
passes can resolve them, and preserve margins, gaps, descendants, paint,
overflow, hit testing, and semantic/source order. The design is `04cc4a3e`,
implementation is `299c93f9`, and local validation and cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-082](tasks/native-engine-082.md)
slice adds bounded `flex` shorthand expansion into the existing grow, shrink,
and basis components. It covers `none`, `auto`, bounded integer factor forms,
and bounded pixel/`auto` bases with declaration-order-aware longhand overrides;
unsupported CSS-wide, percentage, fractional, and ambiguous forms remain
diagnosed. The design is `71d1060d`, implementation is `40f6fb1c`, and local
validation and exact target-cleanup evidence are recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-083](tasks/native-engine-083.md)
slice defines bounded `flex-flow` shorthand expansion into the existing
direction and wrap components. It covers row/reverse-row and nowrap/wrap/
wrap-reverse tokens in either order, with omitted components reset to their
initial values; unsupported columns, duplicates, CSS-wide, logical-direction,
and ambiguous forms remain diagnosed. The design is `80c6836e`, implementation
is `0291bf90`, the strict-Clippy fix is `16e6d9aa`, and focused/full validation
and exact target-cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-084](tasks/native-engine-084.md)
slice defines bounded non-inherited `align-self:auto|flex-start|center|flex-end`
for eligible direct flex items. `auto` resolves to the existing parent
`align-items` value; explicit values reuse the existing line/subtree artifact
translation. Stretch, baseline, logical, CSS-wide, and ambiguous forms remain
diagnosed. The design is `36085e9c`, implementation is `f2f99f66`, and focused,
full, strict, documentation, and exact target-cleanup evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-085](tasks/native-engine-085.md)
slice defines bounded `place-content` expansion into the existing
`align-content` and `justify-content` components. One shared token supports
their common values; two tokens use explicit cross-axis/main-axis order.
Unsupported CSS-wide, logical, safe/unsafe, ambiguous, and unsupported justify
forms remain diagnosed. The design is `adc61a1f`, implementation is
`02f866e6`, and focused, full, strict, documentation, and cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-086](tasks/native-engine-086.md)
slice extends bounded non-inherited `align-self` with `stretch` for eligible
direct flex items. Auto-height items fill the existing line cross size while
explicit heights keep their declared size and use the bounded flex-start
fallback. The design checkpoint is `f92b7b9a`, implementation is `5ea2c8d1`,
and strict layout lint cleanup is `2c07c749`; focused, full, strict,
documentation, release-certification, and exact target-cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-087](tasks/native-engine-087.md)
slice extends parent `align-items` with explicit `stretch`. Children whose
`align-self` remains `auto` reuse the completed 086 used-size path; explicit
child overrides remain authoritative, explicit heights stay fixed, and the
omitted native fallback remains `flex-start`. Design is `4708f663`,
implementation is `e0d051ce`, and focused/full/strict/release-certificate,
documentation, and exact-target evidence are recorded in the task file. The
first all-in-one certification run exposed one environment-sensitive Rust
Analyzer probe; its exact retry passed. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-088](tasks/native-engine-088.md)
slice adds explicit parent `align-items:normal`. In the supported row and
row-reverse flex context, `normal` resolves auto-aligned children through the
completed stretch path while the computed value remains distinct and the
omitted native fallback stays `flex-start`. Design is `162543f1`, implementation
is `19d8d3f6`, and focused, full, strict, documentation, and exact-target
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-089](tasks/native-engine-089.md)
slice adds explicit child `align-self:normal`. In the supported row and
row-reverse flex context, the explicit item value reuses the completed stretch
used-size path regardless of the parent's `align-items` value, while omitted
`align-self:auto` remains parent-controlled and the computed keyword remains
distinct. Design is `95caf4a9`, implementation is `1ee55c43`, and focused,
full, strict, documentation, and exact-target evidence are recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-090](tasks/native-engine-090.md)
slice adds explicit `justify-content:space-around` to the bounded fixed-width
row and row-reverse flex-line owner. Positive main-axis free space is
distributed with deterministic integer cumulative offsets around the existing
item/gap/margin and flex-sizing geometry; row-reverse mirrors the offsets and
all downstream artifacts remain shared. Design is `97c69d9d`, implementation is
`d814784f`, and focused, full, strict, documentation, and exact-target evidence
are recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-091](tasks/native-engine-091.md)
slice adds explicit `justify-content:space-evenly` to the same bounded
fixed-width row and row-reverse flex-line owner. Positive main-axis free space
is distributed into deterministic equal integer slots after existing
item/gap/margin and flex-sizing geometry, while preserving shared layout,
paint, hit-test, scroll, capture, and semantic/source-order consumers. Design
is `be1a8e20`, implementation is `118590f7`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-092](tasks/native-engine-092.md)
slice adds explicit `justify-content:normal` to the same bounded row and
row-reverse owner. It preserves a distinct computed keyword while routing
used placement through the completed `flex-start` geometry, and makes the
shared one-token `place-content:normal` expansion valid. Design is
`4cbaf338`, implementation is `ce19db39`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-093](tasks/native-engine-093.md)
slice adds explicit `justify-content:stretch` to the same bounded row and
row-reverse owner. It preserves a distinct computed keyword while routing used
placement through the completed `flex-start` geometry, and makes the shared
one-token and two-token `place-content:stretch` forms valid. Design is
`273b31db`, implementation is `653f025e`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-094](tasks/native-engine-094.md)
slice extends the bounded Flexbox owner to explicit `flex-direction:column` and
`column-reverse` for fixed-height, no-wrap containers. It maps existing
vertical main-axis justification, row-gap, grow/shrink/basis, cross-axis item
alignment, descendants, and shared artifacts without adding a second geometry
owner. Auto-height columns, wrapping, column-gap line distribution, logical
writing modes, and browser-wide Flexbox remain outside the contract. Design is
`aea47b17`, implementation checkpoints are `7dc92517` and `4e212151`, and the
focused, full, strict, documentation, validator, binary, and exact-target
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-095](tasks/native-engine-095.md)
slice extends column and column-reverse flex to fixed-height `flex-wrap:wrap`
containers. It forms vertical main-axis lines, maps `column-gap` across the
horizontal cross axis, reuses per-line flex sizing/justification and existing
`align-content`, alignment, and shared artifact consumers. `wrap-reverse`,
auto-height columns, and browser-wide Flexbox remain outside this contract.
The design is `598228a7`, implementation is `96ea62a3`, and focused/full
native, feature-library, strict-Clippy, and exact fallback evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-096](tasks/native-engine-096.md)
slice extends that owner to fixed-height column and column-reverse
`flex-wrap:wrap-reverse`. It reflects the formed horizontal line boxes and
cross-axis item alignment while preserving 095 line formation, gaps,
`align-content`, main-axis reversal, complete artifacts, and source/semantic
order. Auto-height columns, intrinsic or percentage sizing, logical writing
modes, and browser-wide Flexbox remain outside the contract. The design is
`f5026f3c`, implementation is `6862aff6`, and focused/full native,
feature-library, strict-Clippy, and exact fallback/cleanup evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-097](tasks/native-engine-097.md)
slice extends the existing no-wrap row, row-reverse, column, and
column-reverse owners to bounded `margin:auto` edges. Auto margins are zero
during flex sizing, then absorb positive main-axis space before
`justify-content` and positive cross-axis space before `align-items`/
`align-self`, with deterministic integer remainder allocation and complete
artifact consumers. Wrapped lines, auto-height columns, intrinsic or
percentage sizing, and normal-flow auto margins remain outside the contract.
The design is `2901c830`, the implementation is `815794ce`, and focused/full
native, feature-library, strict-Clippy, rustdoc, binary, validator, and
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered [native-engine-098](tasks/native-engine-098.md)
slice extends that owner to line-local `margin:auto` resolution for eligible
wrapped row/row-reverse and fixed-height column/column-reverse containers.
Auto edges remain zero during line formation and per-line sizing, then absorb
positive main-axis remainder before justification and positive cross-axis
remainder before item alignment after the existing line and `align-content`
owners have settled. `wrap-reverse`, reverse physical edges, deterministic
integer shares, and complete artifact consumers remain in the same geometry
path. Auto-height columns, new intrinsic/percentage sizing, normal-flow auto
margins, and browser-wide Flexbox remain outside the contract. The design is
`a4b05f07`, implementation is `77a4b629`, and the final test-only checkpoint
is `866a8862`. Focused/full native, feature-library, strict-Clippy, rustdoc,
binary, package/dependency, fuzz, documentation, and exact-target cleanup
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-099](tasks/native-engine-099.md)
slice adds inherited `direction:ltr|rtl` to the existing bounded Flexbox axis
mapping. Rows use the current inline direction for their physical main start;
columns preserve their vertical main axis while reflecting horizontal
cross-axis alignment and wrapped line stacking. Source/semantic order,
non-flex text bidi, vertical writing modes, and browser-wide directionality
remain outside the contract. The design checkpoint is `2f18abb4`, the
implementation is `3bf658e8`, and the complete local gate evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-100](tasks/native-engine-100.md)
slice adds bounded inherited `text-align:start|end` to the fixed-cell
inline-flow owner, resolving logical start/end through inherited
`direction:ltr|rtl` while preserving physical `left|right`, center, source
order, wrapped-line behavior, and the existing no-bidi/shaping boundary. The
design checkpoint is `2dae80fc`, the implementation is `3380978c`, and the
complete local gate and paired-package evidence is recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-101](tasks/native-engine-101.md)
slice adds bounded inherited `text-align:justify` to the fixed-cell inline-flow
owner. Only eligible collapsed ASCII separators on soft-wrapped non-final
lines receive deterministic integer expansion; preformatted flow, bidi,
shaping, and the broader text-conformance matrix remain outside the contract.
The design is `959cbbc9`, implementation is `8ff29aa1`, and the explicit
word-spacing acceptance test is `15cf0c85`; the complete local gate evidence is
recorded in the task file. Exact isolated-target cleanup is recorded in the
final cleanup checkpoint. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-102](tasks/native-engine-102.md)
slice adds bounded inherited `text-align-last:auto|left|center|right|start|end`
to the same fixed-cell inline-flow owner. Only the final non-empty line flushed
by a block's normal completion path uses the explicit value; 101 soft-wrap
justification, forced-break paths, bidi/shaping, and full text conformance
remain bounded as documented. The design is `fc396200`, implementation is
`1157bf49`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-103](tasks/native-engine-103.md)
slice adds bounded inherited `text-align-last:justify` to the same fixed-cell
inline-flow owner. Only the final non-empty line flushed by a block's normal
completion path may distribute positive free space across eligible collapsed
ASCII separators; 101 soft-wrap justification and 102 physical/logical final
alignment remain bounded as documented. The design is `f261773f`, implementation
is `be5757ae`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-104](tasks/native-engine-104.md)
slice adds bounded inherited `text-justify:auto|none|inter-word` to the same
fixed-cell text-spacing owner. `none` suppresses the existing separator
expansion for ordinary soft-wrap and explicit final-line justification;
`auto` and `inter-word` retain the bounded ASCII-space algorithm. The design is
`6ca523f1`, implementation is `d83b24e4`, and complete local gate plus
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered [native-engine-105](tasks/native-engine-105.md)
slice extends the inherited fixed-cell `text-decoration` owner from
`none|underline` to the bounded single values
`none|underline|overline|line-through`. The line state flows through the
existing immutable text commands, clipping, scrolling, opacity replay, and
software raster without changing layout or semantic/source order. Decoration
colors, thickness, style, offsets, combinations, font metrics, shaping, bidi,
vertical writing, and browser-wide text conformance remain outside the
contract. Design is `9002ae13`, implementation is `ebfefefa`, and complete
local gate plus exact-target cleanup evidence is recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-106](tasks/native-engine-106.md)
slice extends the inherited fixed-cell `text-decoration` owner to distinct
multi-token combinations in the `text-decoration` shorthand. The bounded
three-bit line set flows through the existing immutable text commands,
clipping, scrolling, opacity replay, capture, and software raster without
changing layout or semantic/source order. `text-decoration-line` longhand
semantics, decoration colors, thickness, style, offsets, font metrics,
shaping, bidi, vertical writing, and browser-wide text conformance remain
outside the contract. Design is `c0525afb`, implementation is `baf680ee`,
and complete local gate plus exact-target cleanup evidence is recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-107](tasks/native-engine-107.md)
slice extends the inherited fixed-cell decoration owner with a local
`text-decoration-color` value. The existing bounded `NativeColor` grammar
feeds a separate decoration color beside glyph color in one immutable text
command, preserving shared geometry, clipping, scrolling, opacity, capture,
and raster consumers while separating glyph and line pixels. Explicit
decoration-origin propagation, `currentColor` syntax, decoration
style/thickness/offset, and full color/text conformance remain outside the
contract. Design is `c5177215`, implementation is `2474efe6`, and the local
144/144 native integration plus current workspace gate evidence is recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-108](tasks/native-engine-108.md)
slice exposes the same fixed-cell line bitset through the bounded
`text-decoration-line` longhand. `none`, `underline`, `overline`, and
`line-through` combinations reuse the shorthand's declaration-order-aware
slot and one immutable text command, preserving the 107 glyph/decoration color
path and all shared artifact consumers. Full CSS longhand
inheritance/decoration propagation, style/thickness/offset, and text
conformance remain outside the contract. Design is `ea1bf881`, implementation
is `4981ff82`, and complete local gate evidence is recorded in the task file;
the native integration suite passed 145/145 and the full browser/dev plus
strict docs, package, fuzz, and static gates passed. Remote CI remains pending
because the branch is local-only.

The completed dependency-ordered [native-engine-109](tasks/native-engine-109.md)
slice adds bounded inherited `text-decoration-style:solid|dashed|dotted` to
the existing fixed-cell decoration owner. Solid remains the default; dashed
and dotted reuse the existing integer border-pattern helper with one-pixel
decoration lines anchored at each emitted run origin. Wavy/double styles,
thickness, offsets, decoration-origin propagation, and browser-wide CSS
conformance remain outside the contract. Design `93034cbf`, implementation
`81069084`, inherited-style coverage `b8ae87dc`, and docs closeout are recorded
in the task file. Local native, browser/dev, package, fuzz, static, and
formatting gates passed; exact-target cleanup is recorded there. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-110](tasks/native-engine-110.md)
slice adds bounded inherited `text-decoration-thickness:1px|2px|3px|4px` to
the existing fixed-cell decoration owner. Each selected line keeps its
existing y origin and paints a positive-y pixel band; dashed and dotted
periods reuse the existing integer helper scaled by thickness, and replay
clamps externally constructed commands to the same 4px ceiling. Arbitrary,
font-derived, fractional, negative, zero, offset, baseline, and browser-wide
CSS decoration semantics remain outside the contract. Design `1623c5f1`,
implementation `157da4ad`, complete local gate evidence, and exact-target
cleanup are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-111](tasks/native-engine-111.md)
slice adds bounded inherited signed fixed-pixel
`text-underline-offset:-4px..=4px` to the existing fixed-cell underline owner.
Negative offsets move the underline toward decreasing y and positive offsets
toward increasing y; overline and line-through origins remain unchanged, and
the 110 thickness/style helper is reused. `auto`, percentages, fractional
values, font-derived metrics, decoration-origin propagation, and browser-wide
CSS semantics remain outside the contract. Implementation is `215b02a8`,
current-claim docs are `e76a732a`, and complete local gate and cleanup evidence
are recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-112](tasks/native-engine-112.md)
slice adds inherited `text-decoration-style:double` through a dedicated
text-decoration style type and the existing immutable text/raster path: two
solid bands, each retaining the resolved `1px..=4px` thickness, separated by
one pixel. Border styling, layout, line origins, and all existing artifact
consumers remain unchanged. Implementation is `3bdd3b54`; final local gate and
cleanup evidence are recorded in the task file. Remote CI remains pending; the
checkout is local-only.

The completed dependency-ordered [native-engine-113](tasks/native-engine-113.md)
slice adds inherited `text-decoration-style:wavy` through the existing
dedicated text-decoration style type and immutable text/raster path: a
continuous eight-pixel wave with the fixed phase
`[0,1,2,1,0,-1,-2,-1]`, applying the resolved thickness at each x column.
Run-origin phase resets, underline offset, line origins, clipping, scrolling,
opacity, capture, hit testing, and semantic/source order remain shared; CSS
metric centering, fragment continuity, antialiasing, and browser-wide
conformance remain outside the contract. Implementation is `0c6a9ddc`; final
local gate and cleanup evidence are recorded in the task file. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-114](tasks/native-engine-114.md)
implementation is complete. It
adds inherited
`text-decoration-skip-ink:auto|none` through a dedicated value in the existing
immutable text/raster path. `auto` suppresses underline and overline pixels
only where the same fixed-cell text run emits glyph ink; `none` preserves
existing replay and line-through remains unchanged. Wavy, thickness, offset,
clipping, scroll, opacity, capture, hit testing, semantics, source order, and
layout remain shared; font metrics, shaping, fragment continuity, and
browser-wide conformance remain outside the contract. Implementation is
checkpointed at `ceedf1d8` and synchronized documentation at `d276d7b1`; all
required local certification gates pass. Exact cleanup evidence is recorded in
the task file; remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-115](tasks/native-engine-115.md)
implementation is complete. It adds inherited
`text-decoration-skip-spaces:none|all` through the existing immutable
text/raster path. `all` skips decoration pixels over same-run ASCII-space
intervals, including word, letter, and final-line justification spacing, for
underline, overline, and line-through; `none` preserves replay. `start`/`end`,
Unicode whitespace, line-boundary semantics, fragment continuity, and
browser-wide text conformance remain outside the bounded design. Implementation
is checkpointed at `1c0bd484`; local native/dev, strict, package, fuzz,
security, formatting, and static certification is complete and exact cleanup is
recorded in the task file. The browser registry-backed publish dry-run passed;
the dev registry-backed verification is blocked by the immutable public
`glass-browser 0.3.14` API surface, while the local dev no-verify packaging
dry-run passed. No upload was attempted. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-116](tasks/native-engine-116.md)
implementation is complete. It extends inherited
`text-decoration-skip-spaces` with explicit `start`, `end`, and unordered
`start end` line-edge modes. The authoritative block-owned flow flush marks
the first and last text items and carries immutable provenance alongside
display-list text commands, so raster replay skips only leading/trailing
fixed-cell ASCII-space intervals while the 115 `none|all` behavior remains
unchanged; nested inline temporary flows cannot claim a line edge. Implementation
is checkpointed at `a671a559` and `db7585f7`; local native/dev, strict, package,
fuzz, security, formatting, and static certification is recorded in the task
file. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-117](tasks/native-engine-117.md)
implementation is complete at `5e65aadf`. It extends the 116 decoration
replay classifier to Rust's bounded Unicode `char::is_whitespace()` property
for literal and preformatted fixed-cell text, so tabs and non-breaking spaces
can participate in `all` or selected line-edge skipping. Normal collapsing,
line provenance, geometry, spacing arithmetic, and the explicit omission
fallback remain unchanged. Focused, full-native, two-crate, package, fuzz,
security, and static local certification passed; exact evidence and cleanup
are recorded in the task.

The dependency-ordered [native-engine-118](tasks/native-engine-118.md)
implementation is complete at `6f8e89fc`. It accepts the explicit
case-insensitive `text-decoration-skip-spaces: initial` keyword, mapping it to
the existing `start end` computed value while preserving the deliberate
omitted-property `none` fallback. General CSS-wide keyword machinery and all
layout, raster, dependency, feature-default, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task.

The dependency-ordered [native-engine-119](tasks/native-engine-119.md)
implementation is complete at `1118bf2b`. It accepts explicit,
case-insensitive `text-decoration-skip-spaces: inherit` through a private
declaration-only value resolved at the existing parent-style boundary,
keeping the public finite paint enum and all artifact consumers unchanged.
`unset`, `revert`, `revert-layer`, general CSS-wide keyword machinery,
layout/raster changes, new dependencies, default-feature changes, and
crate-boundary changes remain outside this slice. Focused, full-native,
two-crate, strict, package, fuzz, security, formatting, and static local
certification passed; exact evidence and cleanup are recorded in the task.

The dependency-ordered [native-engine-120](tasks/native-engine-120.md)
implementation is complete at `897bd648`. It accepts explicit
case-insensitive `text-decoration-skip-spaces: unset` through the same private
declaration-only value and resolves it as inherited parent state, keeping the
public finite paint enum and all artifact consumers unchanged. `revert`,
`revert-layer`, general CSS-wide keyword machinery, layout/raster changes, new
dependencies, default-feature changes, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-123](tasks/native-engine-123.md)
implementation is complete at `af644112`. It reuses the 122 layer registry
and private rollback state for inherited `text-decoration-skip-ink:
revert-layer`, preserving the finite `Auto|None` paint value,
glyph-intersection replay, and the unlayered/inline layer boundary. Parser,
cascade, display-list, and decoded-raster regressions passed; general CSS-wide
keyword machinery, multiple origins, layer statements, and unsupported values
remain outside the slice. Exact local gate and cleanup evidence is recorded in
the task; remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-124](tasks/native-engine-124.md)
implementation is complete at `d6c8bc70`. It reuses the 122 layer registry and
the private rollback boundary proven by 123 for inherited
`text-decoration-style: revert-layer`, preserving the finite
`Solid|Dashed|Dotted|Double|Wavy` paint value, existing fixed-cell pattern
replay, and the unlayered/inline layer boundary. Parser, cascade, display-list,
command, and decoded-raster regressions passed; general CSS-wide keyword
machinery, multiple origins, layer statements, and unsupported values remain
outside the slice. Exact local gate, documentation-audit, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered [native-engine-125](tasks/native-engine-125.md)
implementation is complete at `271702ae`. It reuses the bounded layer registry
and private rollback boundary proven by 124 for inherited
`text-decoration-thickness: revert-layer`, preserving the finite `1px` through
`4px` value, existing one-to-four-cell decoration geometry, and the
unlayered/inline layer boundary. Parser, cascade, display-list, command, and
decoded-raster regressions passed for underline, overline, and line-through;
general CSS-wide keyword machinery, multiple origins, layer statements, and
unsupported values remain outside the slice. Exact local gate, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered [native-engine-126](tasks/native-engine-126.md)
implementation is complete at `3ffe86f8`. It reuses the bounded layer registry
and private rollback boundary proven by 125 for inherited
`text-underline-offset: revert-layer`, preserving the finite signed `-4px`
through `4px` value, existing underline-only translation, and the
unlayered/inline layer boundary. Parser, cascade, display-list, command, and
decoded-raster regressions passed; general CSS-wide keyword machinery, multiple
origins, layer statements, and unsupported values remain outside the slice.
Exact local gate, issue-sync, and regenerable-output cleanup evidence is
recorded in the task. The dependency-ordered
[native-engine-127](tasks/native-engine-127.md) implementation is complete at
`50a36545`. It reuses the bounded layer registry for local
`text-decoration-color: revert-layer`, preserving the existing
`Option<NativeColor>` no-candidate fallback and separate glyph/decoration paint
owner. Parser, cascade, display-list, command, and decoded-raster regressions
passed; general CSS-wide keyword machinery, `currentColor`, multiple origins,
layer statements, and unsupported values remain outside the slice. Exact local
gate, issue-sync, and regenerable-output cleanup evidence is recorded in the
task.

The dependency-ordered [native-engine-128](tasks/native-engine-128.md)
implementation is complete at `15fc761c`. It reuses the bounded layer registry
for case-insensitive `text-decoration-line: revert-layer` and
`text-decoration: revert-layer`, preserving their shared inherited three-bit
line-state owner, declaration-order interaction, unlayered/inline bucket, and
existing display-list, command, and fixed-cell raster geometry. Parser,
cascade, inherited fallback, and decoded-raster regressions passed; other
CSS-wide keywords, multiple origins, layer statements, and unsupported values
remain typed diagnostics. Exact local gate, documentation-audit, issue-sync,
and regenerable-output cleanup evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-129](tasks/native-engine-129.md)
implementation is complete in `d26033af`, with its fixture assertion correction
in `c32aeafe` and diagnostic-classifier fix in `36a0f68`. It adds private
case-insensitive `revert-layer` declarations for inherited `text-align`,
`text-align-last`, and `text-justify`, reusing the bounded 15-layer and
unlayered/inline cascade boundary while preserving direction mapping,
final-line alignment, separator justification, finite public values, and the
existing fixed-cell line/artifact owner. Focused and full affected-package
local gates are recorded in the task; remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-130](tasks/native-engine-130.md)
implementation is complete in `d7f4d7ca`. It adds private case-insensitive
`white-space: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary. It preserves the five finite whitespace
modes, inherited/root fallback, existing hard-break and fixed-cell wrapping
behavior, and the current line/artifact owner. Focused and complete affected-
package local gates passed; the package library gate used an explicit 32 MiB
test-thread stack to accommodate one pre-existing CLI stack-overflow test.
Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-131](tasks/native-engine-131.md)
implementation is complete in `e35a13fd`. It adds private case-insensitive
`line-height: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary. It preserves the positive-pixel line-height
grammar, inherited/root `Option<u32>` fallback, inline auto-height and
explicit-height precedence, and the current flow/artifact owner. Focused and
complete affected-package local gates passed; the package library gate used an
explicit 32 MiB test-thread stack to accommodate one pre-existing CLI
stack-overflow test. Remote CI remains pending because the checkout is
local-only.

The dependency-ordered [native-engine-132](tasks/native-engine-132.md)
implementation is complete in `4a46862f`. It adds private case-insensitive
`direction: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary while preserving the finite `ltr|rtl` value,
logical text-edge mapping, flex directionality, wrapped-line placement,
source/semantic order, and current layout/artifact owners. Focused,
full-native, affected-library, and strict affected-package local gates passed;
exact test and cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.

The dependency-ordered [native-engine-133](tasks/native-engine-133.md)
implementation is complete in `56944c83`. It extends the bounded private
layer resolver to non-inherited `flex-direction: revert-layer`, preserving
finite row/row-reverse/column/column-reverse placement, the local `row`
fallback, finite `flex-flow` expansion, and the existing wrapped-flex/artifact
owners. Focused, full-native, affected-library, and strict affected-package
local gates passed; exact test and cleanup evidence is recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-134](tasks/native-engine-134.md)
implementation is complete in `f2e20121`. It extends the bounded private
layer resolver to non-inherited `flex-wrap`, `justify-content`, `align-items`,
`align-self`, and `align-content`, preserving their native fallbacks, finite
`flex-flow`/`place-content` expansion, and existing wrapped-flex/artifact
consumers. Focused, full-native, affected-library, and strict affected-package
local gates passed; exact test and cleanup evidence is recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-135](tasks/native-engine-135.md)
implementation is complete in `74195032`. It extends the bounded private
layer resolver to non-inherited `order`, `flex-grow`, `flex-shrink`, and
`flex-basis`, preserving finite `flex` expansion, local fallbacks, stable
visual order, grow/shrink allocation, base-size selection, min/max constraints,
and existing layout/artifact consumers. Focused, full-native,
affected-library, and strict affected-package local gates passed; exact test
and cleanup evidence is recorded in the task. Remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-136](tasks/native-engine-136.md)
implementation is complete in `710ed3bb`. It adds standalone
case-insensitive `flex:revert-layer` through the existing private
grow/shrink/basis rollback components, preserving finite shorthand expansion,
same-block longhand precedence, local fallbacks, and the current
layout/artifact owners. Focused, full-native, affected-library, and strict
affected-package local gates passed; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-137](tasks/native-engine-137.md)
implementation is complete in `7da4dfd5` (design `79e2d2fa`). It adds
standalone case-insensitive `flex-flow:revert-layer` and
`place-content:revert-layer` through the existing private direction/wrap and
align-content/justify-content rollback components, preserving finite shorthand
expansion, same-block longhand precedence, independent local fallbacks, and
the current flex layout/artifact owners. Focused parser/cascade, integration,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed; exact evidence and cleanup are recorded in the
task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-138](tasks/native-engine-138.md) implementation is complete in `bded96ae` (design `656dc37d`). It adds standalone case-insensitive `gap:revert-layer`, `row-gap:revert-layer`, and `column-gap:revert-layer` through private row/column component candidates, preserving finite integer-pixel expansion, same-block shorthand/longhand precedence, independent zero fallback, and the current flex layout/artifact owners. Focused parser/cascade, integration, full-native, affected-library, strict Clippy, formatting, and static documentation gates passed; exact evidence and cleanup are recorded in the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-139](tasks/native-engine-139.md)
implementation is complete in `0114659d` (design `16589ae4`). It adds
standalone case-insensitive `revert-layer` to inherited `text-transform`,
`font-weight`, `font-style`, and `word-break` through private per-property
candidates, preserving finite public values, parent/root fallback, and the
existing fixed-cell layout, wrapping, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners. Unicode case mapping, font metrics,
other word-break modes, multiple origins, and browser-wide text conformance
remain outside the boundary; exact implementation, validation, and cleanup
evidence are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-140](tasks/native-engine-140.md)
implementation is complete in `7d40cf87` (design `34f8ec1a`). It adds
standalone case-insensitive `revert-layer` to inherited `word-spacing` and
`letter-spacing` through private per-property candidates, preserving finite
non-negative pixel values, parent/root fallback, and the existing text-flow,
wrapping, alignment, display-list, raster, overflow, capture, hit-test, and
semantic/source-order owners. The shared parser also preserves an earlier
valid inherited-text declaration when a later declaration is invalid.
Negative, relative, percentage, fractional, cross-fragment, font-metric,
multi-origin, and browser-wide text semantics remain outside the boundary;
exact implementation, validation, and cleanup evidence are recorded in the
task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-141](tasks/native-engine-141.md)
implementation is complete in `271bfaf2` (design `0d1f7281`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to inherited
`vertical-align` through private candidates, preserving finite
`baseline|top|middle|bottom` values, parent/root fallback, and the existing
inline line-item, text-fragment, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners. Baseline metrics, lengths,
percentages, bidi, writing modes, multiple origins, and browser-wide text
conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote
CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-142](tasks/native-engine-142.md)
implementation is complete in `be860447` (design `2794365f`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to the local
`text-indent` and `text-overflow` owners through private candidates, preserving
finite non-negative fixed-pixel indentation, `clip|ellipsis`, local `0px`/`clip`
fallbacks, and the existing first-line flow, eligible clipped-nowrap
truncation, text-fragment, display-list, raster, overflow, capture, hit-test,
and semantic/source-order owners. Negative or hanging indentation, percentages,
font-relative units, inherited text-overflow, marker customization, multiple
origins, and browser-wide text conformance remain outside the boundary.
Focused, full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-143](tasks/native-engine-143.md)
implementation is complete in `b55751da` (design `edc29d7c`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to the local
`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`
owners through independent private candidates, preserving finite non-negative
pixel dimensions, absent local fallbacks, and the existing box-model,
normal-flow, flex, overflow, capture, hit-test, display-list, raster, and
semantic/source-order owners. Percentages, negative dimensions, intrinsic
sizing, aspect ratio, multiple origins, and browser-wide CSS sizing conformance
remain outside the boundary. Focused, full-native, affected-library, strict
Clippy, formatting, and static documentation gates passed locally; exact
evidence and cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-144](tasks/native-engine-144.md)
implementation is complete in `7ce9c52c` (design `255a1ac8`) and recorded in
the task. It extends the same private bounded layer resolver to standalone
case-insensitive `revert-layer` for local `box-sizing`, physical padding and
margin edges, including shorthand/longhand rollback and bounded `margin:auto`,
preserving independent edge ownership, content-box/zero local fallbacks, and
the existing box-model, normal-flow, flex, overflow, capture, hit-test,
display-list, raster, and semantic/source-order owners. Percentages,
negative/logical edges, margin collapsing, positioned or replaced-element
sizing, multiple origins, `!important` inversion, layer statements, and
browser-wide box-model conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-145](tasks/native-engine-145.md)
implementation is complete in `997d4aa7` (design `553c5f89`) and is recorded
in the task. It reuses the same private bounded layer resolver for standalone
case-insensitive `revert-layer` on local `background-color` and inherited
`color`, preserving independent `None`/inherited fallbacks and the existing
fill/text display-list, capture, raster, clipping, opacity, hit-test, and
semantic/source-order owners. Border-color, `currentColor`, gradients, system
colors, multiple origins, and browser-wide CSS color conformance remain
outside the boundary. Focused, full-native, affected-library, strict Clippy,
formatting, and static documentation gates passed locally; exact evidence and
cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-121](tasks/native-engine-121.md)
implementation is complete at `f361415a`. It accepts explicit case-insensitive
`text-decoration-skip-spaces: revert` through a distinct private
declaration-only value and resolves it at the current one-author-origin
inherited fallback boundary, keeping the public finite paint enum and all
artifact consumers unchanged. `revert-layer`, cascade layers, multiple style
origins, general CSS-wide keyword machinery, layout/raster changes, new
dependencies, default-feature changes, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-122](tasks/native-engine-122.md)
implementation is complete at `1291fc2c`, with the strict-Clippy parser-context
follow-up at `efb5bdfc`. It adds bounded top-level named cascade layers,
private first-appearance priority ahead of selector specificity, a 15-layer
limit, and `text-decoration-skip-spaces: revert-layer` rollback through lower
candidates and the existing inherited/root fallback while preserving the
unlayered/inline bucket above named layers. Layer statements, anonymous/comma/
nested layers, multiple origins, and general CSS-wide keyword machinery remain
outside the slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

## Historical plan: Glass v0.3.6 issue #36

Status: Historical/superseded — this 0.3.6 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue [#36](https://github.com/wanazhar/glass/issues/36) is the authoritative
urgent product-repair contract. The 35-pillar baseline, locked architecture,
dependency order, scenarios A-J, and 15 release gates are mapped in
[the v0.3.6 delivery analysis](analysis/release-036.md). The serial tasks are:

1. [correctness-036-001](tasks/correctness-036-001.md) — lifecycle, operations,
   verification, Pi setup, and semantic documentation truth;
2. [browser-workspace-036-002](tasks/browser-workspace-036-002.md) — one shared
   browser controller/view and standalone/embedded parity;
3. [product-ux-036-003](tasks/product-ux-036-003.md) — Agent, Code, App,
   Terminal, Tasks, Git, Debug and contextual desktop interaction;
4. [mobile-onboarding-036-004](tasks/mobile-onboarding-036-004.md) — intentional
   phone navigation, onboarding, and searchable palette;
5. [certification-036-005](tasks/certification-036-005.md) — integrated, PTY,
   remote-boundary, package, exact-tag, publication, and release evidence.

Each completed checkpoint receives a focused conventional commit before the
next task starts.

## Historical plan: Glass v0.3.5 issue #35

Status: Historical/superseded — this 0.3.5 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue [#35](https://github.com/wanazhar/glass/issues/35) is the authoritative
sixteen-pillar trust and runtime-convergence contract. The audited baseline,
dependency order, twelve release gates, scenarios A-I, forbidden outcomes, and
disk-aware validation policy are in [the v0.3.5 delivery analysis](analysis/release-035.md).
The first release-blocking checkpoint is the
[workspace trust boundary](tasks/security-035-001.md).
The [native Pi SDK boundary](tasks/runtime-035-002.md) and
[product-boundary deletion](tasks/product-boundary-035-013.md) are complete in
the source candidate.
The [autonomous task and workspace actor checkpoint](tasks/scheduler-035-003.md)
is also complete locally and removes the global daemon workspace lock.
The [native transport checkpoint](tasks/platform-035-004.md) implements Windows
named pipes; only a remote native Windows run can certify that platform.
The [automatic experiment evidence checkpoint](tasks/experiments-035-005.md)
adds measured provenance and trusted deterministic ranking.

Delivery order:

1. workspace trust and customization authority;
2. native Pi SDK and completed package ownership migration;
3. autonomous verified task DAGs;
4. per-workspace daemon actors and native Windows IPC;
5. automatic experiments, DAP breadth, kernel bindings, graph/replay;
6. TUI parity, stress scenarios, documentation, packaging, and release gates.

Each completed checkpoint receives a focused local conventional commit before
the next checkpoint starts.

## Historical plan: Glass v0.3.4 issue #34

Status: Historical — released and publicly verified on 2026-08-10.

Issue [#34](https://github.com/wanazhar/glass/issues/34) is the authoritative
18-pillar full-agentic-development-suite contract. The baseline inventory,
dependency order, checkpoint boundaries, forbidden outcomes, and release gate
evidence map are in [the v0.3.4 delivery analysis](analysis/release-034.md).
The live audit is in the [v0.3.4 gate review](reviews/release-034-gates.md).

Delivery order:

1. ownership and TUI boundary decomposition;
2. native Pi sessions, governed resident tools, and agent scheduling;
3. shared LSP, real DAP, Git, tests, and persistent kernels;
4. durable workspace ownership, experiments, graph, and replay;
5. agent-native desktop/mobile surfaces and integrated browser/workflow tools;
6. synchronized 0.3.4 packaging, full-system demonstrations, and release gates.

Every completed delivery checkpoint is committed locally with a focused
conventional commit before the next checkpoint begins.

## Historical plan: Glass 0.3.3 documentation depth

Status: Historical — completed and verified locally as documentation-only, with
no push, tag, publication, release, or issue mutation.

The [depth audit](analysis/documentation-depth-035.md) accounts for every
public documentation surface and replaces name-presence checks with workflow,
state, limit, failure, recovery, and exact nested-command contracts. The
implementation record is
[documentation-depth-035-001](tasks/documentation-depth-035-001.md).

## Historical plan: Glass v0.3.3 issue #33

Status: Historical/superseded — this 0.3.3 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue #33 and its authoritative amendment define 15 integration pillars, 53
mandatory release checkboxes, scenarios A–K, full-suite command exposure and
zero-exit browser recovery. The complete defect baseline, integration chains,
TUI inspiration decisions and evidence matrix are in
[analysis/release-033.md](analysis/release-033.md); the auditable 53/53 mapping
is in [reviews/release-033-gates.md](reviews/release-033-gates.md).

Delivery order:

1. [presentation-033-001](tasks/presentation-033-001.md) — independent
   connection dimensions, policy matrix, observatory and correct local pacing.
2. [tui-recovery-033-002](tasks/tui-recovery-033-002.md) — phone shell,
   command palette, browser controller, recovery sheet and target picker.
3. [remote-agent-033-003](tasks/remote-agent-033-003.md) — secure Remote View
   and live attachment-aware agent context.
4. [runtime-033-004](tasks/runtime-033-004.md) — process trees, project tree,
   persistent LSP and real embedded Neovim proof.
5. [release-033-005](tasks/release-033-005.md) — full-suite binaries,
   cross-platform/tool CI, black-box demonstrations, docs and final validation.

## Historical plan: complete public documentation and docs.rs revamp

Status: Historical — completed and verified locally; no remote mutation.

The [documentation audit](analysis/documentation-revamp-034.md) and
[implementation task](tasks/documentation-034-001.md) cover every current
user, operator, SDK, MCP, TUI, package, rustdoc, example, and release-reference
surface, with generated drift checks against the implementation.

## Historical plan: semantic resource and correctness audit

Status: Historical — completed and verified locally; no remote mutation.

The [resource audit](analysis/semantic-resource-audit.md) and atomic
[implementation task](tasks/semantic-resource-033-002.md) optimize the task
compiler, live binding, agent gateway, and private Pi request boundary while
preserving the completed semantic-core contracts.

## Historical plan: semantic core hardening

Status: Historical — completed and verified locally; no remote mutation.

The [delivery analysis](analysis/semantic-core-hardening.md) and atomic
[implementation task](tasks/semantic-core-033-001.md) cover executable Web IR
evidence, relationship-scoped compilation, revision-bound execution,
capability and state fidelity, continuity, and bounded Local/Pi semantic tools.

## Historical/superseded post-0.1.18 roadmap
Status: Historical/superseded — the roadmap body records delivered 0.2.x work
and later issue tracking; it is not the current source-line plan.

Issue #21 was delivered as a serial workflow-runtime sprint. Issues #25 and
#26 were included in the published 0.2.0 release; their remaining artifact and
cross-platform evidence is tracked by issue #28. The reliability task records
are [scenario contract](tasks/reliability-026-001.md),
[adversarial fixture](tasks/reliability-026-002.md), [certification gate](tasks/reliability-026-003.md),
and [capability/replay work](tasks/reliability-026-004.md). Issue #27 is now
active; its first foundation phase is [stable runtime platform](tasks/platform-027-001.md).

## Historical plan: Glass v0.3.1 issue #31

Status: Historical/superseded — this 0.3.1 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

The authoritative [issue #31](https://github.com/wanazhar/glass/issues/31)
defines four mandatory pillars—semantic memory, multi-surface understanding,
runtime survivability beyond CDP, and the Ratatui-native Browser Workspace—
plus the cross-cutting Glass Experience Layer. The delivery analysis,
integration inventory, and dependency order are in
[analysis/release-031.md](analysis/release-031.md).

Foundation wave:

1. [memory-031-001](tasks/memory-031-001.md) — surface/backend-aware
   knowledge provenance and explainability.
2. [surface-031-001](tasks/surface-031-001.md) — bounded multi-surface
   contract.
3. [backend-031-001](tasks/backend-031-001.md) — transport-neutral Browser
   Capability Interface.
4. [presentation-031-001](tasks/presentation-031-001.md) — bounded latest-frame
   presentation contract.
5. [workspace-031-001](tasks/workspace-031-001.md) — addressable workspace and
   mutation-lease contract.

These tasks are intentionally disjoint and do not edit shared exports or
dispatch files. Integration tasks begin only after committed implementation
and independent review.

## Historical plan: Glass v0.3.2 issue #32

Status: Historical/superseded — this 0.3.2 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue #32 is an architectural epic. The original thin-slice interpretation was
rejected during the issue/comment audit; the current candidate is reviewed
against every pillar, mandatory release gate, visual comment, and packaging
comment in the [delivery evidence matrix](analysis/release-032.md).

Delivery order:

1. [development-032-001](tasks/development-032-001.md) — project detection,
   bounded files/editor, PTY process runtime, events, graph, and diff core.
2. [development-032-002](tasks/development-032-002.md) — shared CLI and MCP
   project-runtime contracts for humans and external agents.
3. [development-032-003](tasks/development-032-003.md) — native TUI project
   surface and embedded harness interaction.
4. [release-032-001](tasks/release-032-001.md) — package boundary, versioned
   documentation, release validation, and local 0.3.2 candidate checkpoint.

The candidate must keep the v0.3.1 browser intelligence contracts intact. Any
capability that cannot provide evidence in this checkout is reported as
experimental or unavailable; it is not presented as a completed framework
integration.

Integration wave (after foundation review):

- [surface-031-002](tasks/surface-031-002.md) — integrate surfaces into Web
  IR extraction.
- [backend-031-002](tasks/backend-031-002.md) — route the CDP implementation
  through the Browser Capability Interface.
- [presentation-031-002](tasks/presentation-031-002.md) — add terminal graphics,
  bounded frame presentation, and semantic fallback.
- [workspace-031-002](tasks/workspace-031-002.md) — connect workspace identity
  to profiles, sessions, memory, and attachments.
- [memory-031-002](tasks/memory-031-002.md) — connect retrieval and provenance
  to compiler advisories.
- [experience-031-001](tasks/experience-031-001.md) — expose the shared
  Experience Layer across CLI, TUI, and MCP.
- [integration-031-001](tasks/integration-031-001.md) — run the integrated
  four-pillar conformance demonstration.

## Historical plan: remote development cockpit

Status: Historical — implemented and verified by direct serial work on the
local 0.3.2 candidate.

The post-issue-32 product enhancements are defined in the
[delivery analysis](analysis/mobile-cockpit.md) and implemented as the atomic
[mobile-cockpit-001](tasks/mobile-cockpit-001.md) task.

## Proposed plan: best-in-class agent browser

Status: Proposed draft — requires owner approval before implementation; it is
not a commitment for the current `0.3.14` source line.

The proposed goal is to make Glass a deterministic, memory-efficient browser
control layer that humans and agents prefer over mature alternatives for local
Chrome automation.

Analysis and scorecard:

- [Best-in-class browser analysis](analysis/best-in-class-browser.md)

### Task order

| Order | Task | Outcome |
|---:|---|---|
| 1 | [quality-007](tasks/quality-007.md) | Task-success and resource scorecard before feature work. |
| 2 | [mcp-008](tasks/mcp-008.md) | Bounded, negotiated, cancellable MCP transport. |
| 3 | [target-009](tasks/target-009.md) | Unique locator resolution and verified hit targets. |
| 4 | [wait-010](tasks/wait-010.md) | Typed explicit wait engine. |
| 5 | [topology-011](tasks/topology-011.md) | Tabs, popups, targets, and frames. |
| 6 | [input-012](tasks/input-012.md) | Complete keyboard, pointer, form, and upload primitives. |
| 7 | [observe-013](tasks/observe-013.md) | Consistent, frame-aware, bounded observations. |
| 8 | [diagnostic-014](tasks/diagnostic-014.md) | Scoped console, network, dialog, and download evidence. |
| 9 | [visual-015](tasks/visual-015.md) | Exact viewport/full-page capture and visual verification. |
| 10 | [policy-016](tasks/policy-016.md) | Enforceable safety profiles and side-effect controls. |
| 11 | [release-017](tasks/release-017.md) | Supply-chain, fuzz, crash, and multi-platform hardening. |
| 12 | [compare-018](tasks/compare-018.md) | Final comparative task-success and efficiency gate. |
| 13 | [observe-019](tasks/observe-019.md) | Event-driven accessibility rejected against pinned Chromium semantics. |

Tasks are developed and independently reviewed in dependency order. A phase
does not advance while correctness or safety gates from an earlier task fail.

### Completed plan: Glass 0.2.2 issue #29

Status: Complete — published on crates.io; follow-up work continues in the 0.2.3 development line

The complete issue outline, dependency map, integration points, and atomic
commit boundaries are recorded in
[analysis/release-029.md](analysis/release-029.md). Phase tasks:

1. [release-029-001](tasks/release-029-001.md) — contract foundations
2. [release-029-002](tasks/release-029-002.md) — installation diagnostics
3. [release-029-003](tasks/release-029-003.md) — bounded agent operations
4. [release-029-004](tasks/release-029-004.md) — state and templates
5. [release-029-005](tasks/release-029-005.md) — maintainability and distribution
6. [release-029-006](tasks/release-029-006.md) — release verification

### Completed plan: Glass Semantic Execution Engine issue #30

Status: Complete — `glass-browser 0.3.0` is published on crates.io and
`v0.3.0` has the matching source-only GitHub Release. The epic exit contract
and release delivery record are complete.

Completed:

- [ir-030-001](tasks/ir-030-001.md) — versioned fixture corpus and static
  baseline inventory.
- [ir-030-002](tasks/ir-030-002.md) — bounded extraction contracts, scopes, and
  resource budgets.
- [ir-030-003](tasks/ir-030-003.md) — evidence quality and coverage metadata.
- [ir-030-004](tasks/ir-030-004.md) — deterministic draft Web IR
  reconciliation.
- [ir-030-005](tasks/ir-030-005.md) — explicit opaque boundary graph entities.
- [ir-030-006](tasks/ir-030-006.md) — bounded region relationship evidence.
- [ir-030-007](tasks/ir-030-007.md) — fixture-derived draft graph expectations.
- [ir-030-008](tasks/ir-030-008.md) — evidence-backed form ownership edges.
- [ir-030-009](tasks/ir-030-009.md) — explicit relationship hints.
- [ir-030-010](tasks/ir-030-010.md) — source-level relationship hint
  validation.
- [ir-030-011](tasks/ir-030-011.md) — validated relationship-hint
  diagnostics.
- [ir-030-012](tasks/ir-030-012.md) — unmatched relationship-hint statuses.
- [ir-030-013](tasks/ir-030-013.md) — emitted and unmatched diagnostic
  expectations.
- [ir-030-014](tasks/ir-030-014.md) — corpus hint-diagnostic expectations.
- [ir-030-015](tasks/ir-030-015.md) — runtime custom-control hints.
- [ir-030-016](tasks/ir-030-016.md) — expanded custom-control hints.
- [ir-030-017](tasks/ir-030-017.md) — deterministic Web IR revision diffs.
- [ir-030-018](tasks/ir-030-018.md) — revision identity continuity.
- [ir-030-019](tasks/ir-030-019.md) — strict Task Protocol v1 contract.
- [ir-030-020](tasks/ir-030-020.md) — deterministic Task Protocol execution plans.
- [ir-030-021](tasks/ir-030-021.md) — typed task.compile protocol boundary.
- [ir-030-022](tasks/ir-030-022.md) — typed compiled-plan response.
- [ir-030-023](tasks/ir-030-023.md) — browser-free MCP compileTask integration.
- [ir-030-024](tasks/ir-030-024.md) — MCP compileTask client documentation.
- [ir-030-025](tasks/ir-030-025.md) — browser-free CLI task compile.
- [ir-030-026](tasks/ir-030-026.md) — typed MCP compileTask errors.
- [ir-030-027](tasks/ir-030-027.md) — compiler explanation mode.
- [ir-030-028](tasks/ir-030-028.md) — compiled-plan guard metadata.
- [ir-030-029](tasks/ir-030-029.md) — browser-free task validation.
- [ir-030-030](tasks/ir-030-030.md) — browser-free MCP task validation.
- [ir-030-031](tasks/ir-030-031.md) — Rust crate-root extraction and Web IR APIs.
- [ir-030-032](tasks/ir-030-032.md) — browser-free Web IR inspect and diff CLI.
- [ir-030-033](tasks/ir-030-033.md) — offline Web IR entity continuity classification.
- [ir-030-034](tasks/ir-030-034.md) — deterministic Web IR canonical JSON CLI output.
- [ir-030-035](tasks/ir-030-035.md) — offline Web IR validation command.
- [ir-030-036](tasks/ir-030-036.md) — browser-free MCP Web IR inspection and
  validation.
- [ir-030-037](tasks/ir-030-037.md) — browser-free MCP Web IR diff and
  continuity classification.
- [ir-030-038](tasks/ir-030-038.md) — canonical Glass protocol operations for
  Web IR revision analysis.
- [ir-030-039](tasks/ir-030-039.md) — canonical Glass protocol operations for
  Web IR inspection and validation.
- [ir-030-040](tasks/ir-030-040.md) — canonical Glass protocol operations for
  Task Protocol validation and compilation.
- [ir-030-041](tasks/ir-030-041.md) — route MCP Task Protocol tools through
  typed canonical dispatch.
- [ir-030-042](tasks/ir-030-042.md) — route MCP Web IR tools through typed
  canonical dispatch.
- [ir-030-043](tasks/ir-030-043.md) — typed canonical protocol response fixture
  coverage.
- [ir-030-044](tasks/ir-030-044.md) — typed canonical preflight error fixture
  coverage.
- [ir-030-045](tasks/ir-030-045.md) — advertise canonical Task and Web IR
  schema versions through capability negotiation.
- [ir-030-046](tasks/ir-030-046.md) — advertise Task Protocol and Web IR
  capability statuses.
- [ir-030-047](tasks/ir-030-047.md) — typed MCP Task validation and compilation
  errors.
- [ir-030-048](tasks/ir-030-048.md) — typed canonical Task compilation
  preflight error coverage.
- [ir-030-049](tasks/ir-030-049.md) — route CLI Task commands through canonical
  protocol helpers.
- [ir-030-050](tasks/ir-030-050.md) — route safe CLI Web IR projections through
  canonical protocol helpers.
- [ir-030-051](tasks/ir-030-051.md) — typed Web IR diff and continuity
  preflight error fixture coverage.
- [ir-030-052](tasks/ir-030-052.md) — expose the bounded canonical Web IR
  diff projection through an explicit offline CLI mode.
- [ir-030-053](tasks/ir-030-053.md) — harden deterministic Task Protocol
  execution-plan safety checks.
- [ir-030-054](tasks/ir-030-054.md) — enforce compatible Web IR revision
  transitions for diffs and continuity.
- [ir-030-055](tasks/ir-030-055.md) — verified form task execution boundary
  (implemented and covered in `0.2.7`).
- [ir-030-056](tasks/ir-030-056.md) — expose verified form task execution
  through CLI and MCP.
- [ir-030-057](tasks/ir-030-057.md) — execute bounded semantic region
  extraction through the guarded Task Protocol runtime.
- [ir-030-058](tasks/ir-030-058.md) — standardize typed task retry guidance
  across guarded execution outcomes.
- [ir-030-059](tasks/ir-030-059.md) — execute revision-guarded navigation
  tasks through CLI, MCP, and Rust.
- [ir-030-060](tasks/ir-030-060.md) — execute revision-guarded semantic tab
  selection within scoped regions.
- [ir-030-061](tasks/ir-030-061.md) — execute guarded inspect, confirm, and
  cancel dialog tasks through CLI, MCP, and Rust.
- [ir-030-062](tasks/ir-030-062.md) — execute bounded revision-guarded
  pagination advances within semantic pagination regions.
- [ir-030-063](tasks/ir-030-063.md) — expose typed pending-dialog details
  through `dialog.inspect` task results.
- [ir-030-064](tasks/ir-030-064.md) — route CLI and MCP browser-backed task
  execution through one canonical Rust dispatcher.
- [ir-030-065](tasks/ir-030-065.md) — execute bounded `collection.extract`
  against uniquely scoped semantic collection regions.
- [ir-030-066](tasks/ir-030-066.md) — execute bounded `table.extract` against
  uniquely scoped semantic table regions.
- [ir-030-067](tasks/ir-030-067.md) — execute guarded `field.read` with bounded
  form-state output and policy-preserving redaction.
- [ir-030-068](tasks/ir-030-068.md) — harden `field.read` with sensitive-value
  redaction coverage and post-observation revision checks.
- [ir-030-069](tasks/ir-030-069.md) — require `inputs.field` during authored
  `field.read` validation.
- [ir-030-070](tasks/ir-030-070.md) — require explicit semantic region scopes
  for browser-backed task families.
- [ir-030-071](tasks/ir-030-071.md) — execute bounded `pagination.collect` with
  revision-aware page advances and recovery guidance.
- [ir-030-072](tasks/ir-030-072.md) — harden extraction revision checks and
  semantic no-op detection for pagination collection.
- [ir-030-073](tasks/ir-030-073.md) — add guarded `navigation.openMenu`
  execution with semantic menu-control targets.
- [ir-030-074](tasks/ir-030-074.md) — verify `navigation.openMenu` outcomes
  through observable expanded state and typed indeterminate recovery.
- [ir-030-075](tasks/ir-030-075.md) — verify `navigation.selectTab` through
  bounded ARIA-selected polling and indeterminate recovery.
- [ir-030-076](tasks/ir-030-076.md) — require a bounded semantic page or route
  transition after `pagination.next`, with delayed-success and no-op recovery
  coverage.
- [ir-030-077](tasks/ir-030-077.md) — verify `navigation.follow` reaches the
  requested destination and return indeterminate recovery for redirects or
  other URL mismatches.
- [ir-030-078](tasks/ir-030-078.md) — restrict `form.submit` to
  evidence-backed semantic button targets and fail closed for named fields or
  other non-submit controls.
- [ir-030-079](tasks/ir-030-079.md) — convert `form.fill` operation and
  post-fill inspection failures into bounded indeterminate recovery results.
- [ir-030-080](tasks/ir-030-080.md) — bound mutation verification failures
  and require explicit `form.submit` postconditions.
- [ir-030-081](tasks/ir-030-081.md) — add typed structured-extraction kinds,
  field-level provenance, and explicit output-limit metadata.
- [ir-030-082](tasks/ir-030-082.md) — add bounded item-level records for
  semantic table and repeated-collection extraction.

- [ir-030-083](tasks/ir-030-083.md) — populate bounded semantic table and
  collection records from accessibility evidence.
- [ir-030-084](tasks/ir-030-084.md) — include structured record changes in
  revision-aware semantic page checks.
- [ir-030-085](tasks/ir-030-085.md) — add bounded, revision-bound
  continuation metadata for truncated extraction.
- [ir-030-086](tasks/ir-030-086.md) — validate continuation revision and route
  before resuming extraction.
- [ir-030-087](tasks/ir-030-087.md) — bind continuations to the requested
  semantic region.
- [ir-030-088](tasks/ir-030-088.md) — bind continuations to the extraction
  field contract.
- [ir-030-089](tasks/ir-030-089.md) — add fail-closed sensitive extraction
  gating for secret-like field names and paths.

## Completed plan: performance overhaul
Status: Complete

The previous plan established compact observation, explicit expensive paths,
browser ownership, stable references, persistent MCP, a responsive TUI, and
baseline performance measurements. Its completed tasks remain below as the
delivery record:

1. [baseline-000](tasks/baseline-000.md)
2. [perf-001](tasks/perf-001.md)
3. [lifecycle-002](tasks/lifecycle-002.md)
4. [action-003](tasks/action-003.md)
5. [mcp-004](tasks/mcp-004.md)
6. [tui-005](tasks/tui-005.md)
7. [verify-006](tasks/verify-006.md)
