# Glass Browser Host RFC

## Status and scope

This RFC defines the host boundary for Pillar III backend survivability. A
Browser Host owns transport startup, command serialization, lifecycle, and
bounded endpoint discovery. The host exposes only the transport-neutral
`BrowserBackend` contract from `src/browser_backend.rs`; CDP and WebDriver BiDi
wire types remain below their adapters.

The deterministic `semantic-proof` backend is a conformance backend. It is
useful for protocol tests, but it is not browser parity and MUST NOT be
reported as a real browser.

## Registration and selection

A host registers typed `BackendStartup` candidates and calls
`BackendFactory::start` with a validated `BackendSelectionRequest`. Selection
is deterministic:

1. an explicit backend preference is strict and never silently falls back;
2. automatic selection orders certification and capability coverage;
3. backend id is the stable final tie-breaker.

The returned `StartedBackend` carries both the selected machine-readable
profile and the owned adapter. Every dispatch is capability-gated by
`BrowserBackendDispatcher`; an omitted or disabled capability returns the
stable `CapabilityUnavailable` error.

## BiDi startup and command envelope

`BidiBrowserBackend::connect_with_config` accepts a `ws://` or `wss://` endpoint.
For an `http://` or `https://` endpoint it performs bounded discovery and
requires a `webSocketUrl` (case-compatible `websocketUrl` is accepted). The
WebSocket command envelope is `{id, method, params}` and responses are matched
by id. Events, ping/pong frames, payload size, message count, and command time
are bounded; malformed or mismatched responses fail closed as typed connection
errors.

The certified BiDi slice is intentionally small:

- `session.new` and `session.end` lifecycle;
- `browsingContext.getTree` contexts;
- `browsingContext.navigate` navigation;
- `script.evaluate` for bounded script and evidence extraction;
- bounded DOM click/type action translation;
- revision-based effects and verification through evidence.

Capture, storage, prompts, downloads, key presses, and scrolling remain
unavailable until a capability declaration and deterministic conformance test
exist. A disabled script capability also disables evidence and action.

## Survivability and authority

One serialized command stream is retained per backend. The adapter retains only
current URL, active context, and a monotonic revision; it does not persist
page payloads by default. Transport reconnection is not inferred: after a
closed stream, lifecycle and command calls fail closed rather than replaying a
mutation. The current Web IR, revision, policy, and capability evidence remain
executable authority; backend profiles are declarations, not permission to
bypass those checks.

## Current browser runtime mapping

The full `BrowserSession` remains the Chromium/CDP production path. The public
`BrowserRuntimeSession` adds a deliberately smaller portable path:

| Runtime | Transport | Status | Startup |
|---|---|---|---|
| Chromium | Chrome DevTools Protocol | Production full session | Glass launches or explicitly attaches |
| Firefox | WebDriver BiDi WebSocket | Experimental portable semantics | User starts Firefox with `--remote-debugging-port` and supplies `--browser-endpoint` |
| Safari | W3C WebDriver HTTP through `safaridriver` | Experimental portable semantics | User starts `safaridriver` and supplies its base URL |
| Native | Glass-owned in-process Rust engine | Experimental local semantics; feature-gated | Explicit `native-engine` build; `NativeEngineConfig` or the local CLI path; no endpoint |

The external portable command set is navigation, one active context, compact
script-derived evidence, script evaluation, CSS click/type actions, and
revision effects. The native command set is local navigation, one active
context, bounded URL/title/visible-text evidence, semantic click/type actions,
and bounded native point hit testing plus revision effects; it does not execute
script. Screenshots, storage,
prompts, downloads, keyboard, scrolling, multi-window control, profiles, MCP,
TUI, and the full locator/Web IR pipeline remain capability-denied on these
adapters.

Firefox is configured as a browser-specific BiDi profile so selection can
require `browserFamily=firefox`; Safari is intentionally represented by the
classic WebDriver adapter because SafariDriver is not currently a certified
direct BiDi endpoint in this codebase.

Protocol references:

