# Native browser engine

Status: Experimental Phase 2 semantic DOM/interaction slices, initial Phase 3
presentation/layout/display-list/software-surface/PNG-capture/box-model/
viewport-scroll/side-specific-border/bounded-pattern-border/bounded-corner-radius/
bounded-inline-flow/bounded-fixed-line-height/bounded-direct-text-flow/
bounded-word-wrap/bounded-physical-box-edges/bounded-whitespace-boundaries/
bounded-overflow-hit-test-projection/bounded-css-diagnostics/bounded-pixel-golden-capture/
bounded-descendant-selectors/bounded-overflow-clip/bounded-hard-line-breaks/
bounded-pre-line-breaks/bounded-preformatted-whitespace/
bounded-pre-wrap-whitespace/bounded-nowrap-whitespace/bounded-inherited-line-height/
bounded-clip-aware-root-overflow/bounded-axis-specific-overflow/
bounded-min-max-dimensions/bounded-opacity-groups/bounded-text-alignment/
bounded-functional-alpha-colors/bounded-fixed-cell-text-decoration/
bounded-inherited-text-transform/bounded-first-line-text-indent/
bounded-inherited-word-spacing/bounded-inherited-letter-spacing/
bounded-inherited-font-weight/bounded-inherited-font-style/
bounded-inherited-word-break slices,
bounded-text-overflow/bounded-vertical-align/bounded-flex-row/
bounded-flex-row-gap/bounded-flex-row-justification/bounded-flex-item-order/
bounded-flex-cross-axis-alignment/bounded-flex-direction/bounded-flex-column-direction/
bounded-flex-wrap/
bounded-flex-wrap-reverse,
bounded-flex-column-wrap/bounded-flex-column-wrap-reverse,
bounded-flex-cross-line-alignment/bounded-flex-cross-line-space-around/
bounded-flex-cross-line-space-evenly/bounded-flex-cross-line-stretch/
bounded-flex-cross-line-normal/bounded-flex-cross-line-gap,
bounded-flex-gap-family,
bounded-base64-data-url/bounded-fragment-navigation-history/
bounded-local-link-activation/bounded-fragment-target-scroll/
bounded-relative-local-links/bounded-percent-decoded-fragment-targets/
bounded-legacy-name-fragment-targets/bounded-text-fragment-targets/
bounded-text-fragment-affixes/bounded-root-horizontal-scroll,
including bounded style
inheritance, paint clipping, solid/dashed/dotted border painting, rounded
fill/border masks, inline-box line placement, fixed pixel line-height floors,
and content-box geometry,
bounded base64 data-URL loading, plus feature-gated
runtime/CLI integration; not a stable browser compatibility or security
boundary.

