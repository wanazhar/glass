# glass-browser

`glass-browser` is Glass's standalone browser intelligence runtime and Rust
library. Its full `BrowserSession` drives local Chrome or Chromium through CDP;
its portable semantic session can connect to Firefox WebDriver BiDi or
SafariDriver W3C WebDriver endpoints. It does not bundle a browser, host a
remote browser service, or infer an autonomous action plan.

**Status: Current 0.3.14 source behavior.** This is the browser-only package;
the complete development TUI, project runtime, Pi Agent, editor, PTYs, and
Remote View belong to `glass-dev` and are not exported here.


The package provides:

- the `glass-browser` command for browser-only CLI, TUI, MCP, daemon, semantic,
  workflow, policy, and reliability operations; and
- the `glass_browser` Rust crate for embedding the same runtime.

The full terminal development environment and `glass` command are distributed
separately by [`glass-dev`](https://crates.io/crates/glass-dev).

## Install

```console
cargo install glass-browser --locked
glass-browser doctor
glass-browser --help
```

For subsequent Cargo registry releases, run `glass-browser update --dry-run`
to inspect the resolved package, source, root, and Cargo arguments, then run
`glass-browser update`. The command updates this `glass-browser` package and
does not switch to `glass-dev`. Use `--version VERSION` to pin a release and
`--force` only for an intentional reinstall.

This package installs only `glass-browser`. Install `glass-dev` instead when
you want both `glass` and `glass-browser`. Installing both packages into the
same Cargo home can make the last installation replace the shared
`glass-browser` executable; use one package as the owner of that command.

Chrome, Chromium, or Chrome for Testing is required for full
`BrowserSession`-backed operations. Firefox and Safari require an externally
started BiDi/WebDriver endpoint for the portable semantic command set.
`doctor`, Task Protocol validation/compilation, Web IR operations, policy
checks, and several scorecards are browser-free.

For Rust:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Observe, act, verify

This package owns browser lifecycle, semantic observation, guarded actions,
workflows, MCP, daemon, and its standalone browser TUI. It does not provide
the `glass` development command, project/agent/harness routes, native code
editor, PTY dev-suite launcher, or development-TUI Remote View.

```console
glass-browser navigate https://example.com
glass-browser observe --level interactive
glass-browser click r7:b42 --expected-revision 7
```

Observation is structured-first. Screenshots, full DOM, PDFs, evaluated
JavaScript, and form values are explicit operations and may require policy
capabilities. Locators must resolve exactly one current target; stale revisions
fail before browser input.

The feature-gated native engine additionally derives bounded integer-pixel
normal-flow rectangles, bounded physical four-side padding/margin shorthands
and longhands, explicit content-box or border-box sizing, bounded physical
min/max width/height constraints with bounded case-insensitive 15-layer/
unlayered local `revert-layer` rollback for finite non-negative integer-pixel
`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`
with absent local fallbacks, plus bounded case-insensitive 15-layer/unlayered
local `revert-layer` rollback for `box-sizing`, physical padding and margin
edges, including bounded `margin:auto`, with content-box/zero local fallbacks,
outer/content rectangles, bounded vertical
viewport scrolling, inherited text color, bounded `overflow:hidden` clips
shared by paint, viewport projection, and point hit-testing, bounded
axis-specific `overflow-x`/`overflow-y` `hidden`/`clip` clips through the same
owner, bounded
  side-specific solid/dashed/dotted/double/groove/ridge/inset/outset-border paint,
  bounded physical circular
  border radii, bounded inline-box line placement, bounded fixed pixel
  line-height flow with bounded inherited line-height, bounded direct-text
  flow fragments and source-order text
  paint, bounded word-aware wrapping, bounded source-whitespace boundaries
  across sibling text and supported inline items, bounded inherited
  `white-space: nowrap` collapsed one-line flow, bounded root horizontal
  scrolling, a deterministic
  clear/fill/text/border display list, a
  logical RGBA software surface, and
  bounded inherited ASCII `text-transform:none|uppercase|lowercase` layout,
  bounded non-negative fixed-pixel first-line `text-indent` for block flow,
  plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
  for `text-indent` with a finite `0px` fallback,
  bounded inherited non-negative fixed-pixel `word-spacing` across collapsed
  and supported preformatted ASCII spaces, bounded inherited non-negative
  fixed-pixel `letter-spacing` after every rendered fixed-cell character in
  each emitted fragment, composed with word spacing, plus bounded case-insensitive
  15-layer/unlayered inherited `revert-layer` rollback for `word-spacing` and
  `letter-spacing` with finite parent/root fallbacks,
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
  and finite shorthand expansion, plus bounded case-insensitive gap-family
  `revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with
  independent finite-pixel components,
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
  bounded PNG capture through the explicit backend operation. Rust callers can
also inspect bounded revisioned diagnostics for unsupported CSS; the details
are sanitized and this signal is not stable backend evidence;
it accepts native
`point=x,y` click targets
through its Rust API/CLI path. These are experimental local-content artifacts,
not CSS/layout, font, physical-pixel, or screenshot-evidence compatibility
claims.

The standalone browser TUI starts structured-only by default. Its first screen
offers `l` to launch a local browser, `a` to attach a verified DevTools port,
`n` to navigate, `t` to type, `j`/`k` to select semantic entities, and `Enter`
to activate. Use `live on`/`live off` in that TUI; these are not `glass-dev`
development-TUI commands. For disposable state:

```console
glass-browser --policy hardened --incognito \
  --policy-allow-host example.com navigate https://example.com
```

### Standalone terminal presentation

The standalone browser TUI is structured-first and responsive: terminals up to
72 columns use phone composition, up to 109 columns use compact composition,
and wider terminals use desktop composition. Continuous browser pixels are off
by default. With `--tui-live auto` and backend `auto`, Herdr is used only when
detected; otherwise the TUI remains semantic-only. `--tui-live on` with
backend `auto` uses the bounded ANSI fallback. Explicit `kitty` emits Kitty
graphics and explicit `ansi` uses the portable cell renderer; an unavailable
explicit Herdr backend remains semantic-only. `--tui-live-quality
data|balanced|smooth` targets approximately 3/6/12 FPS, and
`--tui-live-fit contain|cover|actual` controls ANSI fitting (`contain` is the
default; native image paths use contain). If a requested backend cannot
initialize, the TUI reports the failure and remains semantic-only.

```console
glass-browser --tui-live on --tui-live-backend kitty tui
glass-browser --tui-live on --tui-live-backend ansi --tui-live-quality data tui
```

## Rust quick start

```rust,no_run
use glass_browser::{BrowserSession, SessionOptions};

#[tokio::main]
async fn main() -> glass_browser::BrowserResult<()> {
    let options = SessionOptions::builder().incognito(true).build()?;
    let session = BrowserSession::start(&options).await?;
    let page = session.navigate("https://example.com").await?;
    let observation = session.observe().await?;
    println!("{} revision={}", page.url, observation.accessibility.revision);
    session.close().await
}
```

Call `close` on an owned session so Chrome can flush profile state. Attach mode
does not own or close the external browser.

## Major contracts

| Contract | Rust entry point | Purpose |
|---|---|---|
| Browser session | `BrowserSession`, `SessionOptions` | Chrome lifecycle, targets, frames, interaction, storage, evidence |
| Semantic observation | `browser::session::SemanticObservation` | Bounded page, regions, targets, records, revision, route |
| Evidence extraction | `ExtractionRequest`, `ExtractionEvidence` | Strict source-labelled non-mutating evidence |
| Glass Web IR v1 | `GlassWebIrV1` | Stable reconciled entities, relationships, details, coverage |
| Task Protocol | `GlassTask`, `compile_task` | Browser-free deterministic intent-to-plan compilation |
| Workflows | `WorkflowDefinition`, `WorkflowCheckpoint` | Typed bounded execution, proof, resume, and recovery |
| Knowledge | `KnowledgeStore` | Scoped advisory persistence and freshness assessment |
| Backend interface | `browser_backend` | Capability-evidenced semantic backend dispatch |
| Alternative runtimes | `BrowserRuntimeSession` | Portable Firefox BiDi and Safari WebDriver session; feature-gated native local session |
| Native engine | `BackendFactory::native`, `BrowserRuntimeSession::connect_native` (feature-gated) | Experimental Glass-owned engine with bounded local and external HTTP(S) HTML navigation, anchor download ownership, and bounded layout/point input, root horizontal and vertical viewport scrolling, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip` clips, bounded percent-decoded exact visible-id or legacy `<a name>` fragment scrolling/history restoration, simple `#:~:text=start[,end]` matching plus exact adjacent prefix/suffix affixes within the first visible non-truncated text run, inherited ASCII `text-transform:none|uppercase|lowercase` layout, bounded non-negative fixed-pixel first-line `text-indent` for block flow, bounded inherited non-negative fixed-pixel `word-spacing` across collapsed and supported preformatted ASCII spaces, bounded inherited non-negative fixed-pixel `letter-spacing` after every rendered fixed-cell character in each emitted fragment composed with word spacing, bounded inherited `font-weight:normal|bold|400|700` fixed-cell raster presentation with unchanged advances and clipped one-pixel bold dilation, bounded inherited `font-style:normal|italic` fixed-cell raster presentation with unchanged advances and clipped row-dependent italic shear, bounded inherited `word-break:normal|break-all` collapsed fixed-cell wrapping, bounded local `text-overflow:clip|ellipsis` on eligible clipped single-line direct text, bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for fixed-cell inline and inline-block line items, bounded block-level `display:flex` single-row placement for eligible direct element children with fixed widths/margins and normal-flow fallback for unsupported child shapes, one non-negative fixed-pixel `gap` between visible flex-row items, bounded `justify-content:flex-start|center|flex-end|space-between` free-space placement for eligible fixed-width flex rows, bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable source-order ties, bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024` allocation, bounded non-inherited `flex-basis:auto|Npx` base-size selection, case-insensitive 15-layer/unlayered rollback for these flex-item owners with native fallbacks and finite `flex` shorthand expansion, bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis placement using explicit content height or the auto row's maximum item outer height, bounded non-inherited `flex-direction:row|row-reverse` physical placement with item-attached margins, existing gap/justification/alignment, bounded inherited `text-decoration:none|underline|overline|line-through` paint including distinct shorthand combinations, bounded local `text-decoration-color` using the existing fixed palette and alpha grammar plus case-insensitive `currentColor` resolved from the element's local or inherited `color`, with separate glyph and line paint, bounded `text-decoration-line` longhand combinations sharing the same line-state owner, bounded inherited `text-decoration-style:solid|dashed|dotted|double|wavy` presentation with double's two separated solid bands and wavy's fixed eight-pixel phase, bounded inherited `text-decoration-thickness:1px|2px|3px|4px` positive-y bands with thickness-scaled integer dash/dot periods, bounded inherited `text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for underline and overline with unchanged line-through, bounded inherited `text-decoration-skip-spaces:none|all` same-run ASCII-space interval skipping across underline, overline, and line-through including adjacent letter/word/final-line justification spacing, bounded overflow translation, root horizontal scrolling, and unchanged semantic/source order, fragment-only, fixture-relative, and absolute local link activation, and Rust-only display/raster artifacts; native CLI is explicit and local-only |
The local decoration-color owner also accepts case-insensitive CSS-wide
`inherit`, `unset`, `initial`, and `revert`: explicit `inherit` copies the
parent's effective decoration color, reset forms resolve to the element's
current glyph color, and omission retains the existing local fallback.
The native rounded-geometry surface also accepts standalone, case-insensitive
15-layer/unlayered local `border-radius:revert-layer` for the bounded
one-to-four-value integer shorthand. Rollback preserves the zero-corner
fallback and the existing rounded layout, fill, border, point-hit, capture,
raster, overflow, and semantic/source-order owners; elliptical, percentage,
nested-clip, anti-aliasing, multiple-origin, and browser-wide
border-radius conformance remain outside the boundary. The same bounded radius
family honors a terminal case-insensitive `!important` marker with important
radius candidates above normal candidates and reversed named-layer priority in
the existing author-origin cascade; this does not claim generic `!important`
support for other properties. The bounded `background-color`, inherited
`color`, and `text-decoration-color` owners also honor terminal
case-insensitive `!important` with the same important-over-normal and reversed
named-layer ordering. The standalone physical `border-color` shorthand and
physical color longhands also honor the same terminal priority, as do the six
supported horizontal-tb logical `border-block-color`,
`border-block-start-color`, `border-block-end-color`, `border-inline-color`,
`border-inline-start-color`, and `border-inline-end-color` declarations through
their existing `ltr`/`rtl` physical-side projection; complete/side border
shorthands and other properties retain their existing bounded behavior. The
same bounded radius
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
The same bounded local cascade path also accepts standalone,
case-insensitive 15-layer/unlayered `revert-layer` for the existing local
`display` and `visibility` owners. It resolves rollback through lower concrete
candidates or the established normal-flow `display:auto` and visible
fallbacks, preserving `display:none`, `visibility:hidden`, and
`display:contents` behavior across hidden-subtree layout, point hit testing,
display-list, capture, raster, and semantic/source-order owners. Inherited
visibility, display decomposition, formatting-context parity, table/ruby/
flow-root details, animation, multiple origins, and browser-wide CSS
display/visibility conformance remain outside the boundary.
The native border surface also accepts bounded case-insensitive 15-layer/
unlayered local `revert-layer` for the physical `border`, `border-top`,
`border-right`, `border-bottom`, and `border-left` owners. Rollback resolves
each side through lower concrete candidates or the existing zero-width/no-paint
fallback, preserving shorthand/longhand precedence and the existing box-model
inset, border display-list, capture, raster, point-hit, and semantic/source-order
owners. Vertical writing modes, text orientation, elliptical/percentage logical
radii, border-image, gradients, `wavy`
and other unsupported border styles, animation,
multiple origins, and browser-wide CSS border conformance remain outside the
boundary. The same native border owner also accepts bounded case-insensitive
15-layer/unlayered `border-color`, `border-top-color`, `border-right-color`,
`border-bottom-color`, and `border-left-color` values, with one-to-four-value
physical shorthand expansion and independent per-side `revert-layer` rollback
to lower colors or bounded black, while preserving border width/style and all
existing box-model and artifact consumers. Standalone physical `border-color`
and physical color longhands also resolve `currentColor` from the element's
local or inherited color; complete physical border shorthands also resolve
`currentColor` from that same local or inherited color. Gradients, border-image,
and browser-wide border conformance remain outside the boundary. The physical
border-color owner also accepts case-insensitive CSS-wide `inherit`, `unset`,
`initial`, and `revert`: explicit `inherit` copies the parent's effective
per-side colors, reset forms resolve to the element's current color, and
omission retains the existing black side fallback. A single CSS-wide shorthand
token expands to all four sides; mixed CSS-wide/color shorthand forms remain
unsupported.
The native border width surface also accepts bounded case-insensitive
15-layer/unlayered `border-width`, `border-top-width`, `border-right-width`,
`border-bottom-width`, and `border-left-width` values, with one-to-four-value
physical shorthand expansion and independent per-side `revert-layer` rollback
to lower widths or bounded zero. Width-only declarations do not invent a style
or paint a border; existing border style/color and all box-model and artifact
consumers remain the owners. The physical border-width owner also accepts
case-insensitive CSS-wide `inherit`, `unset`, `initial`, and `revert`: explicit
`inherit` copies the parent's effective per-side widths, reset forms resolve to
zero, ordinary omission retains the zero fallback, a single CSS-wide shorthand
token expands to all four sides, and mixed CSS-wide/numeric shorthand forms
remain unsupported. The native border style surface also accepts
bounded case-insensitive 15-layer/unlayered local `border-style` and physical
`border-top-style`, `border-right-style`, `border-bottom-style`, and
`border-left-style` values from the finite
`none|hidden|solid|dashed|dotted|double|groove|ridge|inset|outset` grammar,
with one-to-four-value physical shorthand expansion and independent per-side
`revert-layer` rollback to lower styles. The style owner also accepts
case-insensitive CSS-wide `inherit`, `unset`, `initial`, and `revert`: explicit
`inherit` copies the parent's effective physical side styles, including private
`none`/`hidden` and styles from unpainted or zero-width parents; reset forms
resolve to the private `none` style, ordinary omission retains the no-style
fallback, a single CSS-wide shorthand token expands to all four sides, and
mixed CSS-wide/style shorthand forms remain unsupported. A winning `none` or
`hidden` blocks lower styles and resolves to the existing no-side/zero-width behavior before
layout and paint; `hidden` remains a private distinction for future table
conflict resolution. Width, style, and color components compose only after
independent resolution; width-only or style-only declarations do not invent
missing paint components. Collapsed-table border conflict resolution, logical
sides, fractional/percentage widths, `wavy` and other unsupported styles, and
browser-wide
border conformance remain outside the boundary.
The complete and physical border shorthands also accept the exact
case-insensitive omitted-component `none` and `hidden` forms through private
no-paint style sentinels. A winning `border:none`/`border:hidden`,
`border-top|right|bottom|left:none|hidden` shorthand blocks lower paint
without adding public `None`/`Hidden` styles or inventing shorthand defaults;
`hidden` remains private for future table conflict resolution and arbitrary
omitted-component forms remain outside this bounded surface.
The complete and physical border shorthands also accept bounded complete
`Npx hidden color` values; declared width and color remain private for future
table conflict resolution, while current non-table composition suppresses
hidden paint.
The complete and physical border shorthands also accept bounded complete
`Npx none color` values; declared width and color remain private for future
table conflict resolution, while current non-table composition suppresses
none paint.
The complete and physical border shorthands also accept exact case-insensitive
CSS-wide `inherit`, `unset`, `initial`, and one-author-origin `revert` values.
Explicit `inherit` copies the parent's effective physical width, style, and
color side values, including private `none`/`hidden`, zero-width, and unpainted
states; reset forms project zero width, private `none`, and `currentColor` so
later component declarations can compose, while ordinary omission remains
omission. Mixed CSS-wide/concrete forms remain unsupported.
The native border surface also accepts bounded horizontal-tb logical
`border-block`, `border-block-start`, `border-block-end`, `border-inline`,
`border-inline-start`, and `border-inline-end` shorthands plus their
`-width`, `-style`, and `-color` component families. One- or two-value block
and inline component pairs map to logical start/end; block sides map to
physical top/bottom, while inline sides map to physical left/right for `ltr`
and right/left for `rtl` through the resolved inherited `direction`. Complete
logical shorthands, CSS-wide keywords, `currentColor`, bounded named-layer and
unlayered `revert-layer`, physical/logical precedence, and component composition
reuse the existing physical border streams and box-model, display-list,
capture, raster, point-hit, and semantic/source-order owners. Vertical writing
modes, text orientation, elliptical/percentage logical radii, border-image,
gradients, table conflict resolution,
multiple origins, and browser-wide logical-border conformance remain outside
the boundary.
The native decoration surface also supports inherited signed fixed-pixel
`text-underline-offset:-4px..=4px`; it moves only the underline toward
decreasing or increasing y, keeps overline and line-through origins stable, and
clamps externally supplied display-list offsets to the same range.
The native Flexbox rollback surface also accepts standalone case-insensitive
`flex:revert-layer`, `flex-flow:revert-layer`, and `place-content:revert-layer`,
resolving each shorthand through its existing bounded component candidates
while preserving finite shorthand expansion and same-block longhand
precedence. The same private component-candidate owner accepts standalone
case-insensitive `gap:revert-layer`, `row-gap:revert-layer`, and
`column-gap:revert-layer`, preserving finite pixel parsing, shorthand expansion,
independent row/column fallback, and same-block declaration order.
The inherited text-presentation surface also accepts bounded case-insensitive
15-layer/unlayered `revert-layer` for `text-transform`, `font-weight`,
`font-style`, and `word-break`, resolving through finite parent/root fallbacks
without changing the existing fixed-cell layout, wrapping, or raster owners.
The inherited text-spacing surface also accepts bounded case-insensitive
15-layer/unlayered `revert-layer` for `word-spacing` and `letter-spacing`,
resolving through finite parent/root fallbacks without changing the existing
spacing, wrapping, alignment, or raster owners.
The local text-geometry surface also accepts bounded case-insensitive
15-layer/unlayered `revert-layer` for `text-indent` and `text-overflow`,
resolving through finite `0px`/`clip` local fallbacks without changing the
existing first-line flow or clipped-nowrap direct-text truncation owners.
The local paint-color surface also accepts bounded case-insensitive
15-layer/unlayered `revert-layer` for `background-color` and inherited `color`,
resolving through the existing `None`/parent-root fallbacks without changing
fill/text display-list, clipping, opacity, capture, or raster owners.
`background-color` also resolves case-insensitive `currentColor` from the
element's local or inherited `color`, and local `color: currentColor` resolves
from inherited color with a bounded black initial fallback without
self-recursion. Local `color` also accepts the case-insensitive CSS-wide
`inherit`, `unset`, `initial`, and `revert` keywords: the inherited forms use
the bounded parent-color/black-root fallback, while `initial` resets to black;
omitted direct roots remain `None`. This is the native engine's one-author-
origin model. Gradients, system colors, percentages, color spaces, and
multiple origins remain outside the boundary.
The non-inherited `background-color` owner also accepts those four
case-insensitive CSS-wide keywords: only `inherit` copies the parent's
optional concrete fill, while `unset`, `initial`, and `revert` preserve the
existing no-fill `None` fallback; ordinary omission remains no-fill.
The local overflow surface also accepts bounded case-insensitive
15-layer/unlayered `revert-layer` for `overflow`, `overflow-x`, and `overflow-y`,
resolving through independent visible/no-clip fallbacks while preserving the
existing paint, viewport-projection, point-hit, root-overflow, capture, and
semantic/source-order owners. Nested scrolling, scrollbars, and browser-wide
overflow semantics remain outside the boundary.
| Surfaces | `surfaces` | Multi-surface evidence, coverage, provenance, and bridge grants |
| Presentation | `presentation`, `terminal_graphics` | Bounded latest-frame metadata and terminal adapters |
| MCP/protocol | `mcp`, `protocol` | Negotiated stdio server and canonical request envelopes |
| Reliability | `reliability`, `reliability_runner` | Scenarios, fixtures, replay evidence, and certification gates |

## Cargo features

| Feature | Default | Purpose |
|---|---:|---|
| `visual-compare` | no | Explicit PNG comparison helpers |
| `fuzzing` | no | Test-only fuzz hooks |
| `native-engine` | no | Experimental Glass-owned browser backend with bounded local and external HTTP(S) HTML navigation and parent-owned anchor downloads with bounded physical box-edge layout/point input, root horizontal and vertical viewport scrolling, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip` clips, bounded local opacity subtree groups, inherited physical `text-align:left|center|right` fixed-cell line placement, bounded functional `rgba(R, G, B, A)` alpha colors for background/border/text paint, bounded inherited fixed-cell `text-decoration:none|underline|overline|line-through` paint including distinct shorthand combinations, bounded inherited ASCII `text-transform:none|uppercase|lowercase` layout plus bounded case-insensitive 15-layer/unlayered `revert-layer` rollback for inherited `text-transform`, `font-weight`, `font-style`, and `word-break` with finite parent/root fallbacks, bounded non-negative fixed-pixel first-line `text-indent` for block flow, bounded inherited non-negative fixed-pixel `word-spacing` across collapsed and supported preformatted ASCII spaces, bounded inherited non-negative fixed-pixel `letter-spacing` after every rendered fixed-cell character in each emitted fragment composed with word spacing, bounded inherited `font-weight:normal|bold|400|700` fixed-cell raster presentation with unchanged advances and clipped one-pixel bold dilation, bounded inherited `font-style:normal|italic` fixed-cell raster presentation with unchanged advances and clipped row-dependent italic shear, bounded inherited `word-break:normal|break-all` collapsed fixed-cell wrapping, bounded local `text-overflow:clip|ellipsis` on eligible clipped single-line direct text, bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for fixed-cell inline and inline-block line items, bounded percent-decoded exact visible-id or legacy `<a name>` fragment scrolling/history restoration, simple `#:~:text=start[,end]` matching plus exact adjacent prefix/suffix affixes within the first visible non-truncated text run, fragment-only, fixture-relative, and absolute local link activation, bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis placement using explicit content height or the auto row's maximum item outer height, and Rust-only display/raster artifacts |

## MCP

```console
glass-browser mcp-config --client generic
glass-browser --policy hardened --incognito \
  --policy-allow-host example.com --mcp
```

MCP uses stdio. Keep stdout reserved for protocol frames. Clients must complete
initialization and the initialized notification before tools. The negotiated
agreement reports exact schema and capability status.

Native target sessions also own a bounded nested `iframe`/`frame` tree,
including `srcdoc`, with stable parent-linked frame IDs and initialized child
engine owners. Explicit frame selection routes the normal Glass operations to
the selected child while parked parent and sibling engines retain their
state; frame and target shutdown drain every owner. CSP frame directives,
frame lifecycle events, shared same-origin frame scripting, and popup creation
remain separate production gates in issue #40.

## Safety and support

- Keep the CDP port on a trusted local interface.
- Use `--incognito` when persistence is unnecessary.
- Treat profiles, cookies, storage, screenshots, DOM, PDFs, evaluated results,
  and diagnostic logs as sensitive.
- Linux and macOS targets are declared; native certification is tracked
  separately. Windows receives browser-free source checks but has no certified
  native browser runtime.
- Firefox BiDi and Safari WebDriver are experimental and bounded. An
  unavailable capability fails closed rather than falling back to raw
  transport.
- The native engine is experimental, default-off, and available through the
  explicit Rust backend factory and feature-gated local CLI runtime. Its
  current semantic slice supports bounded local and external HTTP(S) document
  navigation, parent-owned anchor downloads, up to 32 independently owned page
  targets with explicit selection and cleanup, bounded box-model layout,
  root horizontal and vertical viewport scrolling, bounded inherited
  `white-space: nowrap` collapsed one-line flow, point input, click/type
  actions, bounded physical min/max width/height constraints, bounded
  non-inherited `flex-direction:row|row-reverse` physical placement for
  eligible rows, bounded non-inherited `flex-wrap`, `justify-content`,
  `align-items`, `align-self`, and `align-content` rollback, bounded
  non-inherited `order`, `flex-grow`, `flex-shrink`, and `flex-basis` rollback,
  including standalone `flex:revert-layer`, `flex-flow:revert-layer`, and
  `place-content:revert-layer` shorthand rollback,
  plus standalone case-insensitive `gap:revert-layer`, `row-gap:revert-layer`,
  and `column-gap:revert-layer` rollback with finite pixel components,
  plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
  rollback for `text-transform`, `font-weight`, `font-style`, and `word-break`
  with finite parent/root fallbacks,
  plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
  rollback for `word-spacing` and `letter-spacing` with finite parent/root
  fallbacks,
  plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
  rollback for `vertical-align` with finite parent/root fallbacks,
  and revision effects for local controls. The CLI default configuration
  accepts `about:blank`, bounded `data:text/html`, and validated HTTP(S)
  navigation; fixtures remain a Rust configuration path. Native downloads
  complete into existing authorized directories through the runtime and MCP
  surfaces. It is not a browser-parity claim or security boundary for hostile
  remote content.
- Its decoration surface also accepts bounded `text-decoration-line`
  combinations through the same inherited line-state owner; this longhand
  does not claim full CSS decoration propagation. It also accepts inherited
  `text-decoration-style:solid|dashed|dotted|double|wavy` values; `double`
  paints two solid thickness-preserving bands separated by one pixel, `wavy`
  repeats a fixed eight-pixel phase from each emitted text-run origin, and the
  other patterns reuse the existing one-pixel integer line-pattern helper.
  It also accepts inherited bounded
  `text-decoration-thickness:1px|2px|3px|4px` values, extends each selected
  line toward positive y, scales dashed/dotted periods by thickness, and
  clamps externally supplied display-list thickness to the same 4px ceiling.
  It also accepts inherited `text-decoration-skip-ink:auto|none`; `auto` uses
  the existing fixed-cell bold/italic glyph mask to suppress matching
  underline and overline pixels for the same text run, while `line-through`
  and non-intersecting pixels remain unchanged.
  It also accepts inherited `text-decoration-skip-spaces:none|all`; `all`
  suppresses decoration pixels over same-run ASCII-space advances, including
  adjacent letter, word, and final-line justification spacing, across underline,
  overline, and line-through, while `none` preserves continuous replay.
  It also accepts inherited signed fixed-pixel
  `text-underline-offset:-4px..=4px`, moving only the underline toward
  decreasing or increasing y while preserving overline and line-through
  origins; replay clamps externally supplied offsets to the same range.
- The local decoration-color surface accepts case-insensitive
  `text-decoration-color:currentColor` and
  `text-decoration-color:revert-layer` through the bounded 15-layer and
  unlayered/inline cascade buckets. `currentColor` resolves from the element's
  local or inherited `color`; when no concrete candidate remains it preserves
  the existing omitted-color fallback and keeps glyph and decoration paint
  colors separate. It also accepts case-insensitive CSS-wide `inherit`,
  `unset`, `initial`, and `revert`: explicit `inherit` copies the parent's
  effective decoration color, while the reset forms resolve to the element's
  current glyph color and omission remains the local fallback. Gradients, image
  functions, system colors, color spaces, percentages, and browser-wide
  text-color conformance remain outside the boundary.
- The inherited decoration-line surface accepts case-insensitive
  `text-decoration-line:revert-layer` and `text-decoration:revert-layer`
  through the same bounded 15-layer and unlayered/inline cascade buckets. Both
  forms share the existing three-bit line-state owner, roll back to the
  inherited value when no lower candidate remains, and preserve
  declaration-order, display-list, command, and fixed-cell raster behavior.
- Native extensions require explicit opt-in and a platform sandbox gate.

## Documentation

- [Glass product overview](https://github.com/wanazhar/glass/blob/main/README.md)
- [Getting started](https://github.com/wanazhar/glass/blob/main/docs/getting-started.md)
- [Complete feature reference](https://github.com/wanazhar/glass/blob/main/docs/features.md)
- [Rust SDK](https://github.com/wanazhar/glass/blob/main/docs/rust-sdk.md)
- [Runnable examples](https://github.com/wanazhar/glass/blob/main/docs/examples.md)
- [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md)
- [MCP integration](https://github.com/wanazhar/glass/blob/main/docs/mcp.md)
- [Security policy](https://github.com/wanazhar/glass/blob/main/SECURITY.md)
- [Complete uninstall and retained state](https://github.com/wanazhar/glass/blob/main/docs/installation.md#fully-uninstall-glass)
- [API documentation](https://docs.rs/glass-browser)

The docs.rs page documents the Rust library; use the [CLI
reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md) for
installed command behavior.

License: MIT.
