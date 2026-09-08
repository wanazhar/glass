# Complete feature reference

This reference maps capability domains to their user-visible entry points in
the current source checkout; it is not a cross-platform certification claim.
The machine-readable target status is in
[feature-parity.json](feature-parity.json).

**Status: Current 0.3.14 source behavior.** This reference describes the
checked-in source checkout. Checkout-only TUI/editor/collaboration behavior is
current source, not an immutable published-release claim.


Published docs.rs pages match the crate version they were built from
(`0.3.14` at last publication). They are not a substitute for this
source-behavior reference.


## Interfaces

| Domain | CLI | TUI | MCP | Rust | Guide |
|---|---|---|---|---|---|
| Install/update and browser launch/attach | `update`, global options, `doctor`, `install-chromium` | startup | session configuration | `BrowserSession`, `SessionOptions` | [Installation](installation.md) |
| Navigation and targets | `navigate`, `targets`, `new-target`, `select-target`, `close-target` | `navigate` | navigation and target tools | `BrowserSession` target/navigation methods | [CLI](cli.md) |
| Frames and topology | `frames`, `select-frame`, `verify` | current frame state | frame tools and predicates | session frame/topology APIs | [Actions](actions.md) |
| Structured observation | `observe`, `inspect-page`, `observe-delta` | semantic page pane | `observe`, `inspectPage`, `observeDelta` | `observe`, `semantic_observe` | [Semantic observation](semantic-observation.md) |
| Deep/visual evidence | `dom`, `screenshot`, `pdf`, `diagnostics` | explicit screenshot/live view | `getDOM`, `screenshot`, `printToPdf`, `diagnostics` | observation, visual, diagnostic APIs | [Feature details](#observation-and-evidence) |
| Pointer/keyboard/forms | click/type/key/form commands | common action commands | action tools | guarded session methods | [Actions](actions.md) |
| Wait and verification | `wait`, `verify`, `act-and-verify`, `preflight` | verified activity state | corresponding tools | wait/predicate/action APIs | [Action contract](action-contract.md) |
| Task Protocol and Web IR | `task`, `ir` | semantic execution results | task/Web IR tools | `task_protocol`, `task_compiler`, `web_ir` | [Semantic execution](semantic-execution.md) |
| Intent resolution | `resolve-intent`, `execute-intent`, `find-target` | intent commands | intent tools | session intent module | [Intent resolution](intent-resolution.md) |
| Workflows | `workflow`, `workflow-resume`, `checkpoint` | workflow command | `workflow`, checkpoint tools | workflow types and methods | [Workflows](workflows.md) |
| Knowledge/memory | `knowledge`, `memory` | knowledge summary | knowledge/memory tools | knowledge store/retrieval APIs | [Knowledge](knowledge.md) |
| Profiles/storage | `profiles`, cookies commands | profile status | cookie/storage tools | profile/storage APIs | [Profiles](profile-ergonomics.md) |
| Emulation | viewport option; browser API for network/CPU/UA/geo/timezone | status only | emulation tools | session emulation APIs | [CLI/MCP](cli.md) |
| Downloads/uploads/clipboard | dedicated commands | common clipboard keys only | dedicated tools | session APIs | [CLI](cli.md) |
| Policy/security | global policy options | effective policy status | startup policy and typed denials | `BrowserPolicy` and capability types | [Policy](policy.md) |
| Workspaces/daemon | `workspace`, `daemon` | daemon status | workspace and lease tools | `workspace`, `daemon` modules | [Daemon](daemon.md) |
| Development Runtime | `project`, `agent` | Development workspace | `glass.project.*`, `glass.agent.*` | `development` module | [Development Runtime](development-runtime.md) |
| Terminal/remote browser view | `browser`, `session`; Remote View is development-TUI-only | semantic-first Browser view; Herdr or bounded ANSI, plus tokenized loopback Remote View | metadata only; no MCP pixel stream | `connection`, `presentation`, `terminal_graphics`, `development::remote_view` | [Mobile/remote](mobile-remote.md) |

| Backends/surfaces/replay | `backend`, `surfaces`, `replay` | bounded status | corresponding tools | backend/surface/replay contracts | [Architecture](architecture/README.md) |
| Extensions | `--experimental-extensions`, capabilities | negotiated status | negotiated experimental status | `extensions` module | [Extensions](extensions.md) |
| Reliability/certification | `certify`, `smoke-sites` | compact status | scenario operations through core tools | reliability modules | [Reliability](reliability.md) |
| TypeScript/Python clients | repository smoke commands | — | clients wrap MCP | — | [Client SDKs](#typescript-and-python-clients) |

## Browser lifecycle and ownership

- Owned mode launches Chrome/Chromium, owns its process, selects exactly one
  page target, and closes Chrome explicitly on `BrowserSession::close`.
- The portable `BrowserRuntimeSession` can connect to an externally started
  Firefox WebDriver BiDi or SafariDriver W3C WebDriver endpoint for the
  bounded semantic one-shot command set.
- A `native-engine` feature build can construct the explicit local
  `BrowserRuntimeSession::connect_native` path or use
  `--browser-runtime native`; it accepts only local `about:blank` and bounded
  percent-decoded or standard padded-base64 `data:text/html` from the CLI and
  never contacts an endpoint. Its current
  Rust-only presentation artifacts include bounded outer/content box layout with
  physical four-side padding/margin shorthands and longhands plus explicit box
  sizing, bounded physical min/max width/height constraints with bounded
  case-insensitive 15-layer/unlayered local `revert-layer` rollback for finite
  non-negative integer-pixel `width`, `height`, `min-width`, `max-width`,
  `min-height`, and `max-height` with absent local fallbacks, plus bounded
  case-insensitive 15-layer/unlayered local `revert-layer` rollback for
  `box-sizing`, physical padding and margin edges, including bounded
  `margin:auto`, with content-box/zero local fallbacks, bounded root
  horizontal and vertical viewport scrolling,
  inherited text color, bounded `overflow:hidden` clips shared by paint,
  viewport projection, and point hit-testing, bounded axis-specific
  `overflow-x`/`overflow-y` `hidden`/`clip` clips through the same owner, plus
  bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback for
  `overflow`, `overflow-x`, and `overflow-y`, preserving independent
  visible/no-clip fallbacks and the existing paint, projection, point-hit,
  root-overflow, capture, and semantic/source-order owners; nested scrolling,
  scrollbars, and browser-wide overflow semantics remain outside the boundary,
  plus bounded terminal case-insensitive `!important` priority for those three
  overflow declarations: important candidates outrank normal candidates,
  named-layer priority reverses in the private important partition, and
  per-axis shorthand/longhand, invalid-later, inline, and `revert-layer`
  behavior remain bounded through the same clip and artifact owners,
  plus bounded case-insensitive 15-layer/unlayered local `border-radius:revert-layer`
  rollback for the one-to-four-value integer shorthand, preserving the
  zero-corner fallback and existing rounded layout, fill, border, point-hit,
  capture, raster, overflow, and semantic/source-order owners; elliptical,
  percentage, nested-clip, anti-aliasing, multiple-origin, and browser-wide
  border-radius conformance remain outside the boundary,
  plus bounded terminal case-insensitive `!important` priority for the
  complete physical and horizontal-tb logical radius family: important
  candidates outrank normal candidates and named-layer priority is reversed in
  the private important partition; the bounded `background-color`, inherited
  `color`, and `text-decoration-color` owners also honor the same terminal
  priority. The standalone physical `border-color` shorthand and physical
  color longhands also honor the same terminal priority, as do the six
  supported horizontal-tb logical `border-block-color`,
  `border-block-start-color`, `border-block-end-color`, `border-inline-color`,
  `border-inline-start-color`, and `border-inline-end-color` declarations
  through their existing `ltr`/`rtl` physical-side projection; the supported
  text-presentation set (`white-space`, `text-align`, `text-align-last`,
  `text-justify`, `direction`, text-decoration line/style/skip-ink/skip-spaces/
  thickness/underline-offset, `text-transform`, `font-weight`, `font-style`,
  `word-break`, `text-overflow`, `vertical-align`, `text-indent`, `word-spacing`,
  `letter-spacing`, and `line-height`) also honors the same terminal priority;
  local `display`, `visibility`, and `opacity` declarations honor the same
  terminal priority; the normal-only flex and gap declarations (`justify-content`,
  `align-items`, `align-self`, `align-content`, `place-content`,
  `flex-direction`, `flex-wrap`, `flex-flow`, `order`, `flex-grow`,
  `flex-shrink`, `flex-basis`, `flex`, `gap`, `row-gap`, and `column-gap`) also
  honor the same terminal priority through their existing shorthand/component
  owners; the six normal-only dimension declarations (`width`, `height`,
  `min-width`, `max-width`, `min-height`, and `max-height`) also honor the same
  terminal priority through private doubled candidate streams with important-
  over-normal ordering, reversed named-layer priority, inline precedence,
  invalid-later preservation, and `revert-layer` rollback; the normal-only
  physical box-model declarations (`box-sizing`, physical padding, and
  physical margin shorthand/longhand edges) also honor the same priority
  through private doubled candidate streams with per-edge importance,
  preserving content-box/border-box conversion, `auto` margin provenance,
  and existing geometry/artifact owners; the normal-only `overflow`,
  `overflow-x`, and `overflow-y` declarations also honor the same priority
  through private doubled x/y candidate streams with shorthand/x/y importance,
  preserving independent clip projection and visible/no-clip fallback;
  the horizontal-tb logical `padding-block`/`padding-inline` and
  `margin-block`/`margin-inline` shorthands plus their block/inline start/end
  longhands also project through resolved `ltr`/`rtl` direction into the same
  physical edge owners and honor the same bounded important-over-normal,
  reversed-layer, inline-important, invalid-later, and `revert-layer`
  behavior. The same physical and horizontal-tb logical box-model owners accept
  standalone case-insensitive `initial`, `unset`, and one-author-origin
  `revert`: padding and margin reset to zero and `box-sizing` resets to
  `content-box`. They also accept standalone case-insensitive `inherit`:
  physical values copy the parent's effective edges, `box-sizing`, and
  private margin `auto` provenance, while logical values read the parent in
  its resolved `ltr`/`rtl` direction before projecting into the child;
  root fallbacks and omitted-property non-inheritance remain explicit. The
  dimension declarations `width`, `height`, `min-width`, `max-width`,
  `min-height`, and `max-height` also accept standalone case-insensitive
  `inherit`: explicit values copy the parent's computed optional pixel value,
  including a parent `None`/auto result, while omitted dimensions remain
  local and do not inherit. Percentages, negative lengths, intrinsic sizing,
  aspect ratio, and new used-value state remain outside this bounded extension.
  These six local dimension declarations also accept standalone
  case-insensitive `initial`, `unset`, and one-author-origin `revert`, each
  resolving to the existing `None`/auto fallback; `revert-layer` remains a
  separate lower-layer rollback and mixed reset tokens remain unsupported.
  Vertical writing modes, additional logical properties, and remaining properties remain
  outside this bounded inheritance/reset family and generic `!important` semantics,
  plus exact case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
  one-author-origin `revert` for the radius owner: explicit `inherit` copies
  the parent's effective four-corner radius, including from unpainted or
  zero-width parents; reset forms resolve to zero corners and ordinary
  omission remains zero. Mixed CSS-wide/concrete and slash-separated radius
  forms remain unsupported. Physical circular corner longhands
  `border-top-left-radius`, `border-top-right-radius`,
  `border-bottom-right-radius`, and `border-bottom-left-radius` accept one
  bounded integer-pixel value or the same standalone CSS-wide keywords and
  compose with the shorthand through per-corner source order and
  `revert-layer`. Flow-relative corner longhands
  `border-start-start-radius`, `border-start-end-radius`,
  `border-end-start-radius`, and `border-end-end-radius` use the resolved
  horizontal-tb `direction` to map logical block/inline corners to the same
  physical streams for `ltr` and `rtl`; vertical writing modes, text
  orientation, elliptical, and percentage corner values remain unsupported,
  plus bounded case-insensitive 15-layer/unlayered local
  `opacity:revert-layer` rollback for the existing local 8-bit opacity owner,
  preserving the full-opacity (`255`) fallback, reduced-opacity group markers,
  software compositing, and unchanged layout, point-hit, capture, raster,
  overflow, and semantic/source-order owners; inherited opacity,
  stacking-context/blending parity, filters, animation, multiple origins, and
  browser-wide opacity conformance remain outside the boundary,
  plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for `display` and `visibility`, resolving through lower concrete candidates
  or the normal-flow `display:auto` and visible fallbacks while preserving
  `display:none`, `visibility:hidden`, and `display:contents` hidden-subtree,
  layout, point-hit, display-list, capture, raster, and semantic/source-order
  owners; inherited visibility, display decomposition, formatting-context
  parity, table/ruby/flow-root details, animation, multiple origins, and
  browser-wide CSS display/visibility conformance remain outside the boundary,
  plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for physical `border`, `border-top`, `border-right`, `border-bottom`, and
  `border-left` owners, resolving each side through lower concrete candidates
  or the existing zero-width/no-paint fallback while preserving
  shorthand/longhand precedence and the existing box-model inset, border
  display-list, capture, raster, point-hit, and semantic/source-order owners;
  vertical writing modes, text orientation, elliptical/percentage logical
  radii, border-image, gradients, `wavy` and
  other unsupported border styles, animation,
  multiple origins, and browser-wide CSS border conformance remain outside the
  boundary,
  plus exact case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
  one-author-origin `revert` for the complete physical `border`, `border-top`,
  `border-right`, `border-bottom`, and `border-left` shorthands: explicit
  `inherit` copies effective parent width/style/color side values, including
  private `none`/`hidden`, zero-width, and unpainted states; reset forms project
  zero width, private `none`, and `currentColor` for later component composition;
  ordinary omission remains omission and mixed CSS-wide/concrete forms remain
  unsupported,
  plus bounded case-insensitive 15-layer/unlayered local `border-color` and
  physical `border-top-color`, `border-right-color`, `border-bottom-color`,
  and `border-left-color` values with one-to-four-value shorthand expansion
  and independent per-side `revert-layer` rollback to lower colors or bounded
  black, preserving border width/style and existing box-model, display-list,
  capture, raster, point-hit, and semantic/source-order owners. Standalone
  physical `border-color` and physical color longhands also resolve
  `currentColor` from the element's local or inherited color; complete physical
  border shorthands also resolve `currentColor` from that same local or
  inherited color. Gradients, border-image, and browser-wide border conformance
  remain outside the boundary,
  plus case-insensitive CSS-wide `inherit`, `unset`, `initial`, and `revert` for
  the physical border-color owner: explicit `inherit` copies the parent's
  effective top/right/bottom/left colors, reset forms resolve to the element's
  current color, omission retains the existing black side fallback, a single
  CSS-wide shorthand token expands to all four sides, and mixed CSS-wide/color
  shorthand forms remain unsupported,
  plus bounded case-insensitive 15-layer/unlayered local `border-width` and
  physical `border-top-width`, `border-right-width`, `border-bottom-width`,
  and `border-left-width` values with one-to-four-value shorthand expansion
  and independent per-side `revert-layer` rollback to lower widths or bounded
  zero; width-only declarations do not invent a style or paint a border, and
  the physical border-width owner also accepts case-insensitive CSS-wide
  `inherit`, `unset`, `initial`, and `revert`: explicit `inherit` copies the
  parent's effective top/right/bottom/left widths, reset forms resolve to zero,
  ordinary omission retains the zero fallback, a single CSS-wide shorthand
  token expands to all four sides, and mixed CSS-wide/numeric shorthand forms
  remain unsupported. Standalone physical border-width declarations also
  accept a terminal case-insensitive `!important` marker: important widths
  outrank normal widths, use earliest-named-layer priority inside the bounded
  author-important partition, and preserve per-side `revert-layer` and inline
  behavior; complete and side-border shorthand priority is described below,
  bounded case-insensitive 15-layer/unlayered local `border-style` and
  physical `border-top-style`, `border-right-style`, `border-bottom-style`,
  and `border-left-style` values from the finite
  `none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar
  with one-to-four-value shorthand
  expansion and independent per-side `revert-layer` rollback to lower styles;
  the style owner also accepts case-insensitive CSS-wide `inherit`, `unset`,
  `initial`, and `revert`: explicit `inherit` copies the parent's effective
  physical side styles, including private `none`/`hidden` and styles from
  unpainted or zero-width parents; reset forms resolve to the private `none`
  style, ordinary omission retains the no-style fallback, a single CSS-wide
  shorthand token expands to all four sides, and mixed CSS-wide/style shorthand
  forms remain unsupported. Standalone physical border-style declarations also
  accept a terminal case-insensitive `!important` marker: important styles
  outrank normal styles, use earliest-named-layer priority inside the bounded
  author-important partition, preserve per-side `revert-layer`, and retain
  private `none`/`hidden` no-paint behavior; complete and side-border
  shorthand priority is described below. The complete physical `border`
  shorthand and the four physical side-border shorthands also accept a
  terminal case-insensitive `!important` marker: important shorthand
  projections outrank normal projections, use earliest-named-layer priority
  inside the bounded author-important partition, preserve per-side
  `revert-layer` and inline behavior, and carry the marker through the
  existing independent width/style/color component streams. The six supported
  horizontal-tb logical complete/side border shorthands also accept a terminal
  case-insensitive `!important` marker: important logical shorthand projections
  outrank normal projections, use earliest-named-layer priority inside the
  bounded author-important partition, preserve per-side `revert-layer` and
  inline behavior, and carry the marker through resolved `ltr`/`rtl` projection
  into the existing independent physical width/style/color component streams,
  a winning `none` or `hidden` blocks lower styles and resolves to no side/zero
  width before layout and paint; `hidden` remains a private distinction for
  future table conflict resolution; width, style, and color components compose
  only after independent resolution; width-only or style-only declarations do
  not invent missing paint components; collapsed-table border conflict
  resolution, fractional/percentage widths, vertical writing modes, text
  orientation, elliptical/percentage logical radii, `wavy` and other
  unsupported styles, and browser-wide border
  conformance remain outside the
  boundary. The complete and physical border shorthands also accept the exact
  case-insensitive omitted-component `none` and `hidden` forms through private
  no-paint style sentinels; winning omitted-component `hidden` remains private
  for future table conflict resolution and arbitrary omitted-component forms
  remain outside this bounded surface. The native engine provides bounded side-specific
  complete `Npx hidden color` border values with private width/color retention
  for future table conflict resolution; current non-table composition
  suppresses hidden paint. The native engine also provides bounded side-specific
  complete `Npx none color` border values with private width/color retention
  for future table conflict resolution; current non-table composition
  suppresses none paint. The native border surface also accepts bounded
  horizontal-tb logical `border-block`, `border-block-start`,
  `border-block-end`, `border-inline`, `border-inline-start`, and
  `border-inline-end` shorthands plus their `-width`, `-style`, and `-color`
  component families. One- or two-value block and inline component pairs map
  to logical start/end; block sides map to physical top/bottom, while inline
  sides map to physical left/right for `ltr` and right/left for `rtl` through
  the resolved inherited `direction`. The six supported logical border-width
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
  physical border streams and box-model, display-list, capture, raster,
  point-hit, and semantic/source-order owners. Vertical writing modes, logical
  radius, border-image, gradients, table conflict resolution, multiple origins,
  and browser-wide logical-border conformance remain outside the boundary.
  The native engine provides bounded side-specific
  solid/dashed/dotted/double/groove/ridge/inset/outset borders, bounded physical circular
  border radii, bounded inline-box line placement, bounded fixed pixel
  line-height flow with bounded inherited line-height, bounded direct-text
  flow fragments and source-order text
  paint, bounded word-aware wrapping, bounded source-whitespace boundaries
  across supported inline flow, bounded inherited `white-space: nowrap`
  collapsed one-line flow, bounded `overflow:hidden` clips shared by
  paint, viewport projection, and point hit-testing, a display list, a logical RGBA software
  surface, bounded local opacity subtree groups composited through transparent
  software layers, and inherited physical `text-align:left|center|right`
  fixed-cell line placement, bounded functional `rgba(R, G, B, A)` alpha
  colors for background, border, and text paint, plus bounded case-insensitive
  15-layer/unlayered local `revert-layer` rollback for `background-color` and
  inherited `color`, preserving the existing `None`/parent-root fallbacks and
  fill/text display-list, clipping, opacity, capture, and raster owners;
  `background-color` also resolves case-insensitive `currentColor` from the
  element's local or inherited `color`, and local `color: currentColor` resolves
  from inherited color with a bounded black initial fallback without
  self-recursion. Local `color` also accepts the case-insensitive CSS-wide
  `inherit`, `unset`, `initial`, and `revert` keywords: the inherited forms use
  the bounded parent-color/black-root fallback, while `initial` resets to black;
  omitted direct roots remain `None`. This is the native engine's one-author-
  origin model. Gradients, system colors, percentages, color spaces, and
  multiple origins remain outside the boundary, and the non-inherited
  `background-color` owner also accepts those four
  case-insensitive CSS-wide keywords: only `inherit` copies the parent's
  optional concrete fill, while `unset`, `initial`, and `revert` preserve the
  existing no-fill `None` fallback; ordinary omission remains no-fill, plus
  bounded inherited
  fixed-cell
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
named-layer rollback and mixed/unsupported forms remain bounded,
  bounded inherited ASCII `text-transform:none|uppercase|lowercase` layout,
  bounded non-negative fixed-pixel first-line `text-indent` for block flow,
  plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for `text-indent` with a finite `0px` fallback,
  bounded inherited non-negative fixed-pixel `word-spacing` across collapsed
  and supported preformatted ASCII spaces, bounded inherited non-negative
  fixed-pixel `letter-spacing` after every rendered fixed-cell character in
  each emitted fragment, composed with word spacing,
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
  while `inherit`, parent propagation, negative/fractional or percentage
  indentation, and browser-wide overflow conformance remain outside the
  boundary,
  bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
  fixed-cell inline and inline-block line items within the existing line box,
  plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
  rollback for `vertical-align` with finite parent/root fallbacks,
  bounded block-level `display:flex` single-row placement for eligible direct
  element children with fixed widths and margins, with normal-flow fallback
  for unsupported child shapes,
  one non-negative fixed-pixel `gap` between visible items in eligible flex
  rows,
  bounded `justify-content:flex-start|center|flex-end|space-between`
  free-space placement for eligible fixed-width flex rows,
  bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
  source-order ties,
  bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024`
  allocation plus `flex-basis:auto|Npx` base-size selection, including
  case-insensitive 15-layer/unlayered `revert-layer` rollback for the
  components and standalone `flex:revert-layer`, `flex-flow:revert-layer`,
  and `place-content:revert-layer` shorthand rollback with native fallbacks
  and finite shorthand expansion, plus standalone case-insensitive
  `initial|unset|revert` reset forms for
  `flex`, `flex-grow`, `flex-shrink`, and `flex-basis` that resolve through
  existing components to the finite `0 1 auto` initial tuple; standalone
  `initial|unset|revert` reset forms for `flex-direction`, `flex-wrap`, and
  `flex-flow` resolve through existing components to finite `row`/`nowrap`
  initial defaults; standalone `initial|unset|revert` reset forms for local
  flex-item `order` resolve through the existing local resolver to finite `0`;
  standalone `initial|unset|revert` reset forms for local `justify-content`
  reuse the existing bounded `flex-start` fallback; `inherit`, percentages,
  negative/fractional/intrinsic basis values, and other alignment owners remain
  outside the boundary, plus bounded
  case-insensitive gap-family
  `revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with
  independent finite-pixel components, plus standalone case-insensitive
  `initial|unset|revert` reset forms for all three gap declarations that
  resolve to zero; `inherit`, parent-gap propagation, percentages, and
  fractional/intrinsic values remain bounded diagnostics,
  bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
  for `text-transform`, `font-weight`, `font-style`, and `word-break` with
  finite parent/root fallbacks,
  bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`,
  and one-author-origin `revert` for inherited `white-space`, positive-pixel
  `line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
  `vertical-align`, `word-spacing`, and `letter-spacing`; `initial` uses the
  bounded root defaults, `inherit`/`unset` use the computed parent, `revert`
  uses the current one-author-origin parent fallback, and `revert-layer`
  remains the named-layer rollback form,
  bounded standalone case-insensitive CSS-wide `inherit`, `initial`, `unset`,
  and one-author-origin `revert` for inherited `text-align`, `text-align-last`,
  `text-justify`, and `direction`; `initial` uses left, auto, auto, and `ltr`
  root defaults, `inherit`/`unset` use the computed parent, `revert` uses the
  current one-author-origin parent fallback, and `revert-layer` remains the
  named-layer rollback form. The existing logical `ltr`/`rtl` projection and
  fixed-cell layout/display-list/raster consumers remain bounded,
  bounded case-insensitive 15-layer/unlayered rollback for non-inherited
  `flex-wrap`, `justify-content`, `align-items`, `align-self`, `align-content`,
  and `flex-direction` owners with their native fallbacks,
  bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
  placement using explicit content height or the auto row's maximum item outer
  height,
  bounded PNG capture, bounded local fragment navigation with bounded
  percent-decoded exact visible-id or legacy `<a name>` root scrolling and
  simple `#:~:text=start[,end]` matching against the first visible,
  non-truncated text run plus exact same-run prefix/suffix affixes, per-entry Rust history scroll restoration, bounded local
  anchor activation for fragment-only, fixture-relative, and absolute local
  hrefs, and bounded revisioned Rust diagnostics for unsupported
  CSS; these are not screenshot-containing evidence or stable transport
  diagnostics.
- Attach mode connects to an existing CDP endpoint. It does not own Chrome,
  its profile, launch flags, or shutdown.
- Incognito uses a disposable profile. Named profiles retain browser-managed
  state and are protected by an exclusive profile lock.
- An occupied launch port, ambiguous target, incompatible attach option, or
  foreign healthy endpoint fails closed.
- Chrome sandboxing remains enabled unless
  `GLASS_DISABLE_CHROME_SANDBOX=1` is set exactly.

## Observation and evidence

The default is structured-first:

```console
glass observe --level summary
glass observe --level interactive
glass observe --level structured --region REGION_ID
```

`summary`, `interactive`, `structured`, `detailed`, and `raw` progressively
increase semantic detail. Expansion is region-scoped and revision-bound.
Compact observation and semantic observation remain distinct contracts.

Explicit evidence operations are:

- `dom`/`getDOM` for the full DOM;
- `screenshot` for viewport, clip, element, or full-page PNG/JPEG/WebP evidence;
- `pdf`/`printToPdf` for printable page bytes;
- `diagnostics` for bounded console/network evidence;
- `observe --form-values` when policy allows form-value reads; and
- structured extraction with typed fields, item/byte limits, provenance, and
  revision-bound continuation.

Screenshots and live frames are never silently substituted for semantic
evidence. Live terminal pixels are ephemeral and latest-frame-only.

## Interaction and verification

Glass supports unique-target click, double-click, hover, drag, type, clear,
check, uncheck, select, key/down/up, shortcuts, bounded form fill, upload,
coordinate click, scroll, dialog accept/dismiss, recognized consent dismissal,
and popup-expecting click.

`--interaction human` sends bounded smooth pointer movement; `fast` sends
direct pointer events. Both modes produce browser-side evidence and preserve
the same targeting and revision contract.

Use `preflight` for read-only target resolution and clickability. Use
`act-and-verify` or verification predicates for postconditions. A failed
preflight performs no pointer event, focus, scroll, or revision mutation.

## Semantic compiler

The semantic pipeline is:

```text
fresh browser evidence -> Glass Web IR v1 -> Task Protocol compiler
                       -> revision-bound live bindings -> guarded operation
                       -> postcondition receipt/recovery
```

Compilation is browser-free and deterministic. Authored values do not enter
plans or receipts. Live binding must find exactly one current semantic target
at the compiled revision. Historical knowledge is advisory and cannot select
an executable target or authorize mutation.

Supported task families cover forms, field reads, navigation, dialogs,
pagination, tables, collections, and regions. Read-only and mutating families
have different lease and confirmation requirements.

## Workflows, checkpoints, and replay

Workflows provide typed inputs, bounded steps, conditions, retries, outputs,
evidence, traces, and terminal proof. Authoring commands compile YAML/JSON,
format, validate, lint, preview, diff, initialize templates, and record
semantic events. Checkpoints are at most 4 KiB and contain no secrets.

Resume refuses definition mismatch, route drift, completed checkpoints, and
post-dispatch uncertainty. Redacted reliability replay bundles are scenario-
bound evidence; attaching one does not start or mutate a browser.

## Knowledge and advisory memory

Persistent knowledge is isolated by exact origin, profile, workspace, backend,
surface, and lifecycle evidence. Records may be fresh, stale, contradicted, or
quarantined. Retrieval is bounded and exact/graph-compatible; embeddings are
disabled unless explicitly injected.

The separate advisory memory commands inspect, explain, export, forget, prune,
and reindex memory records. Neither memory system stores live browser handles
or authorizes an action.

## Development Runtime

The project runtime provides canonical-root detection, bounded file listing
and reads, native buffers, atomic saves, undo/redo, fuzzy search, PTYs, process
health/output, real rust-analyzer diagnostics, source/runtime graph, semantic
breakpoints, actor-attributed timeline/events, replay, Git worktree
experiments, collaboration claims, Neovim probes, resident sessions,
reconnect capsules, attention inbox, verification cards, evidence-aware review
prompts, and a fixed-path external harness bridge.

Project reads are handle-bound and capped. Mutations stay inside the canonical
root and record actor provenance. Prompt text and tool arguments are represented
in audit state by bounded metadata and digests, not raw values.

Desktop exposes eight destinations—Agent, Code, App, Terminal, Tasks, Git,
Debug, and More. Phone exposes Agent, Code, App, Tasks, and More. Auto layout
uses phone below 72 columns or 22 rows, compact below 118 columns or 32 rows,
and desktop otherwise; `--tui-layout` can force a mode.

First-run Agent onboarding is in the TUI. Trust-required projects first show
`I` inspect, `O` open untrusted, `1` trust once, and `T` trust project. `T` may
queue while a snapshot refresh holds the workspace lock. Agent typing or
`Enter` opens the composer when Pi is ready; otherwise `:agent setup`
installs/repairs the pinned runtime, `:agent setup login` opens Pi `/login`,
and `:agent update` refreshes it. `Ctrl-L` opens the shared composer dock on
every surface. Default mode is Agent; `Ctrl-Shift-A` cycles Ask, Plan, and
Agent. Composer `Enter` sends and stays open; `Esc` closes; failed sends keep
the draft for retry. Mutations use one-use approval cards.

The Code file preview is read-only and wraps long lines on narrow terminals.
Switching to Code queues the selected-file preview if the workspace is busy.
`Enter` or `i` enters full-screen editing. `Alt-W` toggles soft wrap off by
default; wrapped source uses whitespace-aware reflow and continuation gutters,
keeping cursor, selection, and syntax highlighting synchronized. Off uses
horizontal source-column scrolling. `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y` undo/redo,
and `Alt-A` attaches focused path/cursor/selection and unsaved content to a
do-not-edit Pi prompt. The editor starts in INSERT. `Esc` returns to NORMAL.
`Esc` from NORMAL on a clean buffer leaves the editor. Unsaved buffers offer
`S` save, `D` discard, `Q` discard-and-quit, or `Esc`/`N` stay. `Ctrl-C` opens
Glass quit confirmation from editor input; an already-open unsaved-exit prompt
keeps its save/discard/stay choices.

The native editor adds modal motions, operators, tree-sitter structural
textobjects with lexical fallback, local or resident-Pi FIM ghosts, and LSP
hover, definition, references, symbols, and inlay hints. Gutter marks expose
diagnostics, Git hunks, Agent carets, source-page links, proof results, and
comments. Bounded Agent pair-apply proposals stream into the buffer for
accept/reject/yield, while prove-it text attaches a predicate to the agent
request and becomes evidence only after live browser verification passes.

The `REVIEW` panel summarizes open anchored comments, pending exact-base
proposals, and checkpoints. Collaboration routes include `:editor
comment-selection TEXT`, `:editor comment PATH START END TEXT`, `:editor
comment-resolve ID`, `:editor propose PATH SUMMARY TEXT`, `:editor proposals`,
`:editor accept ID`, `:editor reject ID`, `:editor checkpoint NAME`, `:editor
restore CHECKPOINT_ID`, and `:editor replace-selection TEXT`. Conflicting
claims and stale proposal bases fail closed; resident buffers change until
`:editor save PATH` or `Ctrl-S`.

The optional development-TUI browser presentation is semantic-first and
pixel-off by default. `:browser view` toggles the selected path. With
`--tui-live auto` and backend `auto`, Herdr is used only when detected;
otherwise presentation stays semantic-only. `--tui-live on` with backend
`auto` uses bounded ANSI; explicit Kitty emits Kitty graphics and explicit
ANSI uses true-color half-blocks. An unavailable explicit Herdr backend remains
semantic-only. Quality `data|balanced|smooth` targets approximately 3/6/12 FPS;
failed capture clears live mode. The standalone `glass-browser` TUI uses
`live on`/`live off` and has no project, agent, editor, PTY, or Remote View
routes.

The development-TUI `:browser remote-open` route starts a tokenized,
revocable, loopback-only Remote View and prints an SSH-forward hint. It is not
a standalone `glass-browser` command. Chrome CDP, Remote View, and application
servers must remain private.


## Backends and surfaces

CDP is the production backend for the full session. Firefox WebDriver BiDi and
Safari W3C WebDriver are experimental bounded adapters for the portable
semantic session. The feature-gated native engine is an experimental,
fixture/data-URL-only Glass-owned backend exposed through the explicit Rust
factory and local one-shot runtime. Its current semantic surface includes
bounded presentation, inherited text color, bounded `overflow:hidden` clips
shared by paint, viewport projection, and point hit-testing, bounded
axis-specific `overflow-x`/`overflow-y` `hidden`/`clip` clips through the same
owner, side-specific
solid/dashed/dotted/double/groove/ridge/inset/outset-border paint, bounded
physical circular border radii,
bounded inline-box line placement, bounded fixed pixel line-height flow with
bounded inherited line-height,
bounded direct-text flow fragments and source-order text paint, bounded
word-aware wrapping, bounded source-whitespace boundaries across supported
inline flow, bounded `overflow:hidden` clips shared by paint, viewport
projection, and point hit-testing,
  plus bounded case-insensitive 15-layer/unlayered local `border-radius:revert-layer`
  rollback for the one-to-four-value integer shorthand with a zero-corner
  fallback and shared rounded layout, fill, border, point-hit, capture, raster,
  overflow, and semantic/source-order owners; elliptical, percentage,
  nested-clip, anti-aliasing, multiple-origin, and browser-wide border-radius
  conformance remain outside the boundary,
  plus exact case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
  one-author-origin `revert` for the radius owner: explicit `inherit` copies
  the parent's effective four-corner radius, including from unpainted or
  zero-width parents; reset forms resolve to zero corners and ordinary
  omission remains zero. Mixed CSS-wide/concrete and slash-separated radius
  forms remain unsupported. Physical circular corner longhands
  `border-top-left-radius`, `border-top-right-radius`,
  `border-bottom-right-radius`, and `border-bottom-left-radius` accept one
  bounded integer-pixel value or the same standalone CSS-wide keywords and
  compose with the shorthand through per-corner source order and
  `revert-layer`. Flow-relative corner longhands
  `border-start-start-radius`, `border-start-end-radius`,
  `border-end-start-radius`, and `border-end-end-radius` use the resolved
  horizontal-tb `direction` to map logical block/inline corners to the same
  physical streams for `ltr` and `rtl`; vertical writing modes, text
  orientation, elliptical, and percentage corner values remain unsupported,
  plus exact case-insensitive CSS-wide `inherit`, `unset`, `initial`, and
  one-author-origin `revert` for the complete physical `border`, `border-top`,
  `border-right`, `border-bottom`, and `border-left` shorthands: explicit
  `inherit` copies effective parent width/style/color side values, including
  private `none`/`hidden`, zero-width, and unpainted states; reset forms project
  zero width, private `none`, and `currentColor` for later component composition;
  ordinary omission remains omission and mixed CSS-wide/concrete forms remain
  unsupported,
  normal-flow outer/content box geometry with physical four-side padding/margin
  shorthands and longhands, bounded physical min/max width/height constraints
  with bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for finite non-negative integer-pixel `width`, `height`, `min-width`,
  `max-width`, `min-height`, and `max-height` with absent local fallbacks, plus
  bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback for
  `box-sizing`, physical padding and margin edges, including bounded
  `margin:auto`, with content-box/zero local fallbacks, explicit box sizing,
  and bounded root horizontal and vertical viewport scrolling,
  plus bounded terminal case-insensitive `!important` priority for `overflow`,
  `overflow-x`, and `overflow-y` through private doubled x/y candidate streams
  with important-over-normal ordering, reversed named-layer priority, inline
  precedence, invalid-later preservation, and per-axis `revert-layer` rollback,
  bounded inherited fixed-cell text-decoration lines with bounded
  `text-decoration-style:solid|dashed|dotted|double|wavy` and
  `text-decoration-thickness:1px|2px|3px|4px` positive-y raster bands,
  with standalone case-insensitive `inherit`, `initial`, `unset`, and
  one-author-origin `revert` on the inherited style owner; `initial` resolves
  to `solid`, inherited forms use the parent, and `revert-layer` remains the
  named-layer rollback,
  with the inherited thickness owner also accepting the same standalone reset
  family, with `initial` resolving to `1px`,
  bounded inherited `text-decoration-skip-ink:auto|none` same-run glyph
  intersection skipping for underline and overline with unchanged line-through;
  its owner accepts standalone case-insensitive `inherit|initial|unset|revert`,
  using the computed parent for inherited forms, finite `auto` at the root for
  `initial`, and the current one-author-origin parent fallback for `revert`,
  while `revert-layer` remains named-layer rollback,
  bounded inherited `text-decoration-skip-spaces:none|all` same-run ASCII-space
  interval skipping including adjacent letter, word, and final-line
  justification spacing across underline, overline, and line-through,
  bounded inherited signed fixed-pixel `text-underline-offset:-4px..=4px`
  that moves only the underline toward decreasing or increasing y, whose
  inherited owner also accepts standalone case-insensitive
  `inherit|initial|unset|revert` with computed-parent, finite `0px` root, and
  one-author-origin parent-fallback semantics; `revert-layer` remains the
  named-layer rollback and mixed/unsupported forms remain bounded,
  bounded inherited ASCII `text-transform:none|uppercase|lowercase` layout,
  bounded non-negative fixed-pixel first-line `text-indent` for block flow,
  plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for `text-indent` with a finite `0px` fallback,
  bounded inherited non-negative fixed-pixel `word-spacing` across collapsed
  and supported preformatted ASCII spaces, bounded inherited non-negative
  fixed-pixel `letter-spacing` after every rendered fixed-cell character in
  each emitted fragment, composed with word spacing,
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
  while `inherit`, parent propagation, negative/fractional or percentage
  indentation, and browser-wide overflow conformance remain outside the
  boundary,
  bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
  fixed-cell inline and inline-block line items within the existing line box,
  plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
  rollback for `vertical-align` with finite parent/root fallbacks,
  bounded block-level `display:flex` single-row placement for eligible direct
  element children with fixed widths and margins, with normal-flow fallback
  for unsupported child shapes,
  one non-negative fixed-pixel `gap` between visible items in eligible flex
  rows,
  bounded `justify-content:flex-start|center|flex-end|space-between`
  free-space placement for eligible fixed-width flex rows,
  bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
  source-order ties,
  bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024`
  allocation plus `flex-basis:auto|Npx` base-size selection, including
  case-insensitive 15-layer/unlayered `revert-layer` rollback for the
  components and standalone `flex:revert-layer`, `flex-flow:revert-layer`,
  and `place-content:revert-layer` shorthand rollback with native fallbacks
  and finite shorthand expansion, plus standalone case-insensitive
  `initial|unset|revert` reset forms for
  `flex`, `flex-grow`, `flex-shrink`, and `flex-basis` that resolve through
  existing components to the finite `0 1 auto` initial tuple; standalone
  `initial|unset|revert` reset forms for `flex-direction`, `flex-wrap`, and
  `flex-flow` resolve through existing components to finite `row`/`nowrap`
  initial defaults; standalone `initial|unset|revert` reset forms for local
  flex-item `order` resolve through the existing local resolver to finite `0`;
  standalone `initial|unset|revert` reset forms for local `justify-content`
  reuse the existing bounded `flex-start` fallback; `inherit`, percentages,
  negative/fractional/intrinsic basis values, and other alignment owners remain
  outside the boundary, plus bounded
  case-insensitive gap-family
  `revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with
  independent finite-pixel components, plus standalone case-insensitive
  `initial|unset|revert` reset forms for all three gap declarations that
  resolve to zero; `inherit`, parent-gap propagation, percentages, and
  fractional/intrinsic values remain bounded diagnostics,
  bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
  for `text-transform`, `font-weight`, `font-style`, and `word-break` with
  finite parent/root fallbacks,
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
  native point hit testing, local click/type actions, and a revision/changed
  effects signal; bounded text-fragment targets, including exact adjacent
  prefix/suffix affixes, are matched only within the first visible
  non-truncated layout run; its Rust-only
presentation artifacts include a display list, logical RGBA software surface,
and bounded PNG capture through the explicit backend operation. These are not
screenshot-containing evidence, and the backend is not a
remote-content security boundary. The proof backend is browser-free and only
certifies protocol conformance.
Capability omission or incompatibility is a typed denial, never a fallback to
raw transport or another backend.

Surface contracts describe document, frame, shadow, SVG, canvas, media,
embedded, PDF, browser-native, remote-stream, terminal, and extension
boundaries with provenance and coverage. Detection alone does not authorize
interaction. Coordinate actions require strong geometry evidence.

## TypeScript and Python clients

Repository clients in `clients/typescript` and `clients/python` are thin MCP
clients, not browser runtimes. They negotiate capabilities, expose browser and
Development Runtime helpers, maintain bounded request state, support
cancellation, cursor-based project events, process-health waits, reconnect
workflows, and mutation-lease scopes. They are part of the current `0.3.14`
source checkout, not published npm or PyPI packages.

Run their browser-free conformance smokes:

```console
npm --prefix clients/typescript run typecheck
GLASS_BINARY="$PWD/target/release/glass" node clients/typescript/smoke.mjs
GLASS_BINARY="$PWD/target/release/glass" python3 clients/python/smoke.py
```

## Limits and failure rules

Every input and retained output has a contract-specific bound. Unknown fields,
unknown variants, stale revisions, ambiguous targets, missing capabilities,
unsafe paths, oversized frames, and unsupported platform gates fail explicitly.
Glass does not silently retry a possibly applied mutation, downgrade evidence,
capture a screenshot, select a different target, or fall back from an explicit
backend preference.