- [W3C WebDriver BiDi](https://www.w3.org/TR/webdriver-bidi/)
- [MDN: create a WebDriver BiDi connection](https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection)
- [WebKit: WebDriver is coming to Safari](https://webkit.org/blog/9395/webdriver-is-coming-to-safari-in-ios-13/)

## Native browser feasibility

The native-engine program now has real Phase 2 semantic interaction and
initial Phase 3 presentation/layout/display-list/software-surface slices
behind the default-off `native-engine` feature. It owns one deterministic
in-process context, local `about:blank`, `data:text/html`, and registered
`fixture://` resources, a small DOM/text projection, history, revisions,
bounded CSS presentation, integer normal-flow rectangles, point hit testing,
semantic click/type actions for local controls, a bounded effects signal,
bounded inherited text color, `overflow:hidden` clips shared by paint, viewport
projection, and point hit-testing, bounded side-specific
solid/dashed/dotted/double/groove/ridge/inset/outset-border paint,
bounded physical circular border radii, bounded inline-box line placement,
bounded fixed pixel line-height flow, bounded direct-text flow fragments and
source-order text paint, bounded word-aware wrapping,
bounded source-whitespace boundaries across supported inline flow,
bounded outer/content box geometry with physical four-side padding/margin
shorthands and longhands plus explicit box sizing and bounded physical min/max
width/height constraints with bounded case-insensitive 15-layer/unlayered local
`revert-layer` rollback for finite non-negative integer-pixel `width`, `height`,
`min-width`, `max-width`, `min-height`, and `max-height` with absent local
fallbacks, plus bounded case-insensitive 15-layer/unlayered local
`revert-layer` rollback for `box-sizing`, physical padding and margin edges,
including bounded `margin:auto`, with content-box/zero local fallbacks,
bounded vertical viewport
scrolling, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip`
clips through the existing projection and point-hit owner, plus bounded
case-insensitive 15-layer/unlayered local `revert-layer` rollback for
`overflow`, `overflow-x`, and `overflow-y`, preserving independent visible/no-clip
fallbacks and the existing paint, projection, point-hit, root-overflow, capture,
and semantic/source-order owners; nested scrolling, scrollbars, and browser-wide
overflow semantics remain outside the boundary, and bounded PNG capture through
the explicit backend operation. The same bounded local cascade path also accepts
standalone, case-insensitive 15-layer/unlayered `border-radius:revert-layer`
for the one-to-four-value integer shorthand, preserving the zero-corner
fallback and the existing rounded layout, fill, border, point-hit, capture,
raster, overflow, and semantic/source-order owners; elliptical, percentage,
nested-clip, anti-aliasing, multiple-origin, and browser-wide
border-radius conformance remain outside the boundary. The same bounded radius
family honors a terminal case-insensitive `!important` marker through the
existing author-origin cascade: important candidates outrank normal candidates
and named-layer priority reverses within the important partition. The bounded
`background-color`, inherited `color`, and `text-decoration-color` owners
also honor the same terminal priority. The standalone physical `border-color`
shorthand and physical color longhands also honor it, as do the six supported
horizontal-tb logical `border-block-color`, `border-block-start-color`,
`border-block-end-color`, `border-inline-color`, `border-inline-start-color`,
and `border-inline-end-color` declarations through their existing `ltr`/`rtl`
physical-side projection. The supported text-presentation set (`white-space`,
`text-align`, `text-align-last`, `text-justify`, `direction`, text-decoration
line/style/skip-ink/skip-spaces/thickness/underline-offset, `text-transform`,
`font-weight`, `font-style`, `word-break`, `text-overflow`, `vertical-align`,
`text-indent`, `word-spacing`, `letter-spacing`, and `line-height`) also honors
the same terminal priority; local `display`, `visibility`, and `opacity`
declarations honor the same terminal priority; the normal-only flex and gap
declarations (`justify-content`, `align-items`, `align-self`, `align-content`,
`place-content`, `flex-direction`, `flex-wrap`, `flex-flow`, `order`,
`flex-grow`, `flex-shrink`, `flex-basis`, `flex`, `gap`, `row-gap`, and
`column-gap`) honor it through their existing shorthand/component owners;
the six normal-only dimension declarations (`width`, `height`, `min-width`,
`max-width`, `min-height`, and `max-height`) also honor it through private
doubled candidate streams with important-over-normal ordering, reversed
named-layer priority, inline precedence, invalid-later preservation, and
`revert-layer` rollback; the normal-only physical box-model declarations
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
logical properties, and remaining properties retain
their existing bounded behavior.
owner also accepts exact case-insensitive CSS-wide `inherit`, `unset`,
`initial`, and one-author-origin `revert`: explicit `inherit` copies the
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
elliptical, and percentage corner values remain unsupported.
The same bounded local
cascade path also accepts standalone, case-insensitive 15-layer/unlayered
`opacity:revert-layer` for the existing local 8-bit opacity owner, preserving
the full-opacity (`255`) fallback, reduced-opacity group markers and software
compositing, and unchanged layout, point-hit, capture, raster, overflow, and
semantic/source-order owners; inherited opacity, stacking-context/blending
parity, filters, animation, multiple origins, and browser-wide opacity
conformance remain outside the boundary.
The same bounded local cascade path also accepts standalone, case-insensitive
15-layer/unlayered `revert-layer` for the existing local `display` and
`visibility` owners. It resolves through lower concrete candidates or the
normal-flow `display:auto` and visible fallbacks, preserving `display:none`,
`visibility:hidden`, and `display:contents` behavior across hidden-subtree
layout, point hit testing, display-list, capture, raster, and
semantic/source-order owners. Inherited visibility, display decomposition,
formatting-context parity, table/ruby/flow-root details, animation, multiple
origins, and browser-wide CSS display/visibility conformance remain outside
the boundary.
The native border surface also accepts bounded case-insensitive 15-layer/
unlayered local `revert-layer` for the physical `border`, `border-top`,
`border-right`, `border-bottom`, and `border-left` owners. Rollback resolves
each side through lower concrete candidates or the existing zero-width/no-paint
fallback, preserving shorthand/longhand precedence and the existing box-model
inset, border display-list, capture, raster, point-hit, and semantic/source-order
owners. Vertical writing modes, text orientation, elliptical/percentage logical
radii, border-image, gradients, `wavy`
and other unsupported border styles, animation, multiple origins, and browser-wide CSS border
conformance remain outside the
boundary. The same native border owner also accepts bounded case-insensitive
15-layer/unlayered `border-color`, `border-top-color`, `border-right-color`,
`border-bottom-color`, and `border-left-color` values with one-to-four-value
physical shorthand expansion and independent per-side `revert-layer` rollback
to lower colors or bounded black, preserving border width/style and the
existing box-model and artifact consumers. Standalone physical `border-color`
and physical color longhands also resolve `currentColor` from the element's
local or inherited color; complete physical border shorthands also resolve
`currentColor` from that same local or inherited color. Gradients, border-image,
and browser-wide border conformance remain outside the boundary. The physical
border-color owner also accepts case-insensitive CSS-wide `inherit`, `unset`,
`initial`, and `revert`: explicit `inherit` copies the parent's effective
per-side colors, reset forms resolve to the element's current color, omission
retains the existing black side fallback, a single CSS-wide shorthand token
expands to all four sides, and mixed CSS-wide/color shorthand forms remain
unsupported.
The same native border owner also accepts bounded case-insensitive
15-layer/unlayered `border-width`, `border-top-width`, `border-right-width`,
`border-bottom-width`, and `border-left-width` values with one-to-four-value
physical shorthand expansion and independent per-side `revert-layer` rollback
to lower widths or bounded zero. Width-only declarations do not invent a style
or paint a border. The physical border-width owner also accepts case-insensitive
CSS-wide `inherit`, `unset`, `initial`, and `revert`: explicit `inherit` copies
the parent's effective per-side widths, reset forms resolve to zero, ordinary
omission retains the zero fallback, a single CSS-wide shorthand token expands
to all four sides, and mixed CSS-wide/numeric shorthand forms remain
unsupported. Standalone physical border-width declarations also accept a
terminal case-insensitive `!important` marker: important widths outrank normal
widths, use earliest-named-layer priority inside the bounded author-important
partition, and preserve per-side `revert-layer` and inline behavior; complete
and side-border shorthand priority is described below. The same owner
also accepts bounded case-insensitive
15-layer/unlayered local `border-style` and physical `border-top-style`,
`border-right-style`, `border-bottom-style`, and `border-left-style` values
from the finite
`none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar with
one-to-four-value physical shorthand expansion and independent per-side
`revert-layer` rollback
to lower styles. The style owner also accepts case-insensitive CSS-wide
`inherit`, `unset`, `initial`, and `revert`: explicit `inherit` copies the
parent's effective physical side styles, including private `none`/`hidden` and
styles from unpainted or zero-width parents; reset forms resolve to the private
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
styles and resolves
to no side/zero width before layout and paint; `hidden` remains a private
distinction for future table conflict resolution. Width, style, and color
components compose only after independent resolution, and width-only or
style-only declarations do not invent missing paint components;
collapsed-table border conflict resolution, fractional/percentage widths,
vertical writing modes, text orientation, elliptical/percentage logical radii,
`wavy` and other unsupported styles,
and browser-wide border conformance remain outside the boundary. The native
border surface also accepts bounded horizontal-tb logical `border-block`,
`border-block-start`, `border-block-end`, `border-inline`, `border-inline-start`,
and `border-inline-end` shorthands plus their `-width`, `-style`, and `-color`
component families. One- or two-value block and inline component pairs map to
  logical start/end; block sides map to physical top/bottom, while inline sides
  map to physical left/right for `ltr` and right/left for `rtl` through the
  resolved inherited `direction`. The six supported logical border-width
  declarations also accept a terminal
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
described above. Complete logical shorthands, CSS-wide keywords, `currentColor`,
  bounded named-layer and unlayered `revert-layer`,
physical/logical precedence, and component composition reuse the existing
physical border streams and box-model, display-list, capture, raster, point-hit,
and semantic/source-order owners. Vertical writing modes, text orientation,
elliptical/percentage logical radii,
border-image, gradients, table conflict resolution, multiple origins, and
browser-wide logical-border conformance remain outside the boundary. The
complete and physical border shorthands also
accept the exact case-insensitive omitted-component `none` and `hidden` forms
through private no-paint style sentinels; winning omitted-component `hidden`
remains private for future table conflict resolution and arbitrary
omitted-component forms remain outside this bounded surface. Complete and
physical border shorthands also accept bounded complete `Npx hidden color`
values with private width/color retention for future table conflict resolution;
current non-table composition suppresses hidden paint. Complete and physical
border shorthands also accept bounded complete `Npx none color` values with
private width/color retention for future table conflict resolution; current
non-table composition suppresses none paint. The same complete physical border
shorthands also accept exact case-insensitive CSS-wide `inherit`, `unset`,
`initial`, and one-author-origin `revert` values. Explicit `inherit` copies
effective parent width/style/color sides, including private `none`/`hidden`,
zero-width, and unpainted states; reset forms project zero width, private
`none`, and `currentColor` for later component composition; ordinary omission
remains omission and mixed CSS-wide/concrete forms remain unsupported. Bounded local opacity subtree groups through
transparent software layers,
bounded inherited physical `text-align:left|center|right` line placement, and
bounded functional `rgba(R, G, B, A)` alpha colors for background, border, and
text paint, plus bounded case-insensitive 15-layer/unlayered local
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
named-layer rollback and mixed/unsupported forms remain bounded, and bounded
inherited ASCII
`text-transform:none|uppercase|lowercase` layout, bounded non-negative
fixed-pixel first-line `text-indent` for block flow,
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
bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
source-order ties,
bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024`
allocation plus `flex-basis:auto|Npx` base-size selection, including
case-insensitive 15-layer/unlayered `revert-layer` rollback for the components
and standalone `flex:revert-layer`, `flex-flow:revert-layer`, and
`place-content:revert-layer` shorthand rollback with native fallbacks and
finite shorthand expansion, plus bounded case-insensitive gap-family
`revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with independent
finite-pixel components,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `text-transform`, `font-weight`, `font-style`, and `word-break` with finite
parent/root fallbacks,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `white-space`, positive-pixel
`line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
`vertical-align`, `word-spacing`, and `letter-spacing`, with parent/root
fallbacks and the existing fixed-cell artifact owners preserved,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `text-align`, `text-align-last`,
`text-justify`, and `direction`, with left/auto/auto/`ltr` root defaults,
parent fallback for inherited forms, the existing `revert-layer` distinction,
and logical `ltr`/`rtl` projection preserved through the same artifact owners,
bounded case-insensitive 15-layer/unlayered rollback for non-inherited
`flex-wrap`, `justify-content`, `align-items`, `align-self`, `align-content`,
and `flex-direction` owners with their native fallbacks,
bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
placement using explicit content height or the auto row's maximum item outer
height,
bounded non-inherited `flex-direction:row|row-reverse` physical placement of
the order-sorted visual sequence with item-attached margins, existing
gap/justification/alignment, shared subtree artifacts, bounded overflow
translation, root horizontal scrolling, and unchanged semantic/source order,
plus Rust-only display-list/software-surface artifacts and bounded revisioned
Rust diagnostics for unsupported CSS. It does not yet provide
general CSS/nested/horizontal/stacking layout, vertical writing modes and
logical border-radius, negative/percentage/auto box-model values, positioned layout or general
flex/grid layout beyond the bounded single-row `display:flex` subset,
screen-shot-containing evidence, JPEG/PDF capture, or physical-pixel capture,
font/image fidelity, JavaScript, network/security policy, cookies/storage,
downloads, or platform windowing.

`BrowserRuntimeSession` remains the transport adapter for externally managed
Firefox and Safari. With the `native-engine` feature, the native backend is
also exposed through the explicit `BrowserRuntimeSession::connect_native` Rust
constructor and the local one-shot `--browser-runtime native` path. The CLI
default configuration accepts only `about:blank` and bounded `data:text/html`;
Rust callers may register `fixture://` documents. Native never accepts an
external endpoint, enters automatic selection, or falls back to another
backend. The native backend remains a multi-year architecture project with
its own standards conformance, security review, process isolation, and
platform certification. The proof backend must never be presented as browser
parity.

The machine-readable dependency and omission matrix is
[`backend-capability-matrix.json`](backend-capability-matrix.json).
