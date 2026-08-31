# Native browser engine

Status: Experimental Phase 2 semantic DOM/interaction slices, initial Phase 3
presentation/layout/display-list/software-surface/PNG-capture/box-model/
viewport-scroll/side-specific-border/bounded-pattern-border/bounded-corner-radius/
bounded-inline-flow/bounded-fixed-line-height/bounded-direct-text-flow/
bounded-word-wrap/bounded-physical-box-edges/bounded-whitespace-boundaries/
bounded-overflow-hit-test-projection/bounded-css-diagnostics/bounded-pixel-golden-capture/
bounded-descendant-selectors/bounded-overflow-clip/bounded-hard-line-breaks/
bounded-pre-line-breaks/bounded-preformatted-whitespace/
bounded-pre-wrap-whitespace slices,
bounded-base64-data-url/bounded-fragment-navigation-history/
bounded-local-link-activation/bounded-fragment-target-scroll,
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
bounded circular border radii, bounded outer/content box geometry, explicit root
viewport scrolling, bounded inline-box line placement, and bounded fixed pixel
line-height flow, bounded direct-text fragments at actual flow origins,
source-order text paint, bounded word-aware wrapping, bounded physical
four-side padding/margin edges, bounded source-whitespace boundaries, and
bounded hard line breaks in supported inline flow, and
bounded inherited `white-space: pre-line`, `white-space: pre`, and
`white-space: pre-wrap` source whitespace flow, and bounded rectangular
`overflow:hidden`/`overflow:clip` clips shared by paint,
viewport projection, and point hit testing, and bounded read-only diagnostics
for unsupported CSS input, plus bounded local same-document fragment
navigation and Rust history traversal.
Semantic local anchor activation reaches that same bounded navigation owner for
fragment-only and absolute local hrefs; unsupported href forms fail closed.
Visible local fragment targets may position the root viewport at their exact
raw `id`; the active history entry retains the bounded scroll offset for
restoration.
The engine does not
yet own
general CSS, nested/smooth/horizontal scrolling or scrolling/stacking layout,
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
control state, and bounded root-scroll state. If the raw fragment identifies a
visible exact `id`, the root viewport is positioned at that element's clamped
document-space top; missing, empty, hidden, and non-layout targets preserve the
current offset. Explicit Rust back/forward traversal moves
the bounded history cursor; different-resource entries are parsed before
commit, while same-resource fragment entries reuse the current document and
restore the target history entry's saved root offset.

Semantic anchor clicks use the existing action path as one bounded default
action. Fragment-only hrefs resolve against the current local resource, and
absolute about:blank, data:text/html, and registered fixture:// hrefs use the
existing loader. Empty hrefs remain click-only; relative paths, remote schemes,
and other unsupported destinations fail before mutation. Successful link
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
parse-before-commit invariants. Relative paths, remote/network navigation,
downloads, target contexts, and event-loop/default-action behavior remain
unsupported. Its implementation and validation evidence are recorded in the
task file and issue #40.

The 039 bounded-fragment-target-scroll boundary resolves one exact raw local
`id` through the existing document/layout owner and positions the root viewport
at its clamped document-space top. Every history entry stores one bounded root
scroll point; scroll actions update the active entry and traversal restores it,
including reparsed different-resource entries. Percent-decoded fragments,
`name`/text fragments, duplicate-id recovery, smooth/nested/horizontal/keyboard/
snap scrolling, sticky layout, and browser alignment parity remain unsupported.
Its implementation and validation evidence are recorded in the task file and
issue #40.

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
margin collapsing. Percentages, negative/auto values, logical sides, min/max
constraints, positioning, flex/grid, fractional metrics, and nested or
horizontal scrolling remain unsupported.

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
- semantic local anchor activation through dispatcher click;
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
  declarations, physical padding/margin shorthand and longhand cascade,
  explicit box sizing, outer/content rectangles, hidden-box exclusion, and
  Rust-only layout inspection.
- point hit testing with viewport bounds, deepest-hit ordering, actionable
  ancestor resolution, and pre-mutation rejection for empty/out-of-viewport
  points.
- matching-revision display-list generation with bounded clear/fill/text/border
  commands and explicit unsupported-paint boundaries.
- deterministic bounded RGBA surface replay, source-over blending, fixed-glyph
  text drawing, viewport clipping, and explicit surface-allocation limits.
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
- bounded positive-pixel line-height parsing/cascade, flow minimums, inline
  auto-height behavior, and explicit-height precedence.
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
