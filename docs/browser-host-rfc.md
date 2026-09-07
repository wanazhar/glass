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
corner-longhand, nested-clip, anti-aliasing, multiple-origin, and browser-wide
border-radius conformance remain outside the boundary. The same bounded local
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
owners. Logical sides, border-image, gradients, `wavy` and other unsupported
border styles, animation, multiple origins, and browser-wide CSS border
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
and browser-wide border conformance remain outside the boundary.
The same native border owner also accepts bounded case-insensitive
15-layer/unlayered `border-width`, `border-top-width`, `border-right-width`,
`border-bottom-width`, and `border-left-width` values with one-to-four-value
physical shorthand expansion and independent per-side `revert-layer` rollback
to lower widths or bounded zero. Width-only declarations do not invent a style
or paint a border. The same owner also accepts bounded case-insensitive
15-layer/unlayered local `border-style` and physical `border-top-style`,
`border-right-style`, `border-bottom-style`, and `border-left-style` values
from the finite
`none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar with
one-to-four-value physical shorthand expansion and independent per-side
`revert-layer` rollback
to lower styles. A winning `none` or `hidden` blocks lower styles and resolves
to no side/zero width before layout and paint; `hidden` remains a private
distinction for future table conflict resolution. Width, style, and color
components compose only after independent resolution, and width-only or
style-only declarations do not invent missing paint components;
collapsed-table border conflict resolution, logical sides, fractional/percentage
widths, `wavy` and other unsupported styles, and browser-wide border conformance
remain outside the boundary. The complete and physical border shorthands also
accept the exact case-insensitive omitted-component `none` and `hidden` forms
through private no-paint style sentinels; winning omitted-component `hidden`
remains private for future table conflict resolution and arbitrary
omitted-component forms remain outside this bounded surface. Complete and
physical border shorthands also accept bounded complete `Npx hidden color`
values with private width/color retention for future table conflict resolution;
current non-table composition suppresses hidden paint. Complete and physical
border shorthands also accept bounded complete `Npx none color` values with
private width/color retention for future table conflict resolution; current
non-table composition suppresses none paint. Bounded local opacity subtree groups through
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
combinations sharing the same
line-state owner, bounded inherited
`text-decoration-style:solid|dashed|dotted|double|wavy` presentation, where
`double` paints two thickness-preserving solid bands separated by one pixel and
`wavy` repeats a fixed eight-pixel phase `[0,1,2,1,0,-1,-2,-1]`,
bounded inherited
`text-decoration-thickness:1px|2px|3px|4px` positive-y bands with
thickness-scaled integer dash/dot periods, plus inherited
`text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for
underline and overline while preserving line-through, bounded inherited
`text-decoration-skip-spaces:none|all` same-run ASCII-space interval skipping
including adjacent letter, word, and final-line justification spacing across
underline, overline, and line-through, and bounded inherited
signed fixed-pixel
`text-underline-offset:-4px..=4px` that moves only the underline toward
decreasing or increasing y, and bounded inherited ASCII
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
general CSS/nested/horizontal/stacking layout, logical writing-mode sides,
negative/percentage/auto box-model values, positioned layout or general
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
