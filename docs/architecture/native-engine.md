# Native browser engine

Status: Experimental Phase 2 semantic DOM/interaction slices, initial Phase 3
presentation/layout/display-list/software-surface slices, including bounded
style inheritance and paint clipping, plus feature-gated runtime/CLI
integration; not a stable browser compatibility or security boundary.

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
normal-flow geometry with point hit testing, a Rust-only clear/fill/text display
list, a bounded logical RGBA software surface, inherited text color through DOM
parent links, and bounded `overflow:hidden` paint clipping. The engine does not
yet own
general CSS, scrolling/stacking layout, screenshot capture, font/image
fidelity, hit-test visuals, JavaScript,
network access, storage, downloads, prompts, or platform windows.

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
bounded `data:text/html` navigation. Rust callers can still register bounded
`fixture://` documents through `NativeEngineConfig`; fixture registration is
not a CLI file-loading or network capability. Native CLI commands are limited
to navigate, click, type, text, observe, and targets, with semantic locators
instead of CSS selectors. Endpoint, external lifecycle, profile, screenshot,
storage, download, prompt, script/evaluate, MCP, and TUI paths fail closed.

## Configuration and limits

`NativeEngineConfig` contains an initial URL, a viewport descriptor, fixture
documents, and `NativeEngineLimits`. The viewport is recorded now so future
layout and paint work has a stable owner. The current 009 slice derives a
bounded Rust-only layout snapshot from it; it does not imply general browser
layout or painting.

The default limits are intentionally bounded:

| Resource | Default limit | Behavior at the limit |
|---|---:|---|
| source document | 256 KiB | navigation fails before state commit |
| DOM nodes | 4,096 | parse fails before state commit |
| open DOM depth | 128 | parse fails before state commit |
| visible text | 16 KiB | evidence is marked incomplete |
| history entries | 64 | oldest entry is evicted deterministically |
| queued scheduler tasks | 256 | scheduling fails explicitly |
| registered fixtures | 32 | configuration fails explicitly |

All configured URLs and fixture bodies are validated before engine startup.
Limits are configuration errors, not silent truncation, except for the
document's explicitly bounded evidence projection.

## Resource model

The current resource boundary (the Phase 1 loader) supports only:

- `about:blank`, which loads an empty document;
- `data:text/html,...` with UTF-8 percent-decoded HTML; and
- exact `fixture://...` URLs registered in `NativeEngineConfig`.

HTTP, HTTPS, filesystem, custom network, redirects, cookies, and all other
resource schemes fail closed. The resource loader has no filesystem or network
capability. Every successful resource has an opaque origin placeholder until
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
`style` attributes. It matches only one-compound universal/type, ID, class, or
attribute-presence/exact-value selectors and cascades `display` and
`visibility` by specificity, source order, and inline precedence. Unsupported
selectors and declarations are ignored. This state feeds text exclusion and
semantic actionability; it does not imply general CSS, inheritance, or paint.

The 009 layout seed derives integer-pixel rectangles from the current
presentation state and configured viewport using normal block/inline flow.
Only bounded `width:Npx` and `height:Npx` declarations affect geometry;
percentages, margins, padding, borders, positioning, flex, grid, transforms,
and font metrics remain unsupported. Layout is recomputed as a Rust-only
derived view. `display:none`, explicit hidden signals, and
`visibility:hidden` remove boxes; `display:contents` preserves eligible
descendants without creating its own box.

The 010 paint boundary derives a matching immutable display list with a white
viewport clear, explicit bounded solid backgrounds, and direct visible text
runs. The 011 raster boundary replays that list into a capped logical RGBA
surface using integer source-over blending and a fixed 5x7 ASCII glyph subset.
Both are Rust inspection artifacts only; there is no PNG/screenshot transport,
font engine, image decoder, or GPU path.

The 012 style boundary resolves only `color` through parsed DOM parent
links. Explicit child declarations remain authoritative, while absent values
inherit the nearest resolved ancestor color and otherwise use opaque black.
Other CSS inheritance and all user-agent/font/color-management behavior remain
unsupported.

The 013 paint boundary carries the intersection of bounded `overflow:hidden`
ancestor rectangles on fill/text commands and rechecks that intersection at
software replay. It does not create scroll offsets or a general clip stack.

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

The native backend accepts semantic `Click` and `Type` actions plus the native
`point=<x>,<y>` click-target extension. A point is checked against the
viewport, resolved to the deepest visible layout box, and walked to the
nearest actionable semantic ancestor before focus or control state changes.
There is no implicit scrolling or nearest-target adjustment. Click focuses
supported buttons, links, checkboxes, radios, textboxes, and comboboxes;
checkbox and radio state changes are retained in the document owner. Clicking
an option in a single-select combobox selects it and clears its siblings. Type
replaces private state for native `input` and `textarea` textboxes. A bounded
visibility gate recognizes `hidden`, `aria-hidden="true"`, and computed
`display:none`/`visibility:hidden`; hidden subtrees are omitted from text,
layout, and hit testing. Links do not perform default navigation, and no
action performs keyboard navigation, multi-select, or JavaScript execution.
Each accepted action advances the document revision exactly once, so earlier
references must be re-observed. The effects operation returns the current
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
| action | available | semantic click/type plus bounded native point targets for supported local controls; no scrolling or default browser behavior |
| effects | available | current revision and changed signal; bounded event metadata is Rust-only |
| script | omitted | JavaScript is unavailable |
| capture | omitted | no screenshots or pixels |
| storage | omitted | no cookies/local/session storage |
| prompts | omitted | no dialogs |
| downloads | omitted | no download pipeline |

The profile limitations are surfaced through `BackendProfile`. The dispatcher
returns typed capability denials for omitted operations. `EvidenceLevel::Deep`
can return a bounded incomplete projection; screenshot-containing levels are
explicitly denied. No operation silently falls back to CDP, the proof backend,
or another resource loader.

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
- title and visible-text projection with hidden `head`, `script`, and `style`;
- history and monotonic revision behavior;
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
  declarations, hidden-box exclusion, and Rust-only layout inspection.
- point hit testing with viewport bounds, deepest-hit ordering, actionable
  ancestor resolution, and pre-mutation rejection for empty/out-of-viewport
  points.
- matching-revision display-list generation with bounded clear/fill/text
  commands and explicit unsupported-paint boundaries.
- deterministic bounded RGBA surface replay, source-over blending, fixed-glyph
  text drawing, viewport clipping, and explicit surface-allocation limits.
- bounded inherited text-color resolution through DOM parent links, with
  explicit child overrides feeding deterministic text-run commands.
- bounded `overflow:hidden` ancestor intersections on fill/text commands and
  software-surface clipping with no scrolling or capture capability.
- explicit Rust native-session construction and feature-gated CLI dispatch for
  local URL shapes, including rejection of remote endpoints and unsupported
  browser-only flags.

This slice is not browser parity. It cannot be promoted or advertised as safe
for arbitrary remote content until CSS/layout/paint, security policy, process
isolation, cancellation, conformance, and platform evidence exist.

Future phases may split the DOM parser into tokenizer/tree-builder modules and
add general CSS, scrolling/stacking layout, paint, full event-loop, script,
storage, and process boundaries. Those changes require updates to this
document, the epic, and their dependency-ordered task files before
implementation.