This document is the repository contract for the Glass-owned native browser
engine described by [issue #40](https://github.com/wanazhar/glass/issues/40).
The engine is an experimental backend inside `glass-browser`; it is not a
third crate, a protocol adapter, or an embedded copy of another browser.

## Purpose and boundary

The native engine owns a deterministic, headless browser-platform kernel. The
current slices own lifecycle, one browsing context, local document resources,
HTML-to-DOM parsing, history, revisions, bounded semantic evidence, and a small
revisioned semantic interaction/effects model for text, checkbox, radio, and
single-select controls, plus bounded visibility/actionability and raw-text/RCDATA
parser gates, a narrow CSS presentation subset, and deterministic integer-pixel
normal-flow geometry with point hit testing, a Rust-only clear/fill/text/border
display list, a bounded logical RGBA software surface and PNG capture,
inherited text color through DOM parent links, bounded `overflow:hidden`/
`overflow:clip` paint clipping, bounded side-specific solid/dashed/dotted-border paint primitives,
bounded circular border radii, bounded outer/content box geometry with bounded
min/max width/height constraints, explicit root
horizontal and vertical viewport scrolling, bounded inline-box line placement, and bounded fixed pixel
line-height flow with bounded inheritance, bounded direct-text fragments at
actual flow origins,
source-order text paint, bounded word-aware wrapping, bounded physical
four-side padding/margin edges, bounded source-whitespace boundaries, and
bounded hard line breaks in supported inline flow, and
bounded inherited `white-space: pre-line`, `white-space: pre`,
`white-space: pre-wrap`, and `white-space: nowrap` source whitespace flow, and
bounded inherited physical `text-align:left|center|right` line placement, and
bounded functional `rgba(R, G, B, A)` alpha colors for background, border, and
text paint, bounded fixed-cell `text-decoration:none|underline` paint, bounded
inherited ASCII `text-transform:none|uppercase|lowercase` layout, and
bounded inherited non-negative fixed-pixel `word-spacing` across the supported
fixed-cell whitespace modes, and
bounded inherited non-negative fixed-pixel `letter-spacing` after rendered
fixed-cell characters in each emitted fragment, bounded inherited
`font-weight:normal|bold|400|700` fixed-cell raster presentation, bounded
inherited `font-style:normal|italic` fixed-cell raster presentation, and
bounded inherited `word-break:normal|break-all` fixed-cell wrapping, and
bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
fixed-cell inline and inline-block line items, and
bounded block-level `display:flex` single-row placement for eligible direct
element children, and
bounded one-value non-negative fixed-pixel `gap` spacing between visible flex
items, and bounded `justify-content:normal|flex-start|center|flex-end|space-between|
space-around|space-evenly|stretch` free-space placement for eligible fixed-width flex
rows,
bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
source-order ties,
bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
placement using explicit content height or the auto row's maximum item outer
height,
bounded non-inherited `flex-direction:column|column-reverse` placement for
eligible fixed-height, no-wrap flex containers with vertical justification,
row-gap, flexible lengths, horizontal item alignment, reverse placement, and
shared descendant/artifact consumers,
bounded `flex-wrap:wrap` for fixed-height `column|column-reverse` containers
with vertical line formation, horizontal `column-gap`, per-line flex sizing
and justification, bounded `align-content`, and shared descendant/artifact
consumers,
bounded rectangular
`overflow:hidden`/`overflow:clip` clips shared by paint,
viewport projection, and point hit testing, and bounded read-only diagnostics
for unsupported CSS input, plus bounded local same-document fragment
navigation and Rust history traversal, and bounded local opacity groups that
composite reduced-opacity rendered subtrees through transparent software layers.
The 049 boundary adds independent bounded `overflow-x:hidden`/`clip` and
`overflow-y:hidden`/`clip` clips through that same rectangular owner; it does
not add nested scrolling, scrollbars, or visible-overflow propagation.
The 050 boundary adds bounded physical `min-width`/`max-width` and
`min-height`/`max-height` constraints through the same box-model owner;
intrinsic and percentage sizing remain outside the boundary. The later 064
boundary adds only bounded fixed-width single-row `display:flex` placement;
the 065 boundary adds a bounded fixed-pixel `gap`; and the 066 boundary
adds bounded fixed-width row free-space placement. General flex/grid sizing
remains outside the claim. The 068 boundary adds bounded non-inherited
cross-axis alignment for eligible rows without changing their horizontal or
semantic/source order. The 069 boundary adds bounded non-inherited
`flex-direction:row|row-reverse` to those rows: reverse placement walks the
order-sorted sequence from the physical right edge, keeps physical margins
attached, maps existing gap and justification to physical edges, shifts
overflow as one bounded row, and reuses shared subtree artifacts, root
scrolling, hit testing, and paint while leaving semantic/source order
unchanged.
The completed 070 boundary adds bounded non-inherited `flex-wrap:nowrap|wrap`
to those rows. `wrap` forms integer physical lines from measured outer item
widths and the existing gap, reuses the 069 per-line direction and the
existing justification/alignment/artifact consumers, and stacks maximum-height
lines through root overflow while leaving cross-line distribution, row/column
gaps, flex sizing, and `wrap-reverse` outside the boundary. The design is
`8772a6a`, the implementation is `5c16185`, and local validation evidence is
recorded in `docs/plan/tasks/native-engine-070.md`; remote CI remains pending
because the branch is local-only.
The completed 071 boundary adds bounded non-inherited
`align-content:flex-start|center|flex-end|space-between` to wrapped rows. It
distributes only positive explicit-content-height remainder before or between
the existing physical lines, translates complete line artifact ranges after
per-line `align-items`, and leaves `nowrap` and auto-height geometry
unchanged. The contract is recorded in
`docs/plan/tasks/native-engine-071.md`; design is `71bd380`, implementation is
`cc1d602`, the final single-line coverage test is `050d41b`, and local
validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.
The completed 072 boundary adds bounded non-inherited
`align-content:space-around` to wrapped rows. It uses saturating integer
slot-center offsets from positive explicit-content-height remainder while
preserving the 071 line-record, artifact, overflow, and semantic owners. The
contract is recorded in `docs/plan/tasks/native-engine-072.md`; design is
`c8e5170`, implementation is `a1c8b56`, and local validation and cleanup
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.
The completed 073 boundary adds bounded non-inherited
`align-content:space-evenly` to wrapped rows. It uses equal leading,
inter-line, and trailing integer slots from positive explicit-content-height
remainder while preserving the 072 line-record, artifact, overflow, and
semantic owners. The contract is recorded in
`docs/plan/tasks/native-engine-073.md`; design is `23b62a3`, implementation is
`b4833a9`, and local validation and cleanup evidence are recorded in the task
file. The default-stack overflow in one existing large-Clap parser test is
documented there; the full native library suite passes with an explicit 8 MiB
test-thread stack. Remote CI remains pending because the branch is local-only.
The completed 074 boundary adds bounded non-inherited
`flex-wrap:wrap-reverse` to eligible fixed-width rows. It keeps source-order
line formation, reflects physical line origins from the cross-axis end, reuses
all bounded `align-content` offsets, and translates each complete line artifact
range with a signed document-pixel delta. The contract is recorded in
`docs/plan/tasks/native-engine-074.md`; design is `5f6c61a`, implementation is
`96fd15c`, and local validation and cleanup evidence are recorded in the task
file. The previously documented default-stack issue in one large-Clap parser
test remains a harness follow-up; the full native library suite passes with an
explicit 8 MiB test-thread stack. Remote CI remains pending because the branch
is local-only.
The completed 075 boundary adds explicit bounded non-inherited
`align-content:stretch` to wrapped fixed-width rows. Positive explicit
cross-axis remainder expands formed line boxes by deterministic integer shares,
then the existing `align-items` and complete-artifact passes place normal and
wrap-reverse rows through one coordinate owner. The contract is recorded in
`docs/plan/tasks/native-engine-075.md`; design is `4637863`, implementation is
`e26c0a4` with the diagnostics-fixture correction in `cc8b538`, and local
validation and cleanup evidence are recorded in that task file. Remote CI
remains pending because the branch is local-only.
The completed 076 boundary adds the explicit bounded non-inherited
`align-content:normal` keyword to the same wrapped fixed-width rows. It routes
positive explicit cross-axis remainder through the completed stretch line-box
owner for normal and wrap-reverse stacking while preserving the established
omitted-value `flex-start` fallback. Its contract is recorded in
`docs/plan/tasks/native-engine-076.md`; design is `b6c647e`, implementation is
`e3933b3`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 077 boundary adds explicit bounded non-inherited `row-gap`
spacing between adjacent formed lines in eligible wrapped fixed-width flex
rows. It accounts for that cross-line space once before the existing
`align-content` free-space owner and preserves the current main-axis-only `gap`
behavior. The contract is recorded in
`docs/plan/tasks/native-engine-077.md`; design is `99d70ef`, implementation is
`4b27b15`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 078 boundary completes the bounded flex gap family with one- and
two-value integer-pixel `gap`, explicit `row-gap`/`column-gap` longhands, and
source-order-aware shorthand/longhand cascade. One-value `gap` intentionally
supplies both axes, superseding the 077 compatibility behavior where it only
supplied the main axis. Its contract is recorded in
`docs/plan/tasks/native-engine-078.md`; design is `c6ebecd`, implementation is
`1bca33f`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 079 boundary adds bounded non-inherited integer `flex-grow`
weights to eligible row-flex items. Positive line free space is allocated
before justification with deterministic prefix-floor shares, while max-width
caps freeze and redistribute remainder through the same geometry owner. Its
contract is recorded in `docs/plan/tasks/native-engine-079.md`; design is
`a45fb01`, implementation is `1a930a3`, and local validation and cleanup
evidence are recorded in that task file. Remote CI remains pending because the
branch is local-only.
The completed 080 boundary adds bounded non-inherited integer `flex-shrink`
weights to eligible row-flex items. Negative line free space is allocated using
original-base-width weighted prefix-floor shares, while effective outer
`min-width` floors freeze and redistribute the remaining deficit through the
same geometry owner. Its contract is recorded in
`docs/plan/tasks/native-engine-080.md`; design is `46227de5`, implementation is
`b1414931`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 081 boundary defines bounded `flex-basis:auto|Npx` for eligible
row-flex items. Explicit bases override item `width`, use the existing
box-sizing and min/max conversion helpers, remain available to the completed
grow/shrink owner before line formation, and preserve one coordinate/state
owner across descendants and all visual, overflow, interaction, and semantic
consumers. The contract is recorded in
`docs/plan/tasks/native-engine-081.md`; design is `04cc4a3e`, implementation is
`299c93f9`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 082 boundary defines bounded `flex` shorthand expansion into the
existing non-inherited `flex-grow`, `flex-shrink`, and `flex-basis` computed
components. Common `none`, `auto`, bounded integer-factor, and bounded
pixel/`auto` basis forms share the existing declaration precedence, while
unsupported CSS-wide, percentage, fractional, and ambiguous forms remain
typed diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-082.md`; design is `71d1060d`, implementation is
`40f6fb1c`, and local validation and cleanup evidence are recorded there.
Remote CI remains pending because the branch is local-only.
The completed 083 boundary defines bounded `flex-flow` shorthand expansion into
the existing non-inherited `flex-direction` and `flex-wrap` computed
components. One-token forms reset the omitted component to its initial row or
nowrap value; two-token forms accept one bounded direction and one bounded wrap
keyword in either order. Unsupported columns, duplicates, CSS-wide, logical,
and ambiguous forms remain typed diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-083.md`; design is `80c6836e`, implementation is
`0291bf90`, and the strict-Clippy fix is `16e6d9aa`. Local validation and exact
target-cleanup evidence are recorded there. Remote CI remains pending because
the branch is local-only.
The completed 084 boundary adds bounded non-inherited `align-self:auto|flex-start|
center|flex-end` for eligible direct flex items. `auto` resolves to the parent
`align-items` value at the existing cross-axis placement decision; explicit
values override only that item and translate its complete subtree artifacts.
Stretch, baseline, logical, CSS-wide, and ambiguous forms remain typed
diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-084.md`; design is `36085e9c`, implementation is
`f2f99f66`, and focused/full/strict/documentation/cleanup evidence is recorded
there. Remote CI remains pending because the branch is local-only.
The completed 085 boundary expands bounded `place-content` shorthand into the
existing `align-content` and `justify-content` computed components. One shared
token is limited to values valid in both bounded axes; two tokens use explicit
cross-axis/main-axis order. Unsupported CSS-wide, logical, safe/unsafe,
ambiguous, and unsupported justify forms remain typed diagnostics. The
contract is recorded in `docs/plan/tasks/native-engine-085.md`; design is
`adc61a1f`, implementation is `02f866e6`, and focused/full/strict/documentation/
cleanup evidence is recorded there. Remote CI remains pending because the
branch is local-only.
The completed 086 boundary extends bounded non-inherited `align-self` with
`stretch` for eligible direct flex items. Auto-height items fill the existing
line cross size through the current box-model and complete-artifact owners;
explicit heights remain unchanged and use the bounded flex-start fallback.
The contract is recorded in `docs/plan/tasks/native-engine-086.md`; design is
`f92b7b9a`, implementation is `5ea2c8d1`, and strict layout lint cleanup is
`2c07c749`. Focused, full, strict, documentation, release-certification, and
exact target-cleanup evidence are recorded in the task file. Remote CI remains
pending because the branch is local-only.
The completed 087 boundary extends the parent `align-items` grammar with
explicit `stretch`. Children whose `align-self` remains `auto` reuse the 086
used-size and complete-artifact owner; explicit child overrides remain
authoritative and explicit heights stay fixed under the bounded flex-start
fallback. The omitted native fallback remains `flex-start`. The contract is
recorded in `docs/plan/tasks/native-engine-087.md`; design is `4708f663` and
implementation is `e0d051ce`. Focused/full/strict/release-certificate,
documentation, and exact-target evidence are recorded in the task file. The
first all-in-one certification run exposed one environment-sensitive Rust
Analyzer probe; its exact retry passed. Remote CI remains pending because the
branch is local-only.
The completed 088 boundary adds explicit parent `align-items:normal`. In the
supported row/row-reverse flex context, `normal` resolves `align-self:auto`
through the completed stretch used-size owner while retaining a distinct
computed keyword; the omitted native fallback remains `flex-start`. The
contract and evidence are recorded in `docs/plan/tasks/native-engine-088.md`;
design is `162543f1` and implementation is `19d8d3f6`. Remote CI remains
pending because the branch is local-only.
The completed 089 boundary adds explicit child `align-self:normal`. In the
supported row/row-reverse flex context, the explicit item value reuses the
completed stretch used-size owner regardless of parent `align-items`, while
`align-self:auto` remains parent-controlled and the computed keyword remains
distinct. The contract and evidence are recorded in
`docs/plan/tasks/native-engine-089.md`; design is `95caf4a9` and
implementation is `1ee55c43`. Remote CI remains pending because the branch is
local-only.
The completed 090 boundary adds explicit `justify-content:space-around` to the
bounded fixed-width row/row-reverse flex-line owner. Positive main-axis free
space is distributed with deterministic integer cumulative offsets around the
existing item, gap, margin, and flex-sizing geometry; row-reverse mirrors the
same offsets and downstream artifacts remain shared. The complete contract and
evidence are recorded in `docs/plan/tasks/native-engine-090.md`; design is
`97c69d9d` and implementation is `d814784f`. Remote CI remains pending because
the branch is local-only.
The completed 091 boundary adds explicit `justify-content:space-evenly` to that
same bounded fixed-width row/row-reverse owner. Positive main-axis free space
is distributed into equal deterministic integer slots after existing item,
gap, margin, and flex-sizing geometry, with row-reverse mirroring and the same
shared downstream artifact consumers. Its contract and evidence are recorded
in `docs/plan/tasks/native-engine-091.md`; design is `be1a8e20` and
implementation is `118590f7`. Remote CI remains pending because the branch is
local-only.
The completed 092 boundary adds explicit `justify-content:normal` to that
owner. The computed keyword remains distinct while used placement reuses the
completed `flex-start` path, including explicit gaps, margins, row-reverse,
wrapping, and shared downstream artifacts. Its contract and evidence are
recorded in `docs/plan/tasks/native-engine-092.md`; design is `4cbaf338` and
implementation is `ce19db39`. Remote CI remains pending because the branch is
local-only.
The completed 093 boundary adds explicit `justify-content:stretch` to that
owner. The computed keyword remains distinct while used placement reuses the
completed `flex-start` path, including explicit gaps, margins, row-reverse,
wrapping, and shared downstream artifacts; the shared one-token and two-token
`place-content:stretch` forms are also covered. Its contract and evidence are
recorded in `docs/plan/tasks/native-engine-093.md`; design is `273b31db` and
implementation is `653f025e`. Remote CI remains pending because the branch is
local-only.
The completed 094 boundary extends that same owner to explicit
`flex-direction:column|column-reverse` for eligible fixed-height, no-wrap
containers. Vertical main-axis justification uses `row-gap`; bounded
grow/shrink/basis, horizontal item alignment, reverse placement, and complete
descendant/artifact consumers are mapped through the existing state owner.
Auto-height columns, wrapping, column-gap line distribution, logical writing
modes, and browser-wide Flexbox remain outside the contract recorded in
`docs/plan/tasks/native-engine-094.md`; design is `aea47b17`, implementation
checkpoints are `7dc92517` and `4e212151`, and local certification/cleanup
evidence is recorded there. Remote CI remains pending because the branch is
local-only.
The completed 095 boundary extends the completed column owner to
`flex-wrap:wrap` for fixed-height columns. It forms vertical main-axis lines,
uses `column-gap` between horizontal line boxes, reuses per-line bounded
flex-sizing/justification and existing `align-content`, and keeps complete
descendant/artifact consumers on the shared geometry path. `wrap-reverse`,
auto-height columns, and browser-wide Flexbox remain outside the contract
recorded in `docs/plan/tasks/native-engine-095.md`. Design is `598228a7`,
implementation is `96ea62a3`, and local certification evidence is recorded in
the task file. Remote CI remains pending because the branch is local-only.
The active 096 design extends that same fixed-height column owner to
`flex-wrap:wrap-reverse`. It reflects horizontal line boxes and each line's
cross-axis `align-items`/`align-self` placement from the physical cross-end,
reuses 095 line formation, `column-gap`, `align-content`, main-axis direction,
and shared descendant/artifact consumers, and preserves source/semantic order.
The contract is recorded in `docs/plan/tasks/native-engine-096.md`; the design
checkpoint is `f5026f3c` and implementation is in progress.
Auto-height columns, intrinsic or percentage sizing, logical writing modes, and
browser-wide Flexbox remain outside the contract. Remote CI remains pending
because the branch is local-only.
The 051 boundary adds local `opacity` values quantized to bounded 8-bit alpha
and brackets reduced-opacity rendered subtrees with immutable display-list
markers. The rasterizer composites each group inside-out through bounded
transparent layers, while layout, semantic visibility, hit testing, and root
scrolling remain unchanged; general stacking contexts and compositor parity
remain outside the boundary.
Semantic local anchor activation reaches that same bounded navigation owner for
fragment-only, fixture-relative, and absolute local hrefs; unsupported href
forms fail closed.
Visible local fragment targets may position the root viewport at the exact
case-sensitive `id` obtained after bounded percent-decoding; the active history
entry retains the bounded scroll offset for restoration.
The engine does not
yet own
general CSS, nested/smooth/keyboard scrolling, scrollbars, nested overflow
scrolling, or scrolling/stacking layout,
screenshot semantics, font/image
fidelity, hit-test visuals, JavaScript,
network access, storage, downloads, prompts, or platform windows. Unsupported
CSS is not silently indistinguishable from supported CSS: the native Rust API
exposes bounded revisioned diagnostics for ignored selectors, properties,
values, and malformed rules without echoing raw stylesheet content.

```text
BrowserBackendDispatcher
            |
            v
NativeEngineBackend       <- semantic backend profile and lifecycle adapter
            |
            v
NativeEngine              <- the only mutable page-state owner
    +-------+--------+----------------+----------+-----------+
    |       |        |                |          |           |          |
  DOM   history  scheduler      resource loader layout    display list  raster
    |       |        |                |          |           |          |
    +--- semantic projection ---------+---- hit test --- paint commands -> RGBA
            |
            v
   bounded URL/title/text evidence
```

Chromium/CDP remains the production full-session path. Firefox BiDi and Safari
WebDriver remain externally managed experimental adapters. The native engine
is selected only by an explicit backend preference; it is never an automatic
fallback or an implicit replacement for Chromium.

## Cargo and public entry points

The implementation is behind the default-off `native-engine` feature:

```console
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
```

The default feature set does not compile or enable the native engine. The
feature adds no dependency. The first public construction path is:

```rust,no_run
use glass_browser::browser::native_engine::NativeEngineConfig;
use glass_browser::{BackendFactory, NativeEngineBackend};

# fn run() -> Result<(), Box<dyn std::error::Error>> {
let config = NativeEngineConfig::default();
let backend = BackendFactory::native(config)?;
assert_eq!(backend.profile().identity.backend_id, "native-engine");
# Ok(())
# }
```

The explicit Rust engine owner also exposes the CSS audit signal:

```rust,no_run
use glass_browser::browser::native_engine::{NativeEngine, NativeEngineConfig};

# fn run() -> Result<(), Box<dyn std::error::Error>> {
let mut engine = NativeEngine::new(NativeEngineConfig::default())?;
engine.initialize()?;
let diagnostics = engine.diagnostics()?;
assert!(diagnostics.revision >= 1);
# Ok(())
# }
```

Diagnostics are bounded and read-only. They identify unsupported selector
syntax, properties, values, or malformed CSS with a source kind, bounded
offset, and sanitized detail token. They do not echo raw CSS and are not part
of stable `BrowserBackend` evidence, CLI, or MCP capabilities.

`NativeEngineBackend` implements the existing `BrowserBackend` contract. Its
internal DOM, node IDs, scheduler tasks, resource records, and future layout
types never cross the transport-neutral backend boundary.

When the feature is enabled, `BrowserRuntimeSession::connect_native` is the
explicit user-facing Rust session constructor. It accepts a
`NativeEngineConfig`, initializes one native backend, and never interprets a
network endpoint. The native runtime is not selected by omission or automatic
backend ranking.

The same feature exposes `--browser-runtime native` in the one-shot CLI. The
CLI constructs the default local configuration, so it accepts `about:blank` and
bounded percent-decoded or standard padded-base64 `data:text/html` navigation.
Rust callers can still register bounded
`fixture://` documents through `NativeEngineConfig`; fixture registration is
not a CLI file-loading or network capability. Native CLI commands are limited
to navigate, click, type, text, observe, and targets, with semantic locators
instead of CSS selectors. Endpoint, external lifecycle, profile, screenshot,
storage, download, prompt, script/evaluate, MCP, and TUI paths fail closed.

## Configuration and limits

`NativeEngineConfig` contains an initial URL, a viewport descriptor, fixture
documents, and `NativeEngineLimits`. The viewport is the stable owner for
bounded layout, root scrolling, and logical paint output; it does not imply
general browser layout or painting.

The default limits are intentionally bounded:

| Resource | Default limit | Behavior at the limit |
|---|---:|---|
| source document | 256 KiB | navigation fails before state commit |
| DOM nodes | 4,096 | parse fails before state commit |
| open DOM depth | 128 | parse fails before state commit |
| visible text | 16 KiB | evidence is marked incomplete |
| history entries | 64 | oldest entry is evicted deterministically; each entry retains one root scroll point |
| queued scheduler tasks | 256 | scheduling fails explicitly |
| registered fixtures | 32 | configuration fails explicitly |
| retained CSS diagnostics | 256 | later diagnostics are dropped and `truncated` is set |

All configured URLs and fixture bodies are validated before engine startup.
Limits are configuration errors, not silent truncation, except for the
document's explicitly bounded evidence projection.

## Resource model

The current resource boundary (the Phase 1 loader) supports only:

- `about:blank`, which loads an empty document;
- `data:text/html,...` with UTF-8 percent-decoded HTML;
- `data:text/html;base64,...` with standard padded RFC 4648 base64 decoding to
  UTF-8 HTML; and
- exact `fixture://...` URLs registered in `NativeEngineConfig`.

HTTP, HTTPS, filesystem, custom network, redirects, cookies, and all other
resource schemes fail closed. The resource loader has no filesystem or network
capability. A raw fragment is removed for resource lookup and decoding but is
retained in the successful navigation URL; percent-encoded fragment markers
remain payload data. Every successful resource has an opaque origin placeholder until
the origin and security workstream defines a stronger model.

## Lifecycle and state ownership

The state machine is terminal after close:

```text
New --initialize--> Running --close--> Closed
 |                    |
 +-- invalid ----------+-- navigation/evidence require Running
```

Initialization loads the configured initial URL and creates revision `1`.
Each successful navigation parses into a new document before mutating engine
state, then commits one monotonic revision and one history entry. Failed URL,
resource, parse, or limit checks leave the previous document, URL, revision,
and history unchanged. Close is explicit; a closed engine cannot be reopened.

One fixed context ID, `native-context`, is exposed. The current lifecycle and
context slice has no popups, frames, workers, or background contexts.

The DOM is an arena of generational `NodeId` values. A navigation constructs a
new document generation, so a node identity from an earlier document cannot be
mistaken for a current node. Public native semantic references additionally
carry the current revision (`ref=r<revision>:n<node-index>`); a reference from
before any navigation or mutation is rejected as detached.

Same-document fragment navigation changes the URL, revision, and history
cursor while retaining the current document generation and derived layout,
control state, and bounded root-scroll state. If the fragment's bounded
percent-decoded UTF-8 value identifies one visible exact `id`, the root
viewport is positioned at that element's clamped document-space top; missing,
empty, malformed, invalid-UTF-8, hidden, duplicate, and non-layout targets
preserve the current offset. Explicit Rust back/forward traversal moves
the bounded history cursor; different-resource entries are parsed before
commit, while same-resource fragment entries reuse the current document and
restore the target history entry's saved root offset.

Semantic anchor clicks use the existing action path as one bounded default
action. Fragment-only hrefs resolve against the current local resource, and
absolute about:blank, data:text/html, and registered fixture:// hrefs use the
existing loader. Non-fragment relative references resolve only against the
current registered fixture:// host; non-fragment relative links from
about:blank or data: URLs, host-changing references, remote schemes, and other
unsupported destinations fail before mutation. Empty hrefs remain click-only.
Successful link
activation commits the existing same-document or parse-before-commit
different-resource navigation path without adding a transport-level operation.

The scheduler owns a deterministic logical clock and bounded ordered task
queue. It commits navigation in a reproducible order. Interaction mutation is
synchronous and single-owner; the scheduler does not spawn threads, sleep, or
execute arbitrary callbacks.

## Phase 2 semantic DOM/interaction, initial Phase 3 presentation/layout, and runtime slice

The current Phase 2 slice intentionally exposes a narrow semantic surface
without pretending to implement general CSS selectors, general layout, or a
browser event loop.
`NativeDocument::semantic_nodes` projects supported native roles and bounded
names in source order. Supported role inference includes buttons, links with
`href`, text-like inputs, textareas, checkboxes, radios, selects, options, and
headings. `aria-label`, `aria-labelledby`, associated/ancestor labels, and
visible element text are used in that order where applicable.

The tokenizer treats `script` and `style` content as raw text and `title` and
`textarea` content as RCDATA until their matching end tags. Markup-looking
content inside those elements cannot create semantic descendants. Unterminated
content is consumed to the bounded document end; this is parser containment,
not an HTML5 conformance claim.

The initial presentation subset reads bounded `style` elements and inline
`style` attributes. It matches bounded chains of one to eight universal/type,
ID, class, or attribute-presence/exact-value compounds joined by descendant
whitespace, then cascades `display` and `visibility` by summed specificity,
source order, and inline precedence. Unsupported selectors and declarations
are ignored. This state feeds text exclusion, semantic actionability, and the
bounded paint artifacts; it does not imply general CSS, inheritance, or
general paint. Direct-child, sibling, pseudo, functional, namespace, and
other combinators remain unsupported.

The 030 selector boundary resolves descendant chains through the same
arena-owned parent links used by semantic projection and layout. Intermediate
ancestors may be skipped, but traversal is capped by the native DOM depth and
the selector's eight-compound limit. The computed result is therefore still a
deterministic derived style; no selector cache or second ownership graph is
introduced.

The 009 layout seed derives integer-pixel rectangles from the current
presentation state and configured viewport using normal block/inline flow.
The 016 box-model extension adds bounded uniform `padding:Npx` and
`margin:Npx`, explicit `box-sizing:content-box|border-box`, and a derived
content rectangle after border/padding insets. Percentages, negative/auto
values, logical sides, margin collapsing, positioning, flex, grid, transforms,
and font metrics remain unsupported at that checkpoint. Layout is recomputed as a
Rust-only derived view. `display:none`, explicit hidden signals, and
`visibility:hidden` remove boxes; `display:contents` preserves eligible
descendants without creating its own box.

The 017 viewport-scroll boundary derives bounded document content height and a
single vertical maximum offset. `NativeLayoutSnapshot` keeps its boxes in
document coordinates and exposes the offset/max-scroll metadata; point hit
testing adds the offset before resolving a deepest visible box. The display
list carries the same offset and software replay translates its bounded
rectangles/points into viewport coordinates. Scroll deltas are explicit native
actions, clamp at the document edges, and never scroll a semantic target
implicitly into view.

The 018 side-specific-border boundary kept the existing uniform `border:`
shorthand and adds independently cascaded physical `border-top`,
`border-right`, `border-bottom`, and `border-left` declarations in the same
bounded `Npx solid <color>` grammar. Each side contributes its own outer/content
inset and paint data; software replay uses a deterministic corner precedence
after ancestor clipping and root-scroll translation. At that checkpoint, other
border styles, radii, border images, gradients, logical writing-mode sides, and
browser corner joins remained unsupported.

The 019 bounded-pattern-border boundary extends the same physical declarations
with `Npx dashed <color>` and `Npx dotted <color>`. Styles are typed through
computed CSS and the immutable display list. Software replay uses bounded
integer dash/gap or dot/gap periods anchored to document-space geometry, then
applies the existing side precedence, ancestor clips, source-over blending,
and root-scroll translation. Other border styles, standalone `border-style`
properties, radii, images, gradients, logical sides, anti-aliased joins, and

The 020 bounded-corner-radius boundary adds the physical `border-radius`
shorthand with one-to-four non-negative integer-pixel values. The values expand
to physical corners, normalize conservatively to the concrete box, and travel
through computed style, layout metadata, display commands, rounded background
and border replay, and point hit testing. Software uses deterministic
pixel-center circle masks after the existing rectangular ancestor clips and
root-scroll translation. Percentages, slash-separated elliptical radii,
corner longhands, rounded descendant overflow clips, anti-aliasing, and
browser corner-join fidelity remain unsupported.

The 021 bounded-inline-flow boundary fixes placement of adjacent supported
inline element boxes by preflighting their integer outer width and margins
against the remaining line width before creating their layout boxes. It keeps
whole inline boxes atomic, preserves fixed line-height flow, and feeds the
same document-space coordinates to display-list, hit-test, and root-scroll
consumers. At that checkpoint, font metrics/shaping, general word-aware text
fragments, CSS whitespace modes, CSS word-breaking variants, baselines, bidi,
floats, replaced elements, flex/grid, and general inline-formatting parity
remained unsupported.

The 022 bounded-fixed-line-height boundary accepts one positive integer-pixel
`line-height` value with the existing cascade precedence. A flow owner's value
sets the minimum line-box height for direct text and inline children, while an
inline element's own value raises its auto content height and never overrides
an explicit `height`. Unitless, relative, percentage, `normal`, general
inheritance, font metrics, baselines, vertical alignment, and browser
line-layout parity remain unsupported.

The 023 bounded-direct-text-flow boundary collapses each visible direct text
node through the existing bounded text policy, fragments it from the same
integer flow cursor that places inline boxes, and records the resulting
document-space origins. Layout also retains source-order box/text entries, so
the display list no longer re-derives aggregate direct text at an element's
content origin. Fragments use the containing element's computed color and
ancestor clip and remain subject to root-scroll translation. CSS whitespace
modes, CSS word-breaking variants, font metrics/shaping, baselines, bidi,
At that checkpoint, whitespace joining across nodes and inline descendants,
and browser inline-formatting parity, remained unsupported.

The 024 bounded-word-wrap boundary consumes the 023 collapsed text fragments
as ASCII-space-separated words. A complete word plus its separator stays on the
current fixed-width line only when it fits; otherwise the word starts on a
fresh line without that separator, and a word wider than the full line is
split by fixed character capacity. This improves fixture readability without
adding CSS whitespace modes, word-break/overflow-wrap behavior, hyphenation,
font metrics/shaping, baselines, bidi, cross-node whitespace joining, or
browser inline-formatting parity. At that checkpoint, source whitespace
boundaries across separate flow items remained unsupported.

The 025 bounded-physical-box-edges boundary extends the 016 uniform box-model
seed with one-to-four-value physical `padding` and `margin` shorthands plus
the top/right/bottom/left longhands. Each side is cascaded independently using
the existing specificity, source-order, and inline precedence rules. The
resulting side values feed outer/content rectangles, child and direct-text
origins, block/inline normal-flow margins, and the existing paint, hit-test,
and root-scroll projections. Values remain bounded non-negative integer pixels;
invalid, negative, percentage, unitless, `auto`, logical-side, and more than
four-value declarations are ignored. Margin collapsing, positioning, flex,
grid, and general CSS layout remain unsupported.

The 026 bounded-whitespace-boundaries boundary retains whether each direct
text node begins or ends with author whitespace and shares one pending
separator across sibling direct text, `display:contents`, and supported inline
flow items. It paints a consumed separator through the existing containing
element text-fragment path, drops leading whitespace at a fresh line, and
drops a separator when the next inline item or word must wrap. Direct text with
no source boundary remains adjacent. CSS `white-space` modes, preserved
tabs/newlines, `word-spacing`, Unicode line breaking, bidi, font
metrics/shaping, anonymous inline boxes, whitespace joining across independent
nested flow owners, and browser inline-formatting parity remain unsupported.

The 027 bounded-overflow-hit-test-projection boundary derives the same
rectangular `overflow:hidden` ancestor intersection for layout consumers that
the paint path already uses. `NativeLayoutSnapshot` retains one bounded clip
per layout box in document coordinates; viewport rectangle projection applies
it before root-scroll translation, and point hit testing rejects descendants
outside it. The clip remains rectangular and does not create nested scrolling,
axis-specific overflow, or rounded descendant clip geometry.

The 031 bounded-overflow-clip boundary accepts `overflow:clip` as the same
non-scrolling rectangular clip primitive. It shares the existing style cascade,
clip intersection, viewport projection, and point hit-testing consumers with
`overflow:hidden`; it adds no nested scroll offset, scrollbar, axis-specific
overflow, or public computed-value distinction. `visible`, `auto`, and
`scroll` remain unsupported and continue to produce bounded diagnostics.

The 032 bounded-hard-line-breaks boundary treats a visible `<br>` as a hard
line break in the existing integer inline flow. It advances the containing
cursor by the current fixed line-height floor, resets the inline origin, and
creates no semantic node, layout box, or paint command. Hidden or
`display:none` breaks are ignored. At that boundary, `<wbr>`, preserved source
newlines, CSS `white-space` modes, font metrics, and browser inline-formatting
parity remained unsupported.

The completed 033 bounded-pre-line-breaks boundary supports inherited
`white-space: pre-line` as a source-newline mode. LF, CR, and CRLF boundaries
reuse the same hard-break cursor transition while other whitespace remains
collapsed. `normal` remains the default. At the 033 boundary, `pre`,
`pre-wrap`, `break-spaces`, `nowrap`, preserved tabs, and general CSS
white-space conformance were outside the native claim. Its implementation and
full validation evidence are recorded in the task file and issue #40.

The 034 bounded-preformatted-whitespace boundary adds inherited
`white-space: pre`. Literal source spaces, tabs, and other non-line-break
characters retain deterministic one-cell advances; LF, CR, and CRLF reuse the
same hard-break cursor transition, and preformatted segments do not soft-wrap
at the content width. A wide line is bounded by the existing surface and
ancestor clips rather than reflowed or horizontally scrolled. At the 034
boundary, tab stops, font metrics, unsupported glyph fidelity, `pre-wrap`,
`break-spaces`, `nowrap`, text alignment, and general CSS whitespace/overflow
conformance were outside the native claim. Its implementation and full
validation evidence are recorded in the task file and issue #40.

The 035 bounded-pre-wrap-whitespace boundary adds inherited
`white-space: pre-wrap`. Literal source spaces, tabs, and other non-line-break
characters remain fixed-cell text, LF, CR, and CRLF reuse the same hard-break
cursor transition, and source segments may soft-wrap at deterministic
fixed-cell content capacity. Text runs may split at those boundaries while
semantic, layout, and paint node ownership remains unchanged. Browser line
breaking, tab stops, font metrics, shaping, baselines, bidi, justification,
`break-spaces`, `nowrap`, and general CSS whitespace/overflow conformance
remain unsupported. Its implementation and validation evidence are recorded
in the task file and issue #40.

The 036 bounded-base64-data-url boundary accepts standard padded base64
`data:text/html` payloads through the existing local resource loader and real
navigation/dispatcher path. A derived encoded bound is checked before decode,
the decoded UTF-8 body is checked against the document limit, and successful
resources retain the original URL with the existing opaque local origin.
Non-HTML media types, invalid or URL-safe/whitespace-tolerant encodings,
percent-encoded base64, invalid UTF-8, and network/filesystem schemes remain
rejected; this adds no subresource, script, storage, or remote-content
behavior. Its implementation and validation evidence are recorded in the task
file and issue #40.

The 037 bounded-fragment-navigation-history boundary treats a raw URL fragment
as navigation metadata rather than document payload. Same-resource fragment
navigation validates the local resource, preserves the current document and
scroll state, and commits one revision/history entry through the existing
navigation path. Explicit Rust back/forward traversal moves the bounded
history cursor without creating entries, reparses different local resources
before commit, and returns a no-op at either boundary. At the 037 checkpoint,
anchor scrolling, network navigation, redirects, HTTP state, credentials,
cookies, and shared transport history operations remained unsupported; the
039 slice adds the bounded anchor-scroll behavior described below. Its implementation and
validation evidence are recorded in the task file and issue #40.

The 038 bounded-local-link-activation boundary routes semantic `<a href>`
clicks through the existing navigation owner. It accepts only fragment-only
and absolute local hrefs, prevalidates unsupported destinations, keeps empty
hrefs click-only, and preserves the 037 same-document, history, and
parse-before-commit invariants. At the 038 checkpoint, relative paths, remote/
network navigation, downloads, target contexts, and event-loop/default-action
behavior remained unsupported; 040 adds only bounded fixture-relative
resolution. Its implementation and validation evidence are recorded in the
task file and issue #40.

The 039 bounded-fragment-target-scroll boundary resolved one exact raw local
`id` through the existing document/layout owner and positioned the root viewport
at its clamped document-space top. The later 041 bounded-percent-decoded-
fragment-target boundary applied bounded UTF-8 percent decoding before that
exact lookup, 042 added the legacy `<a name>` fallback, and 043 added simple
text-fragment matching. Every history entry stores one bounded root scroll
point; scroll actions update the active entry and traversal restores it,
including reparsed different-resource entries. Prefix/suffix text-fragment
syntax was unsupported at this 039 checkpoint; the later 044 checkpoint adds
bounded exact same-run affixes. Duplicate-id recovery beyond decoded equality,
malformed URL policy, smooth/nested/horizontal/keyboard/snap scrolling, sticky
layout, and browser alignment parity remain unsupported. Each implementation and validation
boundary is recorded in its task file and issue #40.

The 040 bounded-relative-local-links boundary resolves non-absolute link
references only from registered `fixture://` documents, retaining the current
fixture host and using the bounded URL parser for deterministic path/query/
fragment normalization. Relative links from opaque `about:blank` or `data:`
documents, host-changing or malformed references, remote schemes, and missing
fixtures fail before mutation; same-/different-resource commit, anchor scroll,
and per-entry history restoration continue through the existing owners. Its
implementation and validation evidence are recorded in the task file and issue
#40.

At the 041 bounded-percent-decoded-fragment-target checkpoint, local fragment
navigation decoded bounded percent-encoded UTF-8 bytes before the existing
exact visible `id` lookup. Literal `+` remained a plus, and malformed or
invalid-UTF-8 fragments remained successful unresolved-target navigations that
preserved scroll. The later 042 and 043 checkpoints add legacy name anchors and
simple text fragments; general URL-decoding parity remains unsupported. Its
implementation and validation evidence are recorded in the task file and issue
#40.

The 042 bounded-legacy-name-fragment-target boundary is complete locally:
after 041 decoding, a unique exact `<a name>` anchor is used only when no
matching `id` exists. IDs retain precedence, duplicate IDs or names fail
closed, and the existing visible-layout-box, root-scroll, history, and
failure-atomicity rules remain unchanged. Arbitrary `name` attributes and
browser URL/scrolling parity remain unsupported; the subsequent 043 checkpoint
adds only the bounded text-fragment form. Its implementation and validation
evidence are recorded in the task file and issue #40.

The 043 bounded-text-fragment-target boundary is complete locally: a simple
`#:~:text=start` or `#:~:text=start,end` request matches the first visible,
non-truncated text run after per-term bounded UTF-8 decoding. Prefix/suffix
syntax, cross-run ranges, highlights, and browser text-fragment parity remain
unsupported. Its implementation and validation evidence are recorded in the
task file and issue #40.

The 044 bounded-text-fragment-affix boundary is complete locally. It adds the
accepted exact same-run `prefix-,start`, `start,-suffix`, and combined affix
forms around the existing `start[,end]` matcher, including the corresponding
forms with an ordered end term. Raw-comma separation, per-term bounded UTF-8
decoding, first-visible-run order, root-scroll/history behavior, and
fail-closed handling for malformed or unsupported forms remain intact.
Cross-run ranges, multiple directives, highlights, Unicode normalization, and
browser text-fragment parity remain unsupported. Its implementation and
validation evidence are recorded in the task file and issue #40.

The 045 bounded-root-horizontal-scroll boundary is complete locally. It
extends the existing root scroll offset with bounded document width,
independent horizontal/vertical clamping, and the already shared viewport,
hit-test, display-list, and raster projections. Nested scroll containers,
scrollbars, smooth scrolling, and axis-specific CSS overflow remain outside
the slice. Its implementation and validation evidence are recorded in the
task file and issue #40.

The completed 046 bounded-nowrap-whitespace boundary adds inherited
`white-space: nowrap` to the existing collapsed fixed-cell text flow. It keeps
the resulting text on one measured line and reaches the 045 root horizontal
scroll path through the existing layout, viewport, hit-test, display-list,
raster, action, and history owners. Preserved whitespace modes beyond the
supported subset, nested scrolling, scrollbars, and browser line-breaking
parity remain outside the slice. Its implementation and validation evidence
are recorded in the task file and issue #40.

The completed 047 bounded-inherited-line-height boundary propagates the
existing positive pixel `line-height` floor through the DOM style walk so
descendants without a valid local declaration use the nearest computed value.
Font-relative metrics, CSS-wide keywords, baselines, `vertical-align`, and
general computed-style inheritance remain outside the slice.

The completed 048 bounded-clip-aware-root-overflow boundary measures text
contribution to root `content_width` through the same document-space overflow
clip already shared by paint, viewport projection, and point hit testing, so
fully clipped text cannot create a false horizontal scroll range.

The completed 049 bounded-axis-specific-overflow boundary accepts only bounded
`overflow-x`/`overflow-y` `hidden` and `clip` values, cascades the two axes
independently, and preserves the existing rectangular clip owner across paint,
viewport projection, hit testing, and root-overflow measurement.

The completed 050 bounded-min-max-dimensions boundary applies bounded physical
minimum and maximum pixel constraints to the existing content-box or border-box
width/height calculations while preserving normal flow and all downstream
geometry owners.

The completed 051 bounded-opacity-groups boundary adds local CSS opacity as
real subtree compositing. Reduced-opacity elements bracket their own box and
rendered descendants with display-list markers; the software rasterizer
replays each group in a transparent layer before source-over compositing it
onto its parent. Fixed-point alpha parsing, nested groups, `display:contents`,
zero-opacity layout/hit behavior, and explicit layer budgets are in scope;
general stacking, transforms, filters, animation, and browser compositor parity
remain outside the boundary. The completed 052 boundary adds bounded inherited
physical text alignment for fixed-cell direct text and supported inline boxes;
logical directions, justification, and browser inline-formatting parity remain
outside the boundary. The completed 053 boundary extends the existing bounded
color grammar with decimal-channel `rgba(R, G, B, A)` values using fixed-point
alpha quantization and the existing source-over raster path; CSS Color 4,
wide-gamut, interpolation, and color-management behavior remain outside the
boundary. The completed 054 boundary adds inherited fixed-cell
`text-decoration:none|underline` as a text-command bit and a clipped,
alpha-aware one-pixel baseline-offset replay. Font metrics, decoration
propagation, and browser text-paint parity remain outside the boundary. Its
implementation and validation evidence are recorded in the 054 task file and
issue #40. The completed 055 boundary adds inherited ASCII
`text-transform:none|uppercase|lowercase` during fixed-cell layout before
whitespace handling, wrapping, text-fragment matching, display-list projection,
and root-overflow measurement. Semantic source text remains unchanged; Unicode
case mapping, locale behavior, and font-specific glyph metrics remain outside
the boundary. Implementation and validation evidence are recorded in the 055
task file and issue #40.
The completed 056 boundary adds non-negative fixed-pixel `text-indent` for the
first line of block containers. It reduces only that line's fixed-cell
capacity, resets later lines to the full content width, and leaves inline and
`display:contents` elements on their containing block's flow. The effective
indent is clamped to retain one fixed cell; negative, percentage, and
font-relative forms remain outside the boundary. Implementation and local
validation evidence are recorded in the 056 task file and issue #40; remote
CI remains pending until this branch is pushed.
The completed 057 boundary adds bounded inherited non-negative fixed-pixel
`word-spacing` to the existing fixed-cell text-flow owner. Collapsed ASCII
separator spaces receive the extra advance in `normal`, `nowrap`, and
`pre-line`; literal ASCII spaces receive it in `pre` and `pre-wrap`. The
measured advance is shared by wrapping, preformatted chunking, text fragments,
alignment, display commands, raster replay, hit testing, and root overflow;
negative, relative, percentage, keyword, Unicode-whitespace, and browser
word-boundary behavior remain outside the boundary. Implementation and local
validation evidence are recorded in the 057 task file and issue #40; remote
CI remains pending until this branch is pushed.
The completed 058 boundary adds bounded inherited non-negative fixed-pixel
`letter-spacing` after every rendered fixed-cell character in each emitted
fragment, including the final character, and composes it with the existing
word-spacing advance on ASCII spaces. The measured result is shared by
wrapping, preformatted chunking, text fragments, alignment, display commands,
raster replay, hit testing, and root overflow. Fragment and line boundaries,
pair-boundary typography, Unicode shaping, font metrics, negative/relative/
percentage values, `normal`, and browser parity remain outside the boundary.
Its implementation is committed locally as `cb191a3`; the task file records
the local validation evidence, and remote CI remains pending until this branch
is pushed.

The completed 059 boundary adds inherited `font-weight: normal|bold|400|700`
to the fixed-cell text path. `normal`/`400` retain the existing glyph replay
and `bold`/`700` apply a one-pixel horizontal dilation of set glyph pixels
through the existing clipped software rasterizer. Fixed-cell advances,
wrapping, origins, overflow, hit testing, semantics, and capture coordinates
do not change. Font selection/loading, metrics, numeric interpolation,
variable fonts, shaping, anti-aliasing, and browser text-rendering parity
remain unsupported. Its implementation is committed locally as `21fcff5`; the
059 task file records the local validation evidence, and remote CI remains
pending until this branch is pushed.

The completed 060 boundary adds inherited `font-style: normal|italic` to the
fixed-cell text path. `normal` retains the existing glyph replay and `italic`
applies a deterministic bounded row-dependent horizontal shear through the
existing clipped software rasterizer. Bold dilation, underline, spacing,
opacity, scrolling, and capture compose through the same immutable text
command and raster owners; fixed-cell advances, wrapping, origins, overflow,
hit testing, semantics, and capture coordinates do not change. Oblique forms,
angles, font selection/loading/metrics, shaping, anti-aliasing, and browser
text-rendering parity remain unsupported. Its implementation is committed
locally as `2992eb8`; the 060 task file records the local validation evidence,
and remote CI remains pending until this branch is pushed.

The completed 061 boundary adds inherited `word-break: normal|break-all` to
the bounded collapsed fixed-cell flow path. `normal` retains word-aware
wrapping; `break-all` permits deterministic character-boundary splitting for
every collapsed word while preserving the existing separator, spacing,
fragment, overflow, and semantic owners. `pre`, `pre-wrap`, and `nowrap`
retain their existing whitespace behavior. Unicode line-breaking, grapheme
policy, hyphenation, `overflow-wrap`, bidi, writing modes, font metrics, and
browser conformance remain outside the boundary. Its design is `14d7fc4`, its
implementation is `479f3a3`, and its documentation closeout is `433d6fd`;
the 061 task file records the local validation evidence, and remote CI remains
pending until this branch is pushed.

The completed 062 boundary adds local `text-overflow: clip|ellipsis` to the
bounded single-line fixed-cell path. `clip` retains the existing full visual
run under the horizontal overflow clip; eligible `ellipsis` blocks replace an
overflowing suffix with a spacing-aware fixed-cell ASCII `...` marker while
retaining the full semantic source text. The implementation is restricted to
one direct text child in a rendered `nowrap` block with a finite horizontal
clip; multi-line truncation, nested inline formatting, Unicode ellipsis
behavior, and browser conformance remain outside the boundary. Its design is
`7e488aa`, its implementation is `e4c5bb1`, and the 062 task file records the
local validation evidence; remote CI remains pending until this branch is
pushed.

The completed 063 boundary adds inherited
`vertical-align: baseline|top|middle|bottom` to the existing fixed-cell
inline-flow line-item owner. `baseline` preserves the current top-origin
behavior; `top`, `middle`, and `bottom` apply bounded integer offsets within
the existing line box and move an inline item's boxes and text artifacts
together. Font metrics, typographic baselines, lengths, percentages, bidi,
writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. Its design is `7721df2`, its implementation is
`facd2f6`, and the task file records the local validation evidence; remote CI
remains pending until this branch is pushed.

The completed 064 boundary adds block-level `display: flex` for eligible
containers with direct element children. Each item is laid out once in source
order at its explicit or intrinsic fixed width, including physical margins,
against the existing content-box origin. Items do not grow, shrink, wrap,
reverse, reorder, distribute free space, or stretch across the cross axis.
Meaningful direct text, `display: contents`, and visible `<br>` children use
the established normal-flow fallback so the bounded mode never drops source
content. The contract is recorded in
`docs/plan/tasks/native-engine-064.md`; implementation `7c38354` and local
validation evidence are recorded in that task file. Remote CI remains pending
until this branch is pushed.

The completed 065 boundary adds one non-negative fixed-pixel `gap` value between
visible direct element items in an eligible bounded flex row. Hidden and
`display:none` items do not consume a gap position; ineligible containers keep
the existing normal-flow fallback. Multi-value and percentage gap grammar,
`row-gap`, `column-gap`, flex distribution, wrapping, and general Flexbox
remain outside the claim. The contract is recorded in
`docs/plan/tasks/native-engine-065.md`; implementation `28735c4` and local
validation evidence are recorded in that task file. Remote CI remains pending
until this branch is pushed.

The completed 066 boundary adds bounded
`justify-content:flex-start|center|flex-end|space-between` to eligible
fixed-width flex rows. Positive free space is placed before the row or
distributed across the existing gaps with deterministic integer rounding;
overflowing rows retain a zero leading offset and the existing root-scroll
path. The contract is recorded in
`docs/plan/tasks/native-engine-066.md`; design is `a53b10f`,
implementation is `3fe5306`, and the current-source documentation closeout
is recorded in this checkpoint. Remote CI remains pending because the branch
is local-only.

The completed 067 boundary adds bounded non-inherited signed `order` values in
the inclusive range `-1024..=1024` for eligible fixed-width flex rows. Visual
items sort by `(order, source_index)` before the existing gap and
`justify-content` distribution, while semantic DOM/source order remains
unchanged. The contract is recorded in
`docs/plan/tasks/native-engine-067.md`; design is `09f3b00`, implementation is
`a713b6e`, and current-source documentation closeout is recorded in this
checkpoint. Remote CI remains pending because the branch is local-only.

The completed 068 boundary adds bounded non-inherited
`align-items:flex-start|center|flex-end` to eligible fixed-width single-row
flex rows. It aligns complete visual item subtrees within an explicit content
height or the auto row's maximum item outer height using deterministic integer
offsets, while preserving horizontal and semantic/source order. The contract
and exclusions are recorded in
`docs/plan/tasks/native-engine-068.md`; design is `a0488ef`, implementation is
`6b55b9c`, and current-source documentation closeout is recorded in this
checkpoint. Remote CI remains pending because the branch is local-only.

The completed 069 boundary adds bounded non-inherited
`flex-direction:row|row-reverse` to the same eligible single-row flex rows.
`row` remains coordinate-equivalent to 068; `row-reverse` walks the
order-sorted visual sequence from the physical right edge while preserving
margins, gap, justification, cross-axis alignment, shared subtree artifacts,
non-negative coordinates, root horizontal scrolling, and semantic/source
order. The contract and exclusions are recorded in
`docs/plan/tasks/native-engine-069.md`; design is `7fea901`, implementation is
`be11f49`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-070` slice adds bounded
non-inherited `flex-wrap:nowrap|wrap` to the same eligible fixed-width flex
rows. `nowrap` remains coordinate-equivalent to 069; `wrap` forms deterministic
physical lines from measured item outer widths plus the existing gap, reuses
per-line row/reverse placement, justification, and cross-axis alignment, and
preserves complete subtree artifacts, root overflow, and semantic/source order.
The contract and exclusions are recorded in
`docs/plan/tasks/native-engine-070.md`; design is `8772a6a`, implementation is
`5c16185`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

The 010 paint boundary derives a matching immutable display list with a white
viewport clear, explicit bounded solid backgrounds, and direct visible text
runs. The 011 raster boundary replays that list into a capped logical RGBA
surface using integer source-over blending and a fixed 5x7 ASCII glyph subset.
The 015 capture boundary encodes that current logical surface as bounded PNG
bytes through the explicit backend capture operation. It does not add
screenshot-containing evidence levels, physical-device pixels, a font engine,
image decoder, JPEG/PDF encoding, or a GPU path.

The 012 style boundary resolves only `color` through parsed DOM parent
links. Explicit child declarations remain authoritative, while absent values
inherit the nearest resolved ancestor color and otherwise use opaque black.
Other CSS inheritance and all user-agent/font/color-management behavior remain
unsupported.

The 013 paint boundary carries the intersection of bounded `overflow:hidden`
ancestor rectangles on fill/text commands and rechecks that intersection at
software replay. It does not create nested scroll containers or a general clip
stack; the later 017 boundary owns only the root viewport offset.

The 014 paint boundary accepts only a uniform `border:Npx solid <color>`
declaration from the existing bounded CSS grammar. It adds one immutable
`BorderRect` command per eligible layout box and replays the border as an
inside-the-box ring with the existing source-over and clip rules. Border paint
itself does not define individual sides, non-solid styles, transforms, or
general CSS borders; the subsequent 016 layout boundary owns the bounded
padding and box-sizing behavior.

The 015 capture boundary is read-only: a running native engine can encode its
current logical RGBA surface as PNG through `CaptureFormat::Png`, subject to
the stable capture-byte limit. JPEG and PDF are explicit denials, and
`EvidenceLevel::Screenshot`/`Combined` remain denied because the stable
evidence result has no image payload. The native CLI still has no screenshot
command.

The 029 capture certification adds a complete small logical-pixel golden for
the existing renderer and compares both direct surface output and decoded PNG
bytes against it. This protects deterministic clear/fill coordinates and the
capture encoder without claiming physical pixels, font/image fidelity,
anti-aliasing, color management, screenshot evidence, or browser parity.

The 016 box-model boundary accepted only uniform, non-negative `padding:Npx`
and `margin:Npx` values plus `box-sizing:content-box|border-box`. The current
025 boundary additionally accepts bounded physical shorthand and longhand
values per side. It keeps
`NativeLayoutBox::rect` as the outer border box and exposes a derived content
rectangle after border and padding insets; child flow and direct text begin at
that content origin. Side-specific margins consume normal-flow space without
margin collapsing. Percentages, negative/auto values, logical sides,
positioning, flex/grid, and fractional metrics remained
unsupported at the 016 checkpoint; later 045 adds bounded root horizontal
scrolling without nested scrolling.

The native locator grammar is explicit and bounded:

```text
ref=r<current-revision>:n<arena-index>
id=<exact-id>
role=<role>
role=<role>[name=<normalized-name>]
name=<normalized-accessible-name>
text=<normalized-element-text>
point=<unsigned-x>,<unsigned-y>
```

Resolution must produce exactly one current element. Unknown locator forms,
missing targets, duplicate matches, stale references, and non-element
references fail explicitly. Actionability checks for disabled controls,
read-only textboxes, and unsupported action roles also fail before mutation.
The grammar is a semantic locator contract, not a CSS selector implementation;
general CSS selector coverage belongs to a later CSS/layout phase.

The native backend accepts semantic `Click`, `Type`, and bounded vertical
`Scroll` actions plus the native `point=<x>,<y>` click-target extension. A
point is checked against the
viewport, resolved to the deepest visible layout box, and walked to the
nearest actionable semantic ancestor before focus or control state changes.
There is no implicit scrolling or nearest-target adjustment. Explicit scroll
changes the root viewport only; horizontal deltas are denied and edge/no-op
scrolls do not mutate the revision. Click focuses
supported buttons, links, checkboxes, radios, textboxes, and comboboxes;
checkbox and radio state changes are retained in the document owner. Clicking
an option in a single-select combobox selects it and clears its siblings. Type
replaces private state for native `input` and `textarea` textboxes. A bounded
visibility gate recognizes `hidden`, `aria-hidden="true"`, and computed
`display:none`/`visibility:hidden`; hidden subtrees are omitted from text,
layout, and hit testing. Supported local links perform the bounded default
navigation described above; unsupported links fail closed. No action performs
keyboard navigation, multi-select, or JavaScript execution.
Each accepted mutating action advances the document revision exactly once, so
earlier references must be re-observed. The effects operation returns the current
revision and changed bit; bounded native event metadata remains an internal
Rust inspection surface.

## Backend capability contract

The native profile is `experimental` and declares:

| Capability | Level | Current contract |
|---|---|---|
| lifecycle | available | initialize and explicit close |
| navigation | available | local `about`, `data`, and registered fixture URLs |
| contexts | available | one active context |
| evidence | available | bounded URL, title, visible text, revision; native semantic projection is Rust-only |
| action | available | semantic click/type, bounded vertical root scrolling, plus native point targets for supported local controls; no nested scrolling or default browser behavior |
| effects | available | current revision and changed signal; bounded event metadata is Rust-only |
| script | omitted | JavaScript is unavailable |
| capture | available | bounded PNG of the current logical RGBA surface; JPEG/PDF and screenshot-containing evidence are unavailable |
| storage | omitted | no cookies/local/session storage |
| prompts | omitted | no dialogs |
| downloads | omitted | no download pipeline |

The profile limitations are surfaced through `BackendProfile`. The dispatcher
returns typed capability denials for omitted operations. `EvidenceLevel::Deep`
can return a bounded incomplete projection; screenshot-containing levels are
explicitly denied. The separate capture operation supports only bounded PNG
bytes. No operation silently falls back to CDP, the proof backend, or another
resource loader.

## Errors and recovery

`NativeEngineError` distinguishes invalid configuration, lifecycle misuse,
unsupported resources, parser failures, resource limits, scheduler failure,
target resolution/actionability failures, and effect-query revision errors.
The backend translates those errors to the existing bounded
`BrowserBackendError` variants.

Navigation follows this transaction boundary:

```text
validate URL -> load local resource -> parse new DOM -> schedule commit
      |                 |                    |              |
      +-- error: no engine mutation --------+--------------+
                                      commit revision/history/document
```

The engine never logs source HTML, evaluated input, credentials, cookies, form
values, or full documents. Later network and scripting work must preserve the
same transaction, origin, cancellation, and redaction boundaries.

## Tests and promotion boundary

Phase 1 and current Phase 2 semantic-DOM/interaction tests cover:

- lifecycle transitions and repeated/invalid close behavior;
- `about:blank`, percent-decoded `data:` HTML, and registered fixtures;
- raw-fragment URL retention and bounded same-document local navigation;
- exact visible fragment-target scroll and saved history offsets;
- bounded percent-decoded fragment identifiers for exact visible local IDs;
- bounded legacy `<a name>` fragment identifiers as an exact-ID fallback;
- bounded `#:~:text=start[,end]` visible text-fragment targets;
- bounded text-fragment prefix/suffix affixes within one visible text run;
- bounded root horizontal/vertical scroll extents, independent clamping, and
  two-axis projection through layout, hit testing, display, raster, and history;
- bounded independent `overflow-x`/`overflow-y` `hidden`/`clip` rectangles
  through paint, viewport projection, point hit testing, and root-overflow
  measurement, with shorthand/longhand cascade precedence;
- semantic local anchor activation through dispatcher click;
- fixture-relative local link resolution with same-host enforcement;
- title and visible-text projection with hidden `head`, `script`, and `style`;
- bounded history entries, monotonic revisions, and explicit Rust traversal;
- deterministic scheduler ordering and queue bounds;
- failed navigation preserving the previous state;
- dispatcher capability denial and explicit-only backend selection;
- supported semantic roles, associated labels, attributes, and revision-bound
  references;
- duplicate and stale semantic targets plus supported control metadata;
- focus, checkbox/radio state, text-control state, bounded effects, and
  pre-mutation rejection of stale, disabled, read-only, and unsupported targets;
- deterministic single-select defaults, option selection, sibling clearing, and
  rejection of unsupported multi-select behavior;
- hidden-subtree text exclusion, semantic hidden metadata, and pre-mutation
  rejection of hidden controls.
- raw-text/RCDATA containment for script, style, title, and textarea content,
  including unterminated-element behavior.
- bounded stylesheet/inline selector matching and display/visibility cascade
  feeding visible text and semantic actionability.
- deterministic integer-pixel normal-flow layout, bounded width/height
  declarations and min/max width/height constraints, physical padding/margin
  shorthand and longhand cascade, explicit box sizing, outer/content
  rectangles, hidden-box exclusion, and Rust-only layout inspection.
- point hit testing with viewport bounds, deepest-hit ordering, actionable
  ancestor resolution, and pre-mutation rejection for empty/out-of-viewport
  points.
- matching-revision display-list generation with bounded clear/fill/text/border
  commands and explicit unsupported-paint boundaries.
- deterministic bounded RGBA surface replay, source-over blending, fixed-glyph
  text drawing, viewport clipping, and explicit surface-allocation limits.
- bounded local opacity parsing and subtree display-list markers, nested
  transparent-layer replay, inside-out source-over group compositing, and
  explicit aggregate layer depth/pixel limits without changing layout or hit
  testing.
- bounded inherited physical `text-align:left|center|right` parsing and
  cascade, deterministic fixed-cell line offsets across direct text and
  supported inline boxes, and shared layout/paint/raster/hit-test coordinates.
- bounded `rgba(R, G, B, A)` functional alpha parsing for background, border,
  and text colors, with shared fixed-point quantization, display-list color
  ownership, and integer source-over replay.
- bounded inherited `text-decoration:none|underline` parsing and cascade,
  immutable text-command decoration bits, and deterministic clipped
  alpha-aware fixed-cell underline replay.
- bounded inherited ASCII `text-transform:none|uppercase|lowercase` parsing and
  cascade, transformed fixed-cell layout fragments, and consistent wrapping,
  text-fragment, display-list, and root-overflow consumers.
- bounded inherited `vertical-align:baseline|top|middle|bottom` parsing and
  cascade, with clamped fixed-cell top/middle/bottom offsets applied to
  complete inline-item box and text artifact ranges during line flush.
- bounded non-inherited `justify-content:normal|flex-start|center|flex-end|
  space-between|space-around|space-evenly|stretch`
  parsing and cascade, fixed-width flex-row free-space placement, deterministic
  gap distribution, overflow preservation, and shared layout/paint/scroll/
  hit-test coordinates.
- bounded non-inherited signed flex-item `order` in `-1024..=1024` parsing and
  cascade, stable visual `(order, source_index)` sorting, source-order semantic
  preservation, and shared layout/paint/scroll/hit-test coordinates.
- bounded non-inherited `align-items:flex-start|center|flex-end` parsing and
  cascade, explicit/auto cross-axis line sizing, complete item artifact
  translation, overflow preservation, and shared layout/paint/scroll/
  hit-test coordinates.
- bounded non-inherited `align-content:flex-start|center|flex-end|space-between`
  for wrapped rows, with explicit-height positive cross-line distribution and
  complete line artifact translation.
- bounded non-inherited `align-content:space-around` for wrapped rows, with
  saturating integer slot-center offsets and complete line artifact translation.
- bounded non-inherited `align-content:space-evenly` for wrapped rows, with
  equal leading, inter-line, and trailing integer slots and complete line
  artifact translation.
- bounded non-inherited `flex-wrap:wrap-reverse` for eligible fixed-width
  rows, preserving source-order line formation while reversing physical line
  stacking with signed complete-artifact translation.
- bounded explicit non-inherited `align-content:stretch` for wrapped
  fixed-width rows, expanding line heights by deterministic integer shares and
  preserving the shared normal/wrap-reverse artifact consumers.
- bounded non-negative fixed-pixel `text-indent` parsing and cascade for the
  first line of block containers, with one-cell clamping and shared
  layout/wrapping/fragment/paint/hit-test/overflow coordinates.
- bounded inherited non-negative fixed-pixel `word-spacing` across collapsed
  and preformatted ASCII spaces, with shared wrapping, fragment, alignment,
  display-list, raster, hit-test, and root-overflow coordinates.
- bounded inherited text-color resolution through DOM parent links, with
  explicit child overrides feeding deterministic text-run commands.
- bounded `overflow:hidden`/`overflow:clip` ancestor intersections on
  fill/text commands and
  software-surface clipping, viewport projection, and point-hit filtering with
  no nested scrolling or screenshot-evidence capability.
- bounded solid-border parsing, deterministic `BorderRect` command ordering,
  inside-the-box border replay, and clip/source-over enforcement.
- independently cascaded physical side-specific solid/dashed/dotted borders,
  side-aware content-box/border-box insets, deterministic side paint data,
  integer pattern phase, corner precedence, ancestor clipping, and root-scroll
  replay.
- bounded one-to-four-value physical border-radius shorthand expansion,
  conservative concrete-box normalization, rounded fill/border replay, and
  rounded point hit testing with explicit rejection of unsupported radius forms.
- bounded inline-box preflight line placement, deterministic line-height flow,
  and consistent layout/display-list/hit-test coordinates for adjacent inline
  elements.
- bounded positive-pixel line-height parsing/cascade, bounded inheritance
  through the DOM style walk, flow minimums, inline auto-height behavior, and
  explicit-height precedence.
- bounded logical-surface PNG encoding, capture-byte enforcement, read-only
  revision behavior, real native backend dispatch, and explicit JPEG/PDF
  denials.
- bounded physical padding/margin cascade, content-box/border-box sizing,
  outer/content layout rectangles, side-aware margin flow, and content-origin
  text paint.
- bounded source-whitespace boundaries across direct text, `display:contents`,
  and supported inline flow items, including source-order separator paint and
  drop-on-wrap behavior.
- bounded rectangular `overflow:hidden`/`overflow:clip` ancestor clips shared by paint,
  viewport rectangle projection, and point hit-testing, including nested clip
  intersection in document coordinates.
- bounded hard `<br>` line breaks in supported inline flow, including leading,
  consecutive, trailing, and hidden-break behavior without synthetic semantic
  or paint nodes.
- bounded root viewport scrolling, content-height/max-offset derivation,
  viewport-to-document hit-test mapping, translated software replay/capture,
  clamping, and revision/effect behavior for moved scroll actions.
- explicit Rust native-session construction and feature-gated CLI dispatch for
  local URL shapes, including rejection of remote endpoints and unsupported
  browser-only flags.

This slice is not browser parity. It cannot be promoted or advertised as safe
for arbitrary remote content until CSS/layout/paint, security policy, process
isolation, cancellation, conformance, and platform evidence exist.

Future phases may split the DOM parser into tokenizer/tree-builder modules and
add general CSS, nested/scrolling/stacking layout, paint, full event-loop, script,
storage, and process boundaries. Those changes require updates to this
document, the epic, and their dependency-ordered task files before
implementation.
