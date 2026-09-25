# Experimental capabilities

Experimental capabilities are intentionally opt-in. They are available for
evaluation, but Glass does not promise a stable API, complete platform
coverage, or unchanged behavior between releases.

## Discover status first

Inspect the negotiated capability manifest without starting a browser:

```console
glass capabilities
```

Check both fields:

- `capabilities` says whether a capability is currently usable;
- `capabilityStatuses` explains why, such as `disabledByPolicy`, `experimental`,
  or `blockedBySecurityGate`.

Clients should use `capabilityStatuses` when deciding whether to offer an
experimental feature. A boolean alone does not explain whether a feature is
disabled by policy, missing a runtime dependency, or blocked for security.

## Enable experimental extensions

Pass the explicit global flag for the process that will use the capability:

```console
# Inspect the resulting manifest
glass --experimental-extensions capabilities

# Start the TUI with the experimental capability negotiated
glass --experimental-extensions tui

# Start MCP with the experimental capability negotiated
glass --experimental-extensions --mcp
```

The flag is not persistent. Omitting it on the next process start disables the
experimental capability again. Glass prints a warning when the flag is used.

On Linux ARM64, the flag reports extensions as
`experimental` only when all of these conditions hold:

- the host is Linux ARM64;
- Bubblewrap is available and can start; and
- the extension uses the sandboxed host path.

On another OS or architecture, or when the sandbox is unavailable, the status
remains `blockedBySecurityGate`. Glass must not silently fall back to an
ordinary unsandboxed subprocess.

## What to expect

Experimental means more than “the code might have bugs.” Expect all of the
following:

- interfaces, manifest fields, and behavior may change in a later release;
- an invocation may fail, time out, or be unavailable on another platform;
- browser and extension behavior may differ from the stable capabilities;
- documentation and compatibility guarantees are limited to the stated
  target and runtime evidence; and
- an experimental capability must not be used as a production security
  boundary.

Extension code is untrusted code. The sandbox reduces its operating-system
authority, but does not make it trusted. Run only extensions you have reviewed,
avoid sensitive browser profiles while evaluating them, and do not provide
secrets merely because an extension declares a host or action permission.
Guarded browser actions remain subject to Glass policy, revision checks, target
permissions, and bounded verification.

## Why a capability remains experimental

A capability can remain experimental when one or more of these are true:

1. its public contract is still evolving;
2. required runtime dependencies are not available everywhere;
3. native security behavior has only been verified on a limited target set;
4. cross-interface compatibility is incomplete; or
5. failure, recovery, migration, or resource-limit behavior needs more
   evidence.

The status is a deliberate compatibility and safety signal, not a claim that
the implementation is unusable. Experimental capabilities can be useful for
local evaluation while remaining unsuitable for general support claims.

## Guidance for future experimental capabilities

Every new experimental capability should provide:

- a default-off configuration or explicit global opt-in;
- a stable capability ID and `experimental` status in the manifest;
- a fail-closed status when its platform or security dependency is missing;
- a visible warning that explains risk and expected instability;
- CLI, TUI, MCP, and library behavior that agrees on the status;
- deterministic tests for opt-in, refusal, and dependency absence; and
- documentation covering enablement, scope, limitations, and disablement.

Do not change a capability from `experimental` to `available` merely because
one local smoke test passes. Promote it only after its contract, security
boundary, applicable target environments, and compatibility evidence justify
the broader claim.

## Native browser engine

