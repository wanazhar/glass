# Native browser engine

Status: Experimental Phase 2 semantic DOM slice; feature-gated and not a
stable browser compatibility or security boundary.

This document is the repository contract for the Glass-owned native browser
engine described by [issue #40](https://github.com/wanazhar/glass/issues/40).
The engine is an experimental backend inside `glass-browser`; it is not a
third crate, a protocol adapter, or an embedded copy of another browser.

## Purpose and boundary

The native engine owns a deterministic, headless browser-platform kernel. The
current slices own lifecycle, one browsing context, local document resources,
HTML-to-DOM parsing, history, revisions, and bounded semantic evidence. The
revisioned DOM interaction model is the next serial slice. The engine does not
yet own CSS, layout, painting, hit testing, JavaScript, network access,
storage, downloads, prompts, or platform windows.

```text
BrowserBackendDispatcher
            |
            v
NativeEngineBackend       <- semantic backend profile and lifecycle adapter
            |
            v
NativeEngine              <- the only mutable page-state owner
    +-------+--------+----------------+
    |       |        |                |
  DOM   history  scheduler      resource loader
    |       |        |                |
    +--- semantic projection ---------+
    |       |        |                |
    +-------+--------+----------------+
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

## Configuration and limits

`NativeEngineConfig` contains an initial URL, a viewport descriptor, fixture
documents, and `NativeEngineLimits`. The viewport is recorded now so future
layout and paint work has a stable owner; it does not imply that Phase 1
performs layout.

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

Phase 1 supports only:

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

One fixed context ID, `native-context`, is exposed. Phase 1 has no popups,
frames, workers, or background contexts.

The DOM is an arena of generational `NodeId` values. A navigation constructs a
new document generation, so a node identity from an earlier document cannot be
mistaken for a current node. Public native semantic references additionally
carry the current revision (`ref=r<revision>:n<node-index>`); a reference from
before any navigation or mutation is rejected as detached.

The scheduler owns a deterministic logical clock and bounded ordered task
queue. It commits navigation in a reproducible order. Future interaction
mutation will remain synchronous and single-owner; the scheduler does not
spawn threads, sleep, or execute arbitrary callbacks.

## Phase 2 semantic DOM and interaction slice

The current Phase 2 slice intentionally exposes a narrow semantic surface
without pretending to implement CSS selectors, layout, or a browser event loop.
`NativeDocument::semantic_nodes` projects supported native roles and bounded
names in source order. Supported role inference includes buttons, links with
`href`, text-like inputs, textareas, checkboxes, radios, selects, options, and
headings. `aria-label`, `aria-labelledby`, associated/ancestor labels, and
visible element text are used in that order where applicable.

The native locator grammar is explicit and bounded:

```text
ref=r<current-revision>:n<arena-index>
id=<exact-id>
role=<role>
role=<role>[name=<normalized-name>]
name=<normalized-accessible-name>
text=<normalized-element-text>
```

Resolution must produce exactly one current element. Unknown locator forms,
missing targets, duplicate matches, stale references, and non-element
references fail explicitly. Actionability checks for disabled controls,
read-only textboxes, and unsupported action roles belong to the next
interaction slice. The grammar is a semantic locator contract, not a CSS
selector implementation; CSS selectors belong to the later CSS/layout phase.

The next serial interaction slice will accept only semantic `Click` and `Type`
actions through the native backend. It will focus supported controls, retain
values only inside native document state, and advance the document revision so
earlier references must be re-observed. It will not claim coordinate hit
testing, default navigation, or script-driven behavior.

## Backend capability contract

The native profile is `experimental` and declares:

| Capability | Level | Phase 1 contract |
|---|---|---|
| lifecycle | available | initialize and explicit close |
| navigation | available | local `about`, `data`, and registered fixture URLs |
| contexts | available | one active context |
| evidence | available | bounded URL, title, visible text, revision; native semantic projection is Rust-only |
| action | omitted | semantic interaction is the next Phase 2 slice |
| effects | omitted | mutation/effect dispatch is the next Phase 2 slice |
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

Phase 1 and current Phase 2 semantic-DOM tests cover:

- lifecycle transitions and repeated/invalid close behavior;
- `about:blank`, percent-decoded `data:` HTML, and registered fixtures;
- title and visible-text projection with hidden `head`, `script`, and `style`;
- history and monotonic revision behavior;
- deterministic scheduler ordering and queue bounds;
- failed navigation preserving the previous state;
- dispatcher capability denial and explicit-only backend selection;
- supported semantic roles, associated labels, attributes, and revision-bound
  references;
- duplicate and stale semantic targets plus supported control metadata.

This slice is not browser parity. It cannot be promoted or advertised as safe
for arbitrary remote content until CSS/layout, security policy, process
isolation, cancellation, conformance, and platform evidence exist.

Future phases may split the DOM parser into tokenizer/tree-builder modules and
add CSS, layout, paint, full event-loop, script, storage, and process
boundaries. Those changes require updates to this document, the epic, and
their dependency-ordered task files before implementation.
