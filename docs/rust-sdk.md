# Rust SDK

Status: Current 0.3.14 source behavior (including current-source work in this checkout)

The workspace publishes two Rust packages with different boundaries:

* `glass-browser` (`glass_browser`) is the browser-control library and
  browser-only CLI product.
* `glass-dev` (`glass_dev`) is the development runtime, resident agent/Pi
  integration, project/editor/process tooling, and the `glass` CLI product.

The public `browser_workspace` module provides the bounded, revision-safe
controller, state, intent, action, capability, entity, target, layout, focus,
ownership, and presentation contracts used by the browser and development
terminal products.

The `glass-browser` package exports the `glass_browser` library. It owns the
policy, revision, semantic, workflow, workspace, and result contracts used by
the browser CLI, browser TUI, MCP server, daemon, and Rust callers.

The focused browser API is documented at
[`docs.rs/glass-browser`](https://docs.rs/glass-browser). The development
runtime API is documented at
[`docs.rs/glass-dev`](https://docs.rs/glass-dev).

docs.rs renders published Rust API artifacts. This guide follows the checked-in
source, which is version `0.3.14` with current-source work. Published docs.rs
pages currently match crate `0.3.14`. Verify newer source-level surfaces,
including `todos` and `AgentTurnMode`, against this checkout rather than
assuming they were part of the last published API page. Some benchmark-style
Cargo examples use
development-only dependencies and are not all listed in the docs.rs example
index; the checked-in examples catalog remains the source-level inventory.


## Dependency and features

Use the focused browser crate when embedding browser control:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1" # only needed when your application reads/writes JSON contracts
```

Cargo aliases the package to `glass`, so Rust imports use `glass::…`. Without
the alias, imports use `glass_browser::…`. The development runtime is a
separate package:

```toml
glass-dev = "0.3"
```

Use `glass_dev::…` for project/editor/process/LSP, resident Pi, harness,
collaboration, workspace-local Agent todos, Ask/Plan/Agent turn mode, and TUI
APIs.
`glass-dev` depends on `glass-browser`; the browser crate does not depend on
`glass-dev`, so browser-only embeddings do not acquire the development runtime.

| `glass-browser` feature | Default | Purpose |
|---|---:|---|
| `visual-compare` | no | PNG comparison helpers for explicit screenshot checks |
| `fuzzing` | no | Fuzz-only hooks; do not enable in normal applications |
| `native-engine` | yes | Glass-owned native runtime and canonical public `BrowserSession`; enabled by default |

docs.rs builds all features. The native engine remains inside
`glass-browser`; development runtime dependencies such as PTY integration are
in `glass-dev`, not optional browser features.

### Native engine

The native engine is the default local backend inside `glass-browser` and the
canonical public Rust session. `BrowserSession` is the `BrowserRuntimeSession`
type under the standard API name. `BrowserSession::start(config)` and
`BrowserSession::start_default()` construct the native backend directly through
`BackendFactory::native`; they never probe for Chrome, connect to CDP, or fall
back to another runtime. `BrowserRuntimeSession::connect_native` remains an
equivalent explicit constructor.

The native implementation supports `about:blank`, bounded percent-decoded or
standard padded-base64 `data:text/html`, registered `fixture://` documents,
configured-root `file:` documents, and external HTTP(S) navigation. This
source behavior is not a browser-conformance or production-security
certification claim; substantial Core Web Profile and public-operation parity
gates remain tracked in issue #40. It also supports bounded fragment navigation
with UTF-8 percent-decoded exact-id or legacy
`<a name>` root scrolling, simple `#:~:text=start[,end]` matching against the
first visible non-truncated text run with exact adjacent prefix/suffix affixes,
per-entry scroll restoration, and explicit
Rust history traversal. IDs take precedence and duplicate legacy names fail
closed; cross-run text ranges, multiple directives, and browser text-fragment
parity remain unsupported. Rust callers can activate fragment-only,
fixture-relative, and external HTTP(S) links through the native semantic
action path; empty hrefs remain click-only. A feature-enabled binary also
exposes the one-shot `--browser-runtime native` path.

`CdpBrowserSession` is the explicit Chromium/CDP migration API. Its
`SessionOptions` configure Chrome launch, attach, headed/headless mode, and
persistent or disposable Chrome profiles. Choosing this type is explicit; the
native `BrowserSession` never falls back to it:

```rust,no_run
use glass_browser::BrowserSession;
use glass_browser::browser::NativeEngineConfig;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let session = BrowserSession::start(NativeEngineConfig::default()).await?;
let page = session.navigate("https://example.com").await?;
println!("{}", page.url);
session.close().await?;
# Ok(())
# }
```

### Modal JavaScript dialogs

Process-backed native dialogs are opt-in so a caller without an independent
controller cannot deadlock inside synchronous page JavaScript. Embedders that
can run a navigation/evaluation future alongside a host prompt may construct
with `BrowserSession::connect_native_with_modal_dialogs(config)`, then obtain
`session.native_dialog_controller()`. The cloneable controller's
`pending_dialog()` reads the exact dialog without taking the page-operation
lock; `resolve_dialog(id, NativeDialogResolution { accepted, prompt_value })`
accepts or dismisses only that identity. For an accepted prompt,
`prompt_value: None` submits its default value. Ordinary `start`,
`start_default`, and `connect_native` constructors retain the nonblocking
event-queue behavior. CLI/MCP/TUI host presentation remains a separate issue
#40 integration gate.

The backend profile declares lifecycle, navigation, up to 32 independent page
targets with one explicitly selected active context, bounded evidence, semantic
input actions, native point hit-testing, and revision effects. Rust callers can
additionally inspect the native layout's outer and
content rectangles, bounded physical four-side padding/margin shorthands and
longhands plus explicit box sizing and bounded physical min/max width/height
constraints with bounded case-insensitive 15-layer/unlayered local
`revert-layer` rollback for finite non-negative integer-pixel `width`, `height`,
`min-width`, `max-width`, `min-height`, and `max-height` with absent local
fallbacks, plus bounded case-insensitive 15-layer/unlayered local `revert-layer`
rollback for `box-sizing`, physical padding and margin edges, including bounded
`margin:auto`, with content-box/zero local fallbacks,
 bounded root horizontal and vertical viewport scrolling, inherited text color, bounded
`overflow:hidden` clips shared by paint, viewport projection, and point
hit-testing, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip`
clips through the same owner, plus bounded case-insensitive 15-layer/unlayered
local `revert-layer` rollback for `overflow`, `overflow-x`, and `overflow-y`,
preserving independent visible/no-clip fallbacks and the existing paint,
projection, point-hit, root-overflow, capture, and semantic/source-order owners;
nested scrolling, scrollbars, and browser-wide overflow semantics remain
outside the boundary,
plus bounded case-insensitive 15-layer/unlayered local
`border-radius:revert-layer` rollback for the one-to-four-value integer
shorthand, preserving the zero-corner fallback and the existing rounded
layout, fill, border, point-hit, capture, raster, overflow, and
semantic/source-order owners; elliptical, percentage, nested-clip, anti-aliasing,
multiple-origin, and browser-wide border-radius
conformance remain outside the boundary,
plus bounded terminal case-insensitive `!important` priority for the complete
physical and horizontal-tb logical radius family, with important candidates
above normal candidates and reversed named-layer order; the supported
text-presentation set (`white-space`, `text-align`, `text-align-last`,
`text-justify`, `direction`, text-decoration line/style/skip-ink/skip-spaces/
thickness/underline-offset, `text-transform`, `font-weight`, `font-style`,
`word-break`, `text-overflow`, `vertical-align`, `text-indent`, `word-spacing`,
`letter-spacing`, and `line-height`) also honors the same terminal priority;
local `display`, `visibility`, and `opacity` declarations honor the same
terminal priority; the normal-only flex and gap declarations (`justify-content`,
`align-items`, `align-self`, `align-content`, `place-content`,
`flex-direction`, `flex-wrap`, `flex-flow`, `order`, `flex-grow`,
`flex-shrink`, `flex-basis`, `flex`, `gap`, `row-gap`, and `column-gap`) honor
the same priority through their existing shorthand/component owners; the six
normal-only dimension declarations (`width`, `height`, `min-width`,
`max-width`, `min-height`, and `max-height`) also honor the same priority
through private doubled candidate streams with important-over-normal ordering,
reversed named-layer priority, inline precedence, invalid-later preservation,
and `revert-layer` rollback; the normal-only physical box-model declarations
(`box-sizing`, physical padding, and physical margin shorthand/longhand edges)
also honor the same priority through private doubled candidate streams with
per-edge importance, preserving content-box/border-box conversion, `auto`
margin provenance, and existing geometry/artifact owners. The normal-only
`overflow`, `overflow-x`, and `overflow-y` declarations also honor the same
priority through private doubled x/y candidate streams with shorthand/x/y
importance, preserving important-over-normal ordering, reversed named-layer
priority, inline precedence, invalid-later preservation, independent axis
projection, and `revert-layer !important` rollback. The horizontal-tb logical
`padding-block`/`padding-inline` and `margin-block`/`margin-inline` shorthands
plus their block/inline start/end longhands project through resolved `ltr`/`rtl`
direction into the same physical edge owners and honor the same bounded
important-over-normal, reversed-layer, inline-important, invalid-later, and
`revert-layer` behavior. The same physical and horizontal-tb logical box-model
owners accept standalone case-insensitive `initial`, `unset`, and one-author-
origin `revert`: padding and margin reset to zero and `box-sizing` resets to
`content-box`. They also accept standalone case-insensitive `inherit`:
physical values copy the parent's effective edges, `box-sizing`, and private
margin `auto` provenance, while logical values read the parent in its
resolved `ltr`/`rtl` direction before projecting into the child; root
fallbacks and omitted-property non-inheritance remain explicit. The dimension
declarations `width`, `height`, `min-width`, `max-width`,
`min-height`, and `max-height` also accept standalone case-insensitive
`inherit`: explicit values copy the parent's computed optional pixel value,
including a parent `None`/auto result, while omitted dimensions remain local
and do not inherit. Percentages, negative lengths, intrinsic sizing, aspect
ratio, and new used-value state remain outside this bounded extension. These six
local dimension declarations also accept standalone case-insensitive `initial`,
`unset`, and one-author-origin `revert`, each resolving to the existing
`None`/auto fallback; `revert-layer` remains a separate lower-layer rollback
and mixed reset tokens remain unsupported. Vertical writing modes, additional
logical properties, and remaining properties remain
outside this bounded inheritance/reset family and generic `!important`
semantics.
The bounded
`background-color`, inherited `color`, and `text-decoration-color` owners
also honor terminal case-insensitive `!important` with the same
important-over-normal and reversed named-layer ordering. The standalone
physical `border-color` shorthand and physical color longhands also honor the
same terminal priority, as do the six supported horizontal-tb logical
`border-block-color`, `border-block-start-color`, `border-block-end-color`,
`border-inline-color`, `border-inline-start-color`, and
`border-inline-end-color` declarations through their existing `ltr`/`rtl`
physical-side projection; the supported text-presentation set also honors the
same terminal priority; remaining properties remain outside this bounded
paint-color priority,
plus exact case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
one-author-origin `revert` for the radius owner: explicit `inherit` copies the
parent's effective four-corner radius, including from unpainted or zero-width
parents; reset forms resolve to zero corners and ordinary omission remains
zero. Mixed CSS-wide/concrete and slash-separated radius forms remain
unsupported. Physical circular corner longhands
`border-top-left-radius`, `border-top-right-radius`,
`border-bottom-right-radius`, and `border-bottom-left-radius` accept one
bounded integer-pixel value or the same standalone CSS-wide keywords and
compose with the shorthand through per-corner source order and
`revert-layer`. Flow-relative corner longhands
`border-start-start-radius`, `border-start-end-radius`,
`border-end-start-radius`, and `border-end-end-radius` use the resolved
horizontal-tb `direction` to map logical block/inline corners to the same
physical streams for `ltr` and `rtl`; vertical writing modes, text orientation,
elliptical, and percentage corner values remain unsupported,
plus bounded case-insensitive 15-layer/unlayered local
`opacity:revert-layer` rollback for the existing local 8-bit opacity owner,
preserving the full-opacity (`255`) fallback, reduced-opacity group markers,
software compositing, and unchanged layout, point-hit, capture, raster,
overflow, and semantic/source-order owners; inherited opacity,
stacking-context/blending parity, filters, animation, multiple origins, and
browser-wide opacity conformance remain outside the boundary,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `display` and `visibility`, resolving through lower concrete candidates or
the normal-flow `display:auto` and visible fallbacks while preserving
`display:none`, `visibility:hidden`, and `display:contents` behavior across
hidden-subtree layout, point-hit, display-list, capture, raster, and
semantic/source-order owners; inherited visibility, display decomposition,
formatting-context parity, table/ruby/flow-root details, animation, multiple
origins, and browser-wide CSS display/visibility conformance remain outside
the boundary,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for physical `border`, `border-top`, `border-right`, `border-bottom`, and
`border-left` owners, resolving each side through lower concrete candidates or
the existing zero-width/no-paint fallback while preserving shorthand/longhand
precedence and the existing box-model inset, border display-list, capture,
raster, point-hit, and semantic/source-order owners; vertical writing modes,
text orientation, elliptical/percentage logical radii, border-image, gradients,
`wavy` and other unsupported border
styles, animation,
multiple origins, and
browser-wide CSS border conformance remain outside the boundary,
plus bounded case-insensitive 15-layer/unlayered local `border-color`,
`border-top-color`, `border-right-color`, `border-bottom-color`, and
`border-left-color` values with one-to-four-value physical shorthand expansion
and independent per-side `revert-layer` rollback to lower colors or bounded
black, preserving border width/style and the existing box-model, display-list,
capture, raster, point-hit, and semantic/source-order owners. Standalone
physical `border-color` and physical color longhands also resolve `currentColor`
from the element's local or inherited color; complete physical border shorthands
also resolve `currentColor` from that same local or inherited color. Gradients,
border-image, and browser-wide border conformance remain outside the boundary,
plus case-insensitive CSS-wide `inherit`, `unset`, `initial`, and `revert` for
the physical border-color owner: explicit `inherit` copies the parent's
effective per-side colors, reset forms resolve to the element's current color,
omission retains the existing black side fallback, a single CSS-wide shorthand
token expands to all four sides, and mixed CSS-wide/color shorthand forms remain
unsupported,
plus bounded case-insensitive 15-layer/unlayered local `border-width`,
`border-top-width`, `border-right-width`, `border-bottom-width`, and
`border-left-width` values with one-to-four-value physical shorthand expansion
and independent per-side `revert-layer` rollback to lower widths or bounded
zero. Width-only declarations do not invent a style or paint a border. The
physical border-width owner also accepts case-insensitive CSS-wide `inherit`,
`unset`, `initial`, and `revert`: explicit `inherit` copies the parent's
effective per-side widths, reset forms resolve to zero, ordinary omission
retains the zero fallback, a single CSS-wide shorthand token expands to all
four sides, and mixed CSS-wide/numeric shorthand forms remain unsupported.
Standalone physical border-width declarations also accept a terminal
case-insensitive `!important` marker: important widths outrank normal widths,
use earliest-named-layer priority inside the bounded author-important
partition, and preserve per-side `revert-layer` and inline behavior; complete
and side-border shorthand priority is described below. The same owner
also accepts bounded case-insensitive 15-layer/unlayered local
`border-style` and physical `border-top-style`, `border-right-style`,
`border-bottom-style`, and `border-left-style` values from the finite
`none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar with
one-to-four-value physical shorthand expansion and independent per-side
`revert-layer` rollback to lower
styles. The style owner also accepts case-insensitive CSS-wide `inherit`,
`unset`, `initial`, and `revert`: explicit `inherit` copies the parent's
effective physical side styles, including private `none`/`hidden` and styles
from unpainted or zero-width parents; reset forms resolve to the private
`none` style, ordinary omission retains the no-style fallback, a single
CSS-wide shorthand token expands to all four sides, and mixed CSS-wide/style
shorthand forms remain unsupported. Standalone physical border-style
declarations also accept a terminal case-insensitive `!important` marker:
important styles outrank normal styles, use earliest-named-layer priority
inside the bounded author-important partition, preserve per-side
`revert-layer`, and retain private `none`/`hidden` no-paint behavior; complete
and side-border shorthand priority is described below. The complete physical
`border` shorthand and the four physical side-border shorthands also accept a
terminal case-insensitive `!important` marker: important shorthand projections
outrank normal projections, use earliest-named-layer priority inside the
bounded author-important partition, preserve per-side `revert-layer` and
  inline behavior, and carry the marker through the existing independent
  width/style/color component streams. The six supported horizontal-tb logical
  complete/side border shorthands also accept a terminal case-insensitive
  `!important` marker: important logical shorthand projections outrank normal
  projections, use earliest-named-layer priority inside the bounded
  author-important partition, preserve per-side `revert-layer` and inline
  behavior, and carry the marker through resolved `ltr`/`rtl` projection into
  the existing independent physical width/style/color component streams. A
  winning `none`
or `hidden` blocks lower
styles and resolves to no
side/zero width before layout and paint; `hidden` remains a private distinction
for future table conflict resolution. Width, style, and color components
compose only after independent resolution, and width-only or style-only
declarations do not invent missing paint components; collapsed-table border
conflict resolution, fractional/percentage widths, vertical writing modes, text
orientation, elliptical/percentage logical radii, `wavy` and other unsupported
styles, and browser-wide border
conformance remain outside
the boundary. The native border surface also accepts bounded horizontal-tb
logical `border-block`, `border-block-start`, `border-block-end`,
`border-inline`, `border-inline-start`, and `border-inline-end` shorthands plus
their `-width`, `-style`, and `-color` component families. One- or two-value
block and inline component pairs map to logical start/end; block sides map to
physical top/bottom, while inline sides map to physical left/right for `ltr`
and right/left for `rtl` through the resolved inherited `direction`. The six
supported logical border-width declarations also accept a terminal
case-insensitive `!important` marker: important logical widths outrank normal
widths, use earliest-named-layer priority inside the bounded author-important
partition, preserve per-side `revert-layer`, and carry that partition through
resolved `ltr`/`rtl` projection; complete/side shorthand projections use the
logical shorthand priority described above. The six supported logical
border-style declarations also accept a terminal case-insensitive
`!important` marker: important logical styles outrank normal styles, use
earliest-named-layer priority inside the bounded author-important partition,
preserve per-side `revert-layer`, carry that partition through resolved
`ltr`/`rtl` projection, and retain private `none`/`hidden` no-paint behavior;
complete/side shorthand projections use the logical shorthand priority
described above. Complete logical shorthands, CSS-wide keywords, `currentColor`, bounded
named-layer and
unlayered `revert-layer`, physical/logical precedence, and component composition
reuse the existing physical border streams and box-model, display-list,
capture, raster, point-hit, and semantic/source-order owners. Vertical writing
modes, text orientation, elliptical/percentage logical radii, border-image,
gradients, table conflict resolution,
multiple origins, and browser-wide logical-border conformance remain outside
the boundary. The complete and physical border shorthands also accept the
exact case-insensitive omitted-component `none` and `hidden` forms through
private no-paint style sentinels; winning omitted-component `hidden` remains
private for future table conflict resolution and arbitrary omitted-component
forms remain outside this bounded surface. The native engine provides bounded side-specific
complete `Npx hidden color` border values with private width/color retention
for future table conflict resolution; current non-table composition suppresses
hidden paint. The native engine also provides bounded side-specific
complete `Npx none color` border values with private width/color retention
for future table conflict resolution; current non-table composition suppresses
none paint. The native engine provides bounded side-specific
solid/dashed/dotted/double/groove/ridge/inset/outset-border paint, bounded
physical circular
border radii, bounded inline-box line placement, bounded fixed pixel line-height
flow with bounded inherited line-height, bounded direct-text flow fragments and
source-order text paint, bounded
word-aware wrapping, bounded source-whitespace boundaries across supported
inline flow, bounded inherited `white-space: nowrap` collapsed one-line flow,
bounded root horizontal scrolling,
bounded local opacity subtree groups through transparent software layers,
bounded inherited physical `text-align:left|center|right` fixed-cell line
placement, bounded functional `rgba(R, G, B, A)` alpha colors for background,
border, and text paint, plus bounded case-insensitive 15-layer/unlayered local
`revert-layer` rollback for `background-color` and inherited `color`, preserving
the existing `None`/parent-root fallbacks and fill/text display-list, clipping,
opacity, capture, and raster owners; `background-color` also resolves
case-insensitive `currentColor` from the element's local or inherited `color`,
and local `color: currentColor` resolves from inherited color with a bounded
black initial fallback without self-recursion. Local `color` also accepts the
case-insensitive CSS-wide `inherit`, `unset`, `initial`, and `revert` keywords:
the inherited forms use the bounded parent-color/black-root fallback, while
`initial` resets to black; omitted direct roots remain `None`. This is the
native engine's one-author-origin model. Gradients, system colors,
percentages, color spaces, and multiple origins remain outside the boundary,
the non-inherited `background-color` owner also accepts those four
case-insensitive CSS-wide keywords: only `inherit` copies the parent's
optional concrete fill, while `unset`, `initial`, and `revert` preserve the
existing no-fill `None` fallback; ordinary omission remains no-fill, plus
bounded inherited fixed-cell
`text-decoration:none|underline|overline|line-through` paint, including
distinct shorthand combinations, bounded local `text-decoration-color` using
the existing fixed palette and alpha grammar plus case-insensitive `currentColor`
resolved from the element's local or inherited `color`, with separate glyph and
line paint, plus case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
`revert`: explicit `inherit` copies the parent's effective decoration color,
the reset forms resolve to the element's current glyph color, and omission
keeps the existing local fallback. Bounded `text-decoration-line` longhand
combinations sharing the same line-state owner now also accept standalone
case-insensitive `inherit|initial|unset|revert`: inherited forms use the
computed parent, `initial` uses finite `none` at the root, and `revert` uses
the current one-author-origin parent fallback; `revert-layer` remains the
named-layer rollback and mixed/unsupported forms remain bounded. The same
line-state owner is bounded inherited
`text-decoration-style:solid|dashed|dotted|double|wavy` presentation, where
`double` paints two thickness-preserving solid bands separated by one pixel and
`wavy` repeats a fixed eight-pixel phase `[0,1,2,1,0,-1,-2,-1]`,
and the inherited owner accepts standalone case-insensitive `inherit`,
`initial`, `unset`, and one-author-origin `revert`: inherited forms use the
computed parent, `initial` uses the finite `solid` root default, and `revert`
uses the current one-author-origin parent fallback; `revert-layer` remains the
named-layer rollback and mixed reset tokens remain unsupported,
bounded inherited
`text-decoration-thickness:1px|2px|3px|4px` positive-y bands with
thickness-scaled integer dash/dot periods, and the inherited thickness owner
accepts standalone case-insensitive `inherit`, `initial`, `unset`, and
one-author-origin `revert`: inherited forms use the computed parent, `initial`
uses the finite `1px` root default, and `revert-layer` remains the named-layer
rollback, plus inherited
`text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for
underline and overline while preserving line-through. The inherited owner also
accepts standalone case-insensitive `inherit|initial|unset|revert`: inherited
forms use the computed parent, `initial` uses finite `auto` at the root, and
`revert` uses the current one-author-origin parent fallback; `revert-layer`
remains the named-layer rollback and mixed/unsupported forms remain bounded;
bounded inherited
`text-decoration-skip-spaces:none|all` same-run ASCII-space interval skipping
including adjacent letter, word, and final-line justification spacing across
underline, overline, and line-through, and bounded inherited
signed fixed-pixel
`text-underline-offset:-4px..=4px` that moves only the underline toward
decreasing or increasing y, whose inherited owner also accepts standalone
case-insensitive `inherit|initial|unset|revert`: inherited forms use the
computed parent, `initial` uses finite `0px` at the root, and `revert` uses the
current one-author-origin parent fallback; `revert-layer` remains the
named-layer rollback and mixed/unsupported forms remain bounded,
bounded inherited ASCII `text-transform:none|uppercase|lowercase` layout,
bounded non-negative fixed-pixel first-line `text-indent` for block flow,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `text-indent` with a finite `0px` fallback,
bounded inherited non-negative fixed-pixel `word-spacing` across collapsed and
supported preformatted ASCII spaces, bounded inherited non-negative fixed-pixel
`letter-spacing` after every rendered fixed-cell character in each emitted
fragment, composed with word spacing,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `word-spacing` and `letter-spacing` with finite parent/root fallbacks,
bounded inherited `font-weight:normal|bold|400|700` fixed-cell raster
presentation with unchanged advances and clipped one-pixel bold dilation,
bounded inherited `font-style:normal|italic` fixed-cell raster presentation
with unchanged advances and clipped row-dependent italic shear,
bounded inherited `word-break:normal|break-all` collapsed fixed-cell wrapping,
bounded local `text-overflow:clip|ellipsis` on eligible clipped single-line
direct text,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `text-overflow` with a finite `clip` fallback,
plus standalone case-insensitive `initial`, `unset`, and one-author-origin
`revert` reset forms for both local owners: `text-indent` uses finite `0px`
and `text-overflow` uses `clip`; `revert-layer` remains named-layer rollback,
while standalone case-insensitive `text-indent:inherit` copies the computed
parent only when explicitly authored and omitted `text-indent` remains local;
standalone `text-overflow:inherit` likewise copies the computed parent
`clip|ellipsis` value only when explicitly authored and omitted
`text-overflow` remains local; negative/fractional or percentage indentation
and browser-wide overflow conformance remain outside the boundary,
standalone case-insensitive `overflow:inherit`, `overflow-x:inherit`, and
`overflow-y:inherit` likewise copy the parent's effective bounded clip/no-clip
axis projections only when explicitly authored while omitted overflow remains
local with the visible/no-clip fallback; hidden-versus-clip spelling, nested
scrolling, scrollbars, and browser-wide overflow conformance remain outside the
boundary,
standalone case-insensitive `overflow:initial`, `overflow:unset`, and
one-author-origin `overflow:revert` reset each affected axis to the same
visible/no-clip fallback while remaining distinct from explicit `inherit` and
named-layer `revert-layer`; mixed token forms remain invalid,
standalone case-insensitive finite `overflow:visible|auto|scroll`,
`overflow-x:visible|auto|scroll`, and `overflow-y:visible|auto|scroll` values
share the existing visible/no-clip projection without nested scroll containers
or scrollbar artifacts; mixed forms remain invalid and supported keywords do
not emit overflow diagnostics,
bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
fixed-cell inline and inline-block line items within the existing line box,
plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
rollback for `vertical-align` with finite parent/root fallbacks,
bounded block-level `display:flex` single-row placement for eligible direct
element children with fixed widths and margins, with normal-flow fallback for
unsupported child shapes,
one non-negative fixed-pixel `gap` between visible items in eligible flex rows,
bounded `justify-content:flex-start|center|flex-end|space-between` free-space
placement for eligible fixed-width flex rows,
standalone case-insensitive `justify-content:inherit` copies the computed
parent only when explicitly authored while omitted `justify-content` remains
non-inherited,
bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
source-order ties,
bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024`
allocation plus `flex-basis:auto|Npx` base-size selection, including
case-insensitive 15-layer/unlayered `revert-layer` rollback for the components
and standalone `flex:revert-layer`, `flex-flow:revert-layer`, and
`place-content:revert-layer` shorthand rollback with native fallbacks and
finite shorthand expansion, plus standalone case-insensitive
`initial|unset|revert` reset forms for `flex`, `flex-grow`, `flex-shrink`, and
`flex-basis` that resolve through existing components to the finite `0 1 auto`
initial tuple; standalone `initial|unset|revert` reset forms for
`flex-direction`, `flex-wrap`, and `flex-flow` resolve through existing
components to finite `row`/`nowrap` initial defaults; standalone
`initial|unset|revert` reset forms for local flex-item `order` resolve through
the existing local resolver to finite `0`; standalone
`initial|unset|revert` reset forms for local `justify-content` reuse the
existing bounded `flex-start` fallback; standalone
`initial|unset|revert` reset forms for local `align-items` reuse the existing
 bounded `flex-start` fallback; standalone case-insensitive
 `align-items:inherit` copies the computed parent only when explicitly authored
 while omitted `align-items` remains non-inherited; `inherit`, percentages,
negative/fractional/intrinsic basis values; standalone
`initial|unset|revert` reset forms for local `align-self` reuse the existing
 bounded `auto` fallback and continue through parent `align-items`; standalone
 case-insensitive `align-self:inherit` copies the computed parent only when
 explicitly authored while omitted `align-self` remains local `auto`;
standalone `initial|unset|revert` reset forms for local `align-content` reuse
the existing bounded `flex-start` fallback; standalone case-insensitive
`align-content:inherit` copies the computed parent only when explicitly
authored, while omitted `align-content` remains non-inherited; standalone
`initial|unset|revert` reset forms for local `place-content` project through
the existing bounded `flex-start` fallbacks for both `align-content` and
`justify-content`; standalone case-insensitive `place-content:inherit` copies
both computed parent components only when explicitly authored while omitted
`place-content` remains local; mixed inherit/finite and reset/finite tokens
remain bounded diagnostics, plus
bounded case-insensitive gap-family
`revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with
independent finite-pixel components, plus standalone case-insensitive
`initial|unset|revert` reset forms for all three gap declarations that resolve
to zero; standalone case-insensitive `gap:inherit`, `row-gap:inherit`, and
`column-gap:inherit` copy their corresponding computed parent gap components
only when explicitly authored while omitted forms remain local with the
bounded `0` fallback; percentages and fractional/intrinsic values remain
bounded diagnostics,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `text-transform`, `font-weight`, `font-style`, and `word-break` with finite
parent/root fallbacks,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `white-space`, positive-pixel
`line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
`vertical-align`, `word-spacing`, and `letter-spacing`, with the existing
computed-style, layout, display-list, raster, PNG, point-hit, semantic, and
diagnostic owners reused unchanged,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `text-align`, `text-align-last`,
`text-justify`, and `direction`, with left/auto/auto/`ltr` root defaults,
parent fallback for inherited forms, the existing `revert-layer` distinction,
and logical `ltr`/`rtl` projection preserved through the same owners,
bounded case-insensitive 15-layer/unlayered rollback for local
`flex-wrap`, `justify-content`, `align-items`, `align-self`, `align-content`,
and `flex-direction` owners with their native fallbacks,
standalone case-insensitive `flex-direction:inherit` copies the computed
parent direction only when explicitly authored while omitted `flex-direction`
remains local with the bounded `row` fallback,
standalone case-insensitive `flex-wrap:inherit` copies the computed parent wrap
mode only when explicitly authored while omitted `flex-wrap` remains local with
the bounded `nowrap` fallback,
standalone case-insensitive `flex-flow:inherit` copies both computed parent
direction and wrap components only when explicitly authored while omitted
`flex-flow` remains local with bounded `row`/`nowrap` fallbacks,
standalone case-insensitive `flex:inherit` copies the computed parent grow,
shrink, and basis components only when explicitly authored while omitted
`flex` remains local with bounded `0 1 auto` fallbacks,
standalone case-insensitive `flex-grow:inherit` copies the computed parent
grow component only when explicitly authored while omitted `flex-grow` remains
local with the bounded `0` fallback,
standalone case-insensitive `flex-shrink:inherit` copies the computed parent
shrink component only when explicitly authored while omitted `flex-shrink`
remains local with the bounded `1` fallback,
standalone case-insensitive `flex-basis:inherit` copies the computed parent
basis component only when explicitly authored while omitted `flex-basis` remains
local with the bounded `auto` fallback,
standalone case-insensitive `order:inherit` copies the computed parent order
only when explicitly authored while omitted `order` remains local with the
bounded `0` fallback; visual `(order, source_index)` sorting preserves
semantic/source order,
standalone case-insensitive `gap:inherit`, `row-gap:inherit`, and
`column-gap:inherit` copy their corresponding computed parent gap components
only when explicitly authored while omitted `gap`, `row-gap`, and
`column-gap` remain local with the bounded `0` fallbacks; shorthand/longhand
precedence remains bounded,
bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
placement using explicit content height or the auto row's maximum item outer
height,
bounded non-inherited `flex-direction:row|row-reverse` physical placement of
the order-sorted visual sequence with item-attached margins, existing
gap/justification/alignment, shared subtree artifacts, bounded overflow
translation, root horizontal scrolling, and unchanged semantic/source order,
deterministic display list, bounded logical RGBA software surface, and bounded
capture through the native backend directly. Native capture currently supports
PNG, JPEG, WebP, and PDF of the logical page surface; generic
screenshot-containing evidence and physical-pixel capture remain outside this
contract. The runtime also exposes bounded script evaluation, storage and
cookie operations, dialog resolution, and download completion. See the
[native operation profile](experimental-capabilities.md#native-browser-engine)
for current limits and evidence. The `native-engine` feature is enabled by
default and backs the canonical `BrowserSession` directly. Native action
targets are semantic, with the bounded
`point=x,y` click extension; non-text fragment matching is exact and
case-sensitive after bounded UTF-8 percent decoding for visible non-empty `id`
attributes or the unique legacy `<a name>` fallback, while simple text
fragments and their exact adjacent prefix/suffix affixes match only within the
first visible non-truncated layout run.
Unresolved ID/name targets preserve the current offset, as do missing, hidden,
malformed, and unsupported text-fragment requests. Link activation supports
fragment-only, registered fixture-relative, and validated external HTTP(S)
hrefs. Structured evidence omits raw form values by default.

## Session ownership

`BrowserSession::start` and `start_default` create an independent Glass-owned
native runtime directly. Call `close(self)` to end that runtime and release its
owned work; dropping is not a substitute for an explicit lifecycle boundary.
The native constructor never launches or attaches to Chrome. Firefox and
Safari connections are explicit externally managed endpoints through
`BrowserRuntimeSession::connect`. The separate `CdpBrowserSession` type owns a
Chrome process and profile only when its `SessionOptions` launch one; attach
mode controls an existing browser without owning its process. These session
types do not silently switch backends or share an implicit browser context.

## Explicit Chromium/CDP migration session

`CdpBrowserSession` is one owned or attached Chrome/Chromium control session. Methods take
`&self` because operation serialization and mutable browser state are internal.
The session owns target/frame selection, revision counters, bounded caches,
policy interception, presentation state, and an optional Chrome child.

```rust,no_run
use glass_browser::{CdpBrowserSession, SessionOptions};

# async fn run() -> glass_browser::BrowserResult<()> {
let options = SessionOptions::builder()
    .incognito(true)
    .headed(false)
    .port(9222)
    .build()?;
let session = CdpBrowserSession::start(&options).await?;
let page = session.navigate("https://example.com").await?;
println!("{}", page.url);
session.close().await?;
# Ok(())
# }
```

Call `close` for owned sessions. It sends `Browser.close` before process
fallback so named-profile state can flush. Dropping is a fallback, not the
preferred persistence boundary. An attached session never owns or closes the
external Chrome process.

`SessionOptions::validate` rejects incompatible attach settings before
connection. An explicit target ID is required when the endpoint exposes
multiple page targets.

## Structured observation and guarded actions

`BrowserSession::observe()` returns the bounded structured native semantic
observation. Use `semantic_observe(level)` for an explicit projection, plus
`inspect_page()` and `observe_bootstrap()` for their corresponding revisioned
inspection envelopes. Region expansion is revision-checked. The explicit
`CdpBrowserSession` migration API retains its separate compact and semantic
observation methods. These semantic APIs return a typed unsupported-operation
error for an externally connected non-native runtime; they never switch to
CDP.

```rust,no_run
use glass_browser::browser::session::SemanticObservationLevel;
use glass_browser::browser_backend::SemanticAction;
use glass_browser::BrowserSession;

# async fn run() -> glass_browser::BrowserResult<()> {
let session = BrowserSession::start_default().await?;
session
    .navigate("data:text/html,%3Cmain%3E%3Cbutton%3ESave%3C%2Fbutton%3E%3C%2Fmain%3E")
    .await?;
let structured = session.observe().await?;
println!("structured revision={}", structured.revision);
let semantic = session
    .semantic_observe(SemanticObservationLevel::Interactive)
    .await?;
println!("revision={} regions={}", semantic.revision, semantic.regions.len());

let target = semantic
    .regions
    .iter()
    .flat_map(|region| region.targets.iter())
    .find(|target| target.role == "button")
    .expect("the fixture contains one button");
session
    .action_with_revision(
        SemanticAction::Click {
            target: target.reference.clone(),
        },
        semantic.revision,
    )
    .await?;
session.close().await
# }
```

Do not retain a revisioned browser reference across navigation or unverified
page drift. Re-observe and resolve again. Unique resolution, actionable state,
policy, and expected revision are preconditions; failure occurs before input
when those preconditions cannot be proven.

## Target and frame topology

The canonical native `BrowserSession` lists page targets and the selected
target's frames through `list_targets()` and `list_frames()`. `create_target()`
opens a parked page without changing selection; use `select_target()` to route
subsequent operations, and `close_target()` for explicit cleanup. Use
`select_frame()` only with an ID returned by `list_frames()`. These methods
operate on the native target/frame registry. A Firefox or Safari endpoint
session returns a typed unsupported-operation error; it never switches to
another transport. Existing `native_*` method spellings remain available for
source compatibility.

## Navigation history and recovery

The canonical native `BrowserSession` exposes `go_back()` and `go_forward()`
for direct traversal, plus `go_back_with_revision(revision)` and
`go_forward_with_revision(revision)` for observation-guarded traversal.
`reload_with_revision(revision)` reloads the selected entry only when the
observation is current. `recover()` rebuilds the native document owner and
reloads its current URL; `recover_with_revision(revision)` first rejects a
stale observation. Each operation returns `NavigationControlOutcome` with its
action and previous/current revisions. Firefox and Safari sessions return a
typed unsupported-operation error rather than switching transports.

`stop_loading_with_revision(revision)` is a standard native `BrowserSession`
control. During process-backed HTTP(S) navigation it requests cancellation
only when the supplied revision matches the navigation's starting revision
and returns once that request is accepted; if commit already won the race, the
method returns a typed lifecycle error rather than claiming cancellation.
Keep polling the in-flight `navigate` future until it returns: the partial
response is discarded and its content worker is reaped before then. The prior
committed document, history, and revision remain unchanged, but transient
state held only by that worker is not preserved. When idle, the method
validates the current revision and returns it unchanged. Firefox and Safari
reject the native-only control with typed unsupported errors. The separate
persistent-owner socket currently reports this control as unsupported because
its serialized request loop cannot interrupt a running command; that owner
path remains issue #40 work.

## Evidence extraction and Web IR

`ExtractionRequest` is strict and non-mutating. The caller selects sources and
hard budgets. `extract_evidence` returns source-labelled facts;
`extract_web_ir` additionally reconciles them into stable Glass Web IR v1.

```rust,no_run
use glass_browser::{
    CdpBrowserSession, EvidenceSource, ExtractionBudgets, ExtractionRequest,
    ExtractionScope, SessionOptions, EXTRACTION_CONTRACT_SCHEMA_VERSION,
};

# async fn run() -> glass_browser::BrowserResult<()> {
let session = CdpBrowserSession::start(&SessionOptions::builder().build()?).await?;
let request = ExtractionRequest {
    schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
    scope: ExtractionScope::Document,
    sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
    budgets: ExtractionBudgets::default(),
};
let ir = session.extract_web_ir(&request).await?;
println!("revision={} entities={}", ir.revision, ir.entities.len());
session.close().await
# }
```

Requesting `Dom` is an explicit deep-inspection choice. Forms evidence may
contain sensitive state and is policy-gated. Returned evidence declares
missing sources, truncation, coverage, and opaque regions instead of inventing
certainty.

Offline callers can use `reconcile_evidence`, `GlassWebIrV1::validate`,
`GlassWebIrV1::diff`, and `classify_entity_continuity`. Validation does not
turn an offline graph into browser authority.

## Task Protocol and deterministic compilation

Authored tasks contain semantic scope, named inputs, budgets, risk, ambiguity,
revision policy, and postconditions. They never contain selectors or CDP
handles. Compilation consumes validated Web IR and emits a value-free plan.

```rust,no_run
use glass_browser::{compile_task, GlassTask, GlassWebIrV1};

fn compile(task_json: &str, ir_json: &str) -> Result<String, Box<dyn std::error::Error>> {
    let task = GlassTask::from_json(task_json)?;
    let ir: GlassWebIrV1 = serde_json::from_str(ir_json)?;
    ir.validate()?;
    let plan = compile_task(&task, &ir)?;
    Ok(plan.to_canonical_json()?)
}
```

`compile_task_with_knowledge` and `compile_task_with_options` may attach
advisory memory provenance. Executable entity selection, preconditions, and
postconditions still derive only from current IR.

For live execution, the complete migration API currently exposes
`CdpBrowserSession::execute_task`. The canonical native `BrowserSession`
continues to gain the operation parity required by issue #40. The CDP method validates,
extracts current evidence, compiles, binds semantic keys to exactly one current
revisioned target, enforces confirmation/lease rules, performs the operation,
and verifies postconditions. Receipts exclude authored values and live browser
references.

## Intent, workflows, and recovery

The session intent API separates normalization, candidate resolution,
selection, and execution. `resolve_intent` is browser-free when supplied
candidate evidence; `executeIntent`-equivalent session methods re-observe and
re-resolve before action.

Workflow types define typed inputs, budgets, steps, conditions, outputs,
checkpoints, terminal proof, and traces. Authoring helpers compile strict YAML
or JSON and report source locations without echoing sensitive input.

After a possibly dispatched mutation fails, inspect `ActionOutcome`,
`TaskExecutionResult`, or `WorkflowRunResult`. An `indeterminate` result means
mutation may have occurred. Use `recover_run`, current observation, checkpoint,
or returned retry classification; do not replay the mutation blindly.

## Knowledge

`KnowledgeStore` is a bounded, crash-safe local snapshot. Build lookup context
from fresh observation plus explicit profile/workspace/backend/surface inputs.
Assessment and retrieval are read-only. Learning requires successful,
non-private, non-truncated, non-sensitive evidence and matching scope
provenance.

Knowledge records never retain executable target references. Historical
fingerprints can explain or rank current candidates but cannot authorize a
mutation.

The focused `glass-browser` crate does not export the Development Runtime;
project files, PTYs, LSP, Pi, and Neovim ownership remain in `glass-dev`.


## Development Runtime

Depend on `glass-dev` to embed project tooling:

```toml
glass-dev = "0.3"
```

```rust,no_run
use glass_dev::development::ProjectWorkspace;

fn inspect(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = ProjectWorkspace::open(root)?;
    println!("{:?}", workspace.detection());
    for entry in workspace.list_files()? {
        println!("{}", entry.path);
    }
    Ok(())
}
```

`ProjectWorkspace` owns canonical-root confinement, native buffers, event
timeline, graph, replay, and process manager. File and process limits are
enforced before retention. Mutations should carry an `Actor`; external-agent
edits and links remain attributable.

`AgentToolGateway` validates a fixed descriptor catalog, call envelope, JSON
schema, authorization, and confirmation. Audit events store argument byte
count/digest and result metadata, not argument values.

### Managed Pi boundary

`glass_dev::pi_runtime` exposes `PiReadiness`, `PiReadinessComponent`,
`PiReadinessState`, `PiSessionRequest`, and `PINNED_PI_SDK_VERSION`
(`0.84.4`). `PiReadiness` checks Node (currently 22.19.0 or newer), the
managed SDK, authentication, provider, and session state. `PiSessionRequest`
is the native resident-session protocol used by the development agent runtime;
it is distinct from the CLI's legacy one-shot `PiHarness` RPC adapter.

`AgentTurnMode` is the per-turn composer personality: Ask, Plan, or Agent
(default Agent). Ask and Plan are fail-closed for mutations. `SessionTodo`,
`SessionTodoList`, and `TodoStatus` are the workspace-local Agent checklist
persisted at `.glass/todos/session.json`. They are not the overnight task DAG.

## Backends, surfaces, and presentation

- `browser_backend` defines semantic requests, responses, profiles,
  capabilities, errors, and `BrowserBackendDispatcher`. Call through the
  dispatcher; it validates capability dependency closure and results.
- `BackendFactory` selects an explicit or best certified backend without
  iteration-order dependence. An explicit unusable backend fails; it does not
  silently fall back.
- `surfaces` models nested browser-hosted boundaries, evidence, coverage,
  provenance, bridge grants, and action requirements. Detection is never an
  input grant.
- `presentation` owns frame metadata, geometry/revision mapping, latest-frame
  mailbox, payload ownership events, and metrics. It does not own browser or
  terminal transports.
- `terminal_graphics` provides Herdr-owned pane, Kitty protocol, ANSI canvas,
  and semantic render adapters. The development TUI's live policy selects
  `Herdr`, explicit `Kitty`, or bounded `Ansi`; `Auto` may remain
  semantic-only when no native backend is detected.
- Live quality is bounded to data (~3 FPS), balanced (~6 FPS), or smooth (~12
  FPS). ANSI fit supports `contain`, `cover`, and `actual`; native image paths
  use `contain`. These are CLI/TUI policy inputs, not browser-session
  ownership.


## Protocol, MCP, daemon, and results

`protocol` contains canonical Glass request/response envelopes. MCP adds
JSON-RPC framing but maps tools into those payloads. `mcp::serve` owns stdio;
stdout must contain only protocol frames.

`daemon` provides local Unix-socket lifecycle, isolated MCP children,
mutation leases, bounded status/logs, and explicit interrupted-run recovery.
It is not a network service.

`ExperienceResult` and `OperationResult` project bounded minimal, normal, or
diagnostic output. Large diagnostic detail belongs in `ResultStore`; callers
receive a local result ID rather than an unbounded transport payload.

## Public module map

Module paths are package-qualified below because both packages expose a
`browser` and a `tui` module. This is the checked-in public Rust surface; the
generated rustdoc pages and source exports are the authority for individual
items.

### `glass-browser` (`glass_browser`)

| Module | Contract |
|---|---|
| `browser` | Chrome/CDP lifecycle, session API, policies, profiles, actions, observations, workflows, and advisory knowledge |
| `browser_backend` | Transport-neutral semantic backend contract and dispatcher |
| `browser_workspace` | Bounded revision-safe browser UI state, actions, focus, ownership, layout, and presentation contracts |
| `capabilities` | Versioned discovery and negotiation manifest |
| `cli` | Clap argument definitions and browser command dispatch |
| `connection` | Independent connection environment, presentation profiles, policy reasons, and observatory metrics |
| `daemon` | Local socket, isolated MCP sessions, leases, and recovery |
| `extensions` | Manifest, permission, registry, sandbox, and guarded-action boundary |
| `extraction` | Strict source-labelled evidence extraction |
| `mcp` | JSON-RPC/MCP stdio server, prompts, resources, and tool dispatch |
| `presentation` | Browser-neutral frame metadata, geometry, ownership, mailbox, and metrics |
| `protocol` | Canonical versioned operations and responses |
| `reliability` | Browser-free scenario, fixture, replay, scorecard, and gate contracts |
| `reliability_runner` | Bounded browser execution for reliability scenarios |
| `results` | Agent-facing projections and local diagnostic artifacts |
| `surfaces` | Bounded multi-surface understanding and bridge grants |
| `task_compiler` | Deterministic Task Protocol to execution-plan compiler |
| `task_protocol` | Strict authored semantic task contract |
| `terminal_graphics` | Herdr, Kitty, ANSI, and semantic terminal render adapters |
| `tui` | Standalone browser TUI reducer, responsive layouts, semantic selection, and bounded live presentation |
| `web_ir` | Stable Web IR reconciliation, validation, diff, and continuity |
| `workspace` | Workspace identity, ownership, attachments, lifecycle, and persistence |

`update` is an implementation module and is not public. The browser crate does
not export the Development Runtime; project files, native editors, PTYs, LSP,
Pi, Neovim, Git, and collaboration belong to `glass-dev`.

### `glass-dev` (`glass_dev`)

| Module | Contract |
|---|---|
| `agents` | Resident Pi scheduling, lifecycle, evidence, and approval state |
| `browser` | Development-browser service configuration, state, and worker handle |
| `cli` | `glass` command dispatch for development routes |
| `customization` | User/project configuration, skills, and custom commands |
| `daemon` | Resident development daemon integration |
| `debugger` | Debugger and semantic breakpoint support |
| `development` | Project files, buffers, editors, PTYs, LSP, graph, replay, and collaboration |
| `experiments` | Isolated Git worktree experiments and comparison |
| `external_agents` | One-shot adapters for installed external coding agents |
| `git` | Governed Git workspace operations |
| `github` | GitHub status, review, and pull-request shipping operations |
| `harness` | Discovery and safe launch of installed coding harnesses |
| `intelligence` | Development graph and causal intelligence projections |
| `kernels` | Kernel process and runtime integration |
| `lsp` | LSP-facing language-service integration |
| `mcp` | Development MCP server and tool integration |
| `pi_runtime` | Managed Pi runtime readiness and sessions |
| `tasks` | Task scheduling, retry, evidence, and verification requirements |
| `testing` | Test execution and result collection |
| `todos` | Workspace-local Agent checklist persisted at `.glass/todos/session.json` (`glass.todo.*`); not the overnight DAG |
| `fim` | Configured fill-in-the-middle provider for editor ghosts |
| `tools` | Governed development-tool routing |
| `trust` | Workspace trust decisions and persistence |
| `tui` | Resident development terminal application and surface routing |
| `workspace` | Workspace ownership, shared handles, and per-turn Ask/Plan/Agent mode |

## Errors and privacy

Browser methods return `BrowserResult<T>`. Stable higher-level contracts use
typed errors such as `ActionContractError`, `TaskProtocolError`,
`TaskCompilationError`, `WebIrValidationError`, `KnowledgeStoreError`, and
backend/workspace errors. Do not parse display strings when a typed result is
available.

Treat DOM, screenshots, PDFs, cookies, storage, evaluated values, profiles,
and diagnostic logs as sensitive. Do not write them to tracing or durable
artifacts unless the caller explicitly selected that evidence path.

## Examples and validation

See [runnable examples](examples.md). Library documentation is validated with:

```console
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
cargo test --workspace --doc --all-features --locked
```