The Glass-owned native engine is the primary feature-enabled `glass-browser`
backend tracked by [issue #40](https://github.com/wanazhar/glass/issues/40).
It is enabled by default and selected without a silent CDP fallback:

```console
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
```

The native engine is headless and deterministic where its current contract is
bounded. It accepts `about:blank`, bounded percent-decoded or standard
padded-base64 `data:text/html` URLs, registered `fixture://` documents through
the Rust API, and external HTTP(S) documents through the native content worker.
It exposes lifecycle, navigation, page targets, frames, bounded URL/title/
visible-text evidence, semantic actions, storage, script, workers, capture,
and revisioned recovery through the existing backend dispatcher. The complete
Core Web Profile and production promotion gates remain owned by issue #40.

The action surface focuses supported controls, toggles checkbox/radio state,
selects an option in a single-select control, and replaces private text state
for `input`/`textarea` controls. Its bounded visibility gate excludes
`hidden`, `aria-hidden="true"`, and inline `display:none`/`visibility:hidden`
subtrees from visible text and rejects those action targets before mutation.
Explicit references are revision-bound; ambiguous, stale, disabled, read-only,
and unsupported targets fail before mutation. It provides only bounded
presentation, normal-flow outer/content box geometry with bounded physical
four-side padding/margin shorthands and longhands plus explicit box sizing and
bounded physical min/max width/height constraints with bounded
case-insensitive 15-layer/unlayered local `revert-layer` rollback for finite
non-negative integer-pixel `width`, `height`, `min-width`, `max-width`,
`min-height`, and `max-height` with absent local fallbacks,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `box-sizing`, physical padding and margin edges, including bounded
`margin:auto`, with content-box/zero local fallbacks,
bounded vertical viewport
scrolling, inherited text color,
bounded `overflow:hidden` clips shared by paint, viewport projection, and point
hit-testing, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip`
clips through the same owner, plus bounded case-insensitive 15-layer/unlayered
local `revert-layer` rollback for `overflow`, `overflow-x`, and `overflow-y`,
preserving independent visible/no-clip fallbacks and the existing paint,
projection, point-hit, root-overflow, capture, and semantic/source-order owners;
nested scrolling, scrollbars, and browser-wide overflow semantics remain
outside the boundary. The same three overflow declarations also accept a
bounded terminal case-insensitive `!important` marker through private doubled
x/y candidate streams with important-over-normal ordering, reversed named-layer
priority, inline precedence, invalid-later preservation, independent axis
projection, and `revert-layer !important` rollback. The same bounded local
cascade path also accepts
standalone, case-insensitive 15-layer/unlayered local
`border-radius:revert-layer` for the one-to-four-value integer shorthand,
preserving the zero-corner fallback and the existing rounded layout, fill,
border, point-hit, capture, raster, overflow, and semantic/source-order owners;
elliptical, percentage, nested-clip, anti-aliasing,
multiple-origin, and browser-wide border-radius conformance remain outside the
boundary. The same bounded radius owner also accepts exact case-insensitive
physical and horizontal-tb logical radius declarations with a terminal
case-insensitive `!important` marker; important candidates outrank normal
candidates and named-layer priority reverses within the important partition.
This remains radius-only for the radius owner; the bounded
`background-color`, inherited `color`, and `text-decoration-color` owners
also honor the same terminal priority. The standalone physical
`border-color` shorthand and physical color longhands also honor the same
terminal priority, as do the six supported horizontal-tb logical
`border-block-color`, `border-block-start-color`, `border-block-end-color`,
`border-inline-color`, `border-inline-start-color`, and
`border-inline-end-color` declarations through their existing `ltr`/`rtl`
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
margin provenance, and existing geometry/artifact owners. The horizontal-tb
logical `padding-block`/`padding-inline` and `margin-block`/`margin-inline`
shorthands plus their block/inline start/end longhands project through resolved
`ltr`/`rtl` direction into the same physical edge owners and honor the same
bounded important-over-normal, reversed-layer, inline-important, invalid-later,
and `revert-layer` behavior. The same physical and horizontal-tb logical
box-model owners accept standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert`: padding and margin reset to zero and `box-sizing`
resets to `content-box`. They also accept standalone case-insensitive
`inherit`: physical values copy the parent's effective edges, `box-sizing`,
and private margin `auto` provenance, while logical values read the parent in
its resolved `ltr`/`rtl` direction before projecting into the child; root
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
logical properties, and remaining properties do not
claim generic `!important` semantics or this bounded inheritance/reset family.
CSS-wide `inherit`, `unset`, `initial`, and one-author-origin `revert`: explicit
`inherit` copies the parent's effective four-corner radius, including from
unpainted or zero-width parents; reset forms resolve to zero corners and
ordinary omission remains zero. Mixed CSS-wide/concrete and slash-separated
radius forms remain unsupported. Physical circular corner longhands
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
The same bounded local cascade path also accepts standalone,
case-insensitive 15-layer/unlayered `opacity:revert-layer` for the existing
local 8-bit opacity owner, preserving the full-opacity (`255`) fallback,
reduced-opacity group markers and software compositing, and unchanged layout,
point-hit, capture, raster, overflow, and semantic/source-order owners;
inherited opacity, stacking-context/blending parity, filters, animation,
multiple origins, and browser-wide opacity conformance remain outside the
boundary. The same bounded local cascade path also accepts standalone,
case-insensitive 15-layer/unlayered `revert-layer` for the existing local
`display` and `visibility` owners. It resolves through lower concrete
candidates or the normal-flow `display:auto` and visible fallbacks, preserving
`display:none`, `visibility:hidden`, and `display:contents` behavior across
hidden-subtree layout, point hit testing, display-list, capture, raster, and
semantic/source-order owners. Inherited visibility, display decomposition,
formatting-context parity, table/ruby/flow-root details, animation, multiple
origins, and browser-wide CSS display/visibility conformance remain outside
the boundary. The native border surface also accepts bounded case-insensitive
15-layer/unlayered local `revert-layer` for the physical `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left` owners.
Rollback resolves each side through lower concrete candidates or the existing
zero-width/no-paint fallback, preserving shorthand/longhand precedence and
the existing box-model inset, border display-list, capture, raster, point-hit,
and semantic/source-order owners. Vertical writing modes, text orientation,
elliptical/percentage logical radii,
border-image, gradients, `wavy` and other unsupported border styles, animation,
multiple origins, and
browser-wide CSS border conformance remain outside the boundary. The same owner also accepts bounded
case-insensitive 15-layer/unlayered `border-color`, `border-top-color`,
`border-right-color`, `border-bottom-color`, and `border-left-color` values,
with one-to-four-value physical shorthand expansion and independent per-side
`revert-layer` rollback to lower colors or bounded black, preserving border
width/style and existing box-model and artifact consumers. Standalone physical
`border-color` and physical color longhands also resolve `currentColor` from
the element's local or inherited color; complete physical border shorthands
also resolve `currentColor` from that same local or inherited color. Gradients,
border-image, and browser-wide border conformance remain outside the boundary.
The physical border-color owner also accepts case-insensitive CSS-wide `inherit`,
`unset`, `initial`, and `revert`: explicit `inherit` copies the parent's
effective per-side colors, reset forms resolve to the element's current color,
omission retains the existing black side fallback, a single CSS-wide shorthand
token expands to all four sides, and mixed CSS-wide/color shorthand forms remain
unsupported.
The same owner also accepts bounded case-insensitive
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
and browser-wide border conformance
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
non-table composition suppresses none paint. The same complete physical border
shorthands also accept exact case-insensitive CSS-wide `inherit`, `unset`,
`initial`, and one-author-origin `revert` values. Explicit `inherit` copies
effective parent width/style/color sides, including private `none`/`hidden`,
zero-width, and unpainted states; reset forms project zero width, private
`none`, and `currentColor` for later component composition; ordinary omission
remains omission and mixed CSS-wide/concrete forms remain unsupported. The
native border surface also accepts bounded horizontal-tb logical
`border-block`, `border-block-start`, `border-block-end`, `border-inline`,
`border-inline-start`, and `border-inline-end` shorthands plus their
`-width`, `-style`, and `-color` component families. One- or two-value block
  and inline component pairs map to logical start/end; block sides map to
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
  described above. Complete logical shorthands, CSS-wide keywords, `currentColor`,
  bounded named-layer and
unlayered `revert-layer`, physical/logical precedence, and component composition
reuse the existing physical border streams and box-model, display-list,
capture, raster, point-hit, and semantic/source-order owners. Vertical writing
modes, text orientation, elliptical/percentage logical radii, border-image,
gradients, table conflict resolution,
multiple origins, and browser-wide logical-border conformance remain outside
the boundary. It also provides bounded side-specific
solid/dashed/dotted/double/groove/ridge/inset/outset-border paint,
bounded physical circular border radii, bounded inline-box line placement, bounded fixed pixel line-height
flow with bounded inherited line-height, bounded direct-text flow fragments and
source-order text paint, bounded
word-aware wrapping, bounded source-whitespace boundaries across supported
inline flow, bounded inherited `white-space: nowrap` collapsed one-line flow,
bounded root horizontal scrolling, bounded local opacity subtree groups with
inside-out transparent-layer compositing, bounded inherited physical
`text-align:left|center|right` fixed-cell line placement, bounded functional
`rgba(R, G, B, A)` alpha colors for background, border, and text paint, plus
bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback for
`background-color` and inherited `color`, preserving the existing
`None`/parent-root fallbacks and fill/text display-list, clipping, opacity,
capture, and raster owners; `background-color` also resolves case-insensitive
`currentColor` from the element's local or inherited `color`, and local `color:
currentColor` resolves from inherited color with a bounded black initial fallback
without self-recursion. Local `color` also accepts the case-insensitive CSS-wide
`inherit`, `unset`, `initial`, and `revert` keywords: the inherited forms use
the bounded parent-color/black-root fallback, while `initial` resets to black;
omitted direct roots remain `None`. This is the native engine's one-author-
origin model. Gradients, system colors, percentages, color spaces,
and multiple origins remain outside the boundary, and the non-inherited
`background-color` owner also accepts those four
case-insensitive CSS-wide keywords: only `inherit` copies the parent's
optional concrete fill, while `unset`, `initial`, and `revert` preserve the
existing no-fill `None` fallback; ordinary omission remains no-fill, plus
 bounded
inherited fixed-cell `text-decoration:none|underline|overline|line-through`
paint including distinct shorthand combinations, bounded local
`text-decoration-color` using the existing fixed palette and alpha grammar
plus case-insensitive `currentColor` resolved from the element's local or
inherited `color`, with separate glyph and line paint, plus case-insensitive
CSS-wide `inherit`, `unset`, `initial`, and `revert`: explicit `inherit` copies
the parent's effective decoration color, the reset forms resolve to the
element's current glyph color, and omission keeps the existing local fallback.
Bounded
`text-decoration-line` longhand combinations sharing the same line-state owner
now also accept standalone case-insensitive `inherit|initial|unset|revert`:
inherited forms use the computed parent, `initial` uses finite `none` at the
root, and `revert` uses the current one-author-origin parent fallback;
`revert-layer` remains the named-layer rollback and mixed/unsupported forms
remain bounded. The same line-state owner is bounded inherited
`text-decoration-style:solid|dashed|dotted|double|wavy` presentation, where
`double` paints two thickness-preserving solid bands separated by one pixel and
`wavy` repeats a fixed eight-pixel phase `[0,1,2,1,0,-1,-2,-1]`,
and the inherited owner accepts standalone case-insensitive `inherit`,
`initial`, `unset`, and one-author-origin `revert`: inherited forms use the
computed parent, `initial` uses the finite `solid` root default, and `revert`
uses the current one-author-origin parent fallback; `revert-layer` remains the
named-layer rollback and mixed reset tokens remain unsupported,
bounded PNG
`text-decoration-thickness:1px|2px|3px|4px` positive-y bands with
thickness-scaled integer dash/dot periods, and the inherited thickness owner
accepts standalone case-insensitive `inherit`, `initial`, `unset`, and
one-author-origin `revert`: inherited forms use the computed parent, `initial`
uses the finite `1px` root default, and `revert-layer` remains the named-layer
rollback, bounded inherited
`text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for
underline and overline while preserving line-through. The inherited owner also
accepts standalone case-insensitive `inherit|initial|unset|revert`: inherited
forms use the computed parent, `initial` uses finite `auto` at the root, and
`revert` uses the current one-author-origin parent fallback; `revert-layer`
remains the named-layer rollback and mixed/unsupported forms remain bounded.
Bounded inherited signed
`text-decoration-skip-spaces:none|all` same-run ASCII-space interval skipping
including adjacent letter, word, and final-line justification spacing across
underline, overline, and line-through, and bounded inherited signed
fixed-pixel
`text-underline-offset:-4px..=4px` that moves only the underline toward
decreasing or increasing y, whose inherited owner also accepts standalone
case-insensitive `inherit|initial|unset|revert`: inherited forms use the
computed parent, `initial` uses finite `0px` at the root, and `revert` uses the
current one-author-origin parent fallback; `revert-layer` remains the
named-layer rollback and mixed/unsupported forms remain bounded; bounded PNG
capture, and bounded
inherited ASCII
`text-transform:none|uppercase|lowercase` layout,
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
`revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with independent
finite-pixel components, plus standalone case-insensitive `initial|unset|revert`
reset forms for all three gap declarations that resolve to zero; standalone
case-insensitive `gap:inherit`, `row-gap:inherit`, and `column-gap:inherit`
copy their corresponding computed parent gap components only when explicitly
authored while omitted forms remain local with the bounded `0` fallback;
percentages and fractional/intrinsic values remain bounded diagnostics,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `text-transform`, `font-weight`, `font-style`, and `word-break` with finite
parent/root fallbacks,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `white-space`, positive-pixel
`line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
`vertical-align`, `word-spacing`, and `letter-spacing`; `initial` uses the
bounded root defaults, `inherit`/`unset` use the computed parent, `revert` uses
the current one-author-origin parent fallback, and `revert-layer` remains the
named-layer rollback form,
bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`, and
one-author-origin `revert` for inherited `text-align`, `text-align-last`,
`text-justify`, and `direction`, with left/auto/auto/`ltr` root defaults,
parent fallback for inherited forms, the existing `revert-layer` distinction,
and logical `ltr`/`rtl` projection preserved through the same artifact owners,
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
and Rust-only display-list/software-surface artifacts. Rust callers can
inspect bounded revisioned diagnostics for unsupported CSS without raw
stylesheet echo. Separate native slices also implement external HTTP(S)
navigation and resource loading, JavaScript, cookie/web storage, dialogs,
downloads, configured-root file loading, PNG capture, and PDF generation.
These are bounded implementations, not full browser parity. General
CSS/nested/stacking layout, vertical writing modes, negative/percentage/auto
box-model values, positioned or general flex/grid layout, complete
ECMAScript/Web IDL and DOM behavior, full network/security and storage
semantics, complete font/image fidelity, physical-pixel capture, and screenshot
embedding in generic evidence remain issue #40 gates. File reads remain
restricted to explicitly configured roots. Fragment navigation percent-decodes bounded UTF-8 `%HH` sequences,
then uses one exact, case-sensitive visible non-empty `id` target or, when no
`id` matches, one exact visible legacy `<a name>` target. It positions the
root viewport at the selected target's clamped document-space top and stores
one bounded root-scroll point per history entry for traversal restoration.
For ID/name targets, literal `+` remains a plus; missing, hidden, duplicate,
malformed, and invalid-UTF-8 targets preserve the current offset. IDs take
precedence and duplicate legacy names fail closed. Simple `#:~:text=start` and
`#:~:text=start,end` fragments also match the first visible, non-truncated
layout text run after independently decoding each term as bounded UTF-8;
bounded exact prefix/suffix affixes are supported when directly adjacent in
that same run; cross-run ranges, multiple directives, highlights, Unicode
normalization, and browser text-fragment parity remain unsupported.
Fragment-only, fixture-relative, and validated external HTTP(S) link activation
uses the existing bounded navigation path; fixture-relative links retain the
current registered fixture host. The native backend is the default in
feature-enabled products and `BrowserSession::start` constructs it directly;
the lower-level `BrowserRuntimeSession::connect_native` constructor remains
available. Neither constructor probes or falls back to Chromium or the
semantic proof backend. Because the current runtime is in-process and
incomplete, it is not a production security boundary and must not be presented
as safe for hostile remote content.
