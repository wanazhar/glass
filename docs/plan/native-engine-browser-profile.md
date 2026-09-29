# Glass Core Web Profile

Profile: `GCWP-0.1`

Status: implementation baseline for issue #40. This profile is the M0
contract; it is not a conformance result or a production certification record
for the current native backend.

Authority: [issue #40](https://github.com/wanazhar/glass/issues/40), with the
implementation and ownership contract in
[`docs/architecture/native-engine.md`](../architecture/native-engine.md).

[wpt-about-blank]: https://github.com/web-platform-tests/wpt/blob/master/content-security-policy/child-src/child-src-about-blank-allowed-by-default.sub.html

## Purpose

`GCWP-0.1` defines the smallest profile that can honestly replace Chromium/CDP
for ordinary Glass browser work. “Complete” means complete for this versioned
profile, not complete for every obsolete plugin, proprietary vendor service,
unstable draft API, or browser-specific extension.

The profile is deliberately external-web capable. A native implementation that
only loads `about:blank`, `data:` URLs, or registered fixtures does not satisfy
this contract.

## Non-negotiable contract

When the native backend is selected, it must:

1. navigate ordinary external HTTP(S) pages without launching Chromium,
   connecting to CDP, using a remote browser, or silently switching backend;
2. expose the declared web-platform behavior through one Glass-owned lifecycle,
   origin, document, script, style, layout, render, input, storage, and
   accessibility model;
3. implement every normal Glass operation mapped to this profile, including
   CLI, `BrowserSession`, MCP, and TUI paths;
   the public Rust `glass_browser::BrowserSession` constructor must create the
   native backend directly; the Chrome/CDP migration API is explicitly named
   `CdpBrowserSession` and cannot be selected by fallback or backend probing;
   the native Rust session exposes the versioned semantic surface through
   `observe()`, `semantic_observe(level)`, and revision-checked region
   expansion, rather than requiring callers to use a native-internal method
   name. It also exposes page and frame topology through `list_targets()`,
   `create_target(url)`, `select_target(id)`, `close_target(id)`,
   `list_frames()`, and `select_frame(id)`. These methods use the native
   registry directly and do not trigger backend probing or fallback. The
   parent visible-text projection excludes HTML `iframe`/`frame` fallback
   content when representing the parent browsing context. The canonical
   session also exposes native history traversal (`go_back()` and
   `go_forward()` with revision-checked counterparts), `reload_with_revision()`,
   owner recovery (`recover()` and `recover_with_revision()`), and guarded
   `stop_loading_with_revision()`. Stop-loading requests cancellation of
   process-backed HTTP(S) navigation without committing the pending document;
   callers continue polling the navigation future until its content worker is
   reaped. The committed document, history, and revision remain unchanged, but
   transient worker-only state is discarded. If navigation already claimed
   its commit phase, stop returns a typed lifecycle error. Idle calls validate
   the revision and preserve it. Non-native sessions receive typed
   unsupported-operation errors without switching transports. The native
   persistent owner keeps one browser operation as the state writer while
   polling that future alongside its local socket. During an active operation,
   it services status and revision-checked `stopLoading` only when a
   process-backed HTTP(S) navigation is actually active; other operations are
   explicitly rejected as busy, and stop during non-navigation work fails
   promptly rather than waiting on the owner. The original command continues
   polling until cancellation settles and its content worker is reaped. Other
   navigation sources remain open profile work;
4. return typed, versioned unsupported errors only for capabilities explicitly
   outside this profile; and
5. run untrusted content behind the production process/sandbox boundary before
   promotion. The current in-process engine is a development implementation,
   not a security boundary.

CDP remains an explicitly selectable migration backend until native promotion.
It is never an implicit fallback for a native request.

## Capability matrix

| ID | Required ownership | Included behavior | Promotion evidence |
| --- | --- | --- | --- |
| `runtime` | browser and content runtime | process lifecycle, IPC, cancellation, event loop, task/microtask ordering, worker lifecycle, quotas, crash recovery, deterministic traces | lifecycle, cancellation, hostile-content, timeout, and restart tests on every supported OS |
| `network-origin` | network and security layer | URL/encoding, DNS, HTTP(S), TLS policy, redirects, MIME/charset, cache, cookies, origins/sites, CORS, CSP, Subresource Integrity for scripts/stylesheets/modules, mixed content, referrer, service workers, WebSocket/EventSource where selected, permissions | mandatory security/URL tests, selected Fetch WPT, differential request traces, no cross-origin leaks |
| `html-dom` | document platform | standards HTML tokenization/tree construction and error recovery, including SVG/MathML attribute-name adjustment and XLink/XML/XMLNS attribute namespace identity; document lifecycle, DOM mutation/selection/ranges, events, forms, focus, shadow DOM, custom elements, frames, and document policies | HTML parser/DOM/event WPT, malformed-document corpus, frame/AX fixtures |
| `javascript-webidl` | script and binding layer | ECMAScript realms, Web IDL bindings, promises, timers, Document-scoped (see the [HTML Standard's import-map algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)) parser- and dynamically registered import maps, module resolution/fetch/integrity and resolved-module-set locking; worker module graphs use worker URL/referrer resolution without Document import maps; request URLs including query/fragment plus module type identify distinct page and worker module records, while final response URLs provide descendant bases and fragments are omitted only from I/O; literal and runtime-valued dynamic imports are fetched only when invoked through the owning asynchronous module queue, with dynamic `type: "json"` options and no speculative target requests; rooted-file external module `src` URLs resolve relative to the document before configured-root validation; structured clone, workers, fetch/XHR, required DOM APIs, exception propagation, microtask ordering | script/evaluate parity, async-ordering, page/worker module identity and base-URL, worker, and binding tests |
| `css-layout` | style and geometry layer | tokenizer/parser, cascade, selectors, inheritance, custom properties, media/container queries, block/inline/flex/grid/table/positioned layout, overflow/scrolling, writing modes/bidi, animation/transition, and selected fragmentation | selected CSS/layout WPT, geometry differential corpus, scroll/hit-test invariants |
| `rendering` | paint and compositor | fonts/shaping/rasterization, images/SVG/canvas, media resource lifecycle, paint, clipping, transforms, filters, layers, compositing, hit testing, software/headless/GPU surfaces, screenshots, and print where selected | deterministic visual/print corpus, pixel/geometry diffs, capture repeatability |
| `contexts-input` | browser primitives | tabs/windows, browsing contexts, frames/popups/opener relationships, history/session state, keyboard/pointer/touch/IME, selection, drag/drop, clipboard, file chooser, upload/download, dialogs, prompts, permissions | native-only end-to-end Glass workflows and recovery tests |
| `storage` | durable web state | profile isolation, cookies, cache, session/local storage, IndexedDB-class storage selected by the profile, service-worker state, downloads, and deletion/eviction policy | restart/persistence/isolation/cleanup tests with no cross-profile leakage |
| `accessibility` | semantic and assistive surface | roles, states, properties, name/description computation, focus, actions, and incremental updates for the declared DOM/layout surface | accessibility-tree differential fixtures and action/focus tests |
| `glass-integration` | public Glass contract | stable backend capability profile, navigation, targets, DOM/AX/evidence, actions, key input, script/evaluate, waits/events, screenshots, contexts, storage, downloads/uploads, prompts, CLI, MCP, and TUI parity | all normal operations pass in native-only mode with no hidden CDP process/socket |

### Shared-profile cookie synchronization

When separately created native sessions use the same explicit profile path,
accepted cookie changes are merged into the durable profile under its existing
profile lock and then published through the bounded profile event journal.
Another live session reads its leased journal cursor at browser operation
boundaries, ignores its own records, coalesces external changes by cookie key,
and applies them to its request loader and live content process before the next
request. Applying a journal record is runtime-only: the receiving session does
not write the same change back to the profile or republish it. Slices 820-821
verify HTTP response `Set-Cookie` updates and explicit native cookie
import/clear API calls across live sessions, including response-driven
deletion. Slice 821 verifies that API changes are persisted before journal
publication and that API clear includes a cookie written by a peer before the
clearing session synchronizes. A session with no explicit profile has no
cross-session state; separate profile paths do not share notifications.
Delivery is operation-boundary synchronization, not an OS file watcher or an
interrupt to an already-running request. See the
[Slice 820 task](tasks/native-engine-browser-820.md) for implementation and
evidence boundaries.

### Direct Fetch from SharedWorker

The local content-process route supplies each SharedWorker create command with
the constructor storage key derived from the owning document's origin,
context, frame, and generation. Browser-coordinated SharedWorker creation
continues to receive this key from the browser owner. A connected module
SharedWorker can issue bounded direct Fetch API requests through its existing
worker registry and loader, consume response bodies, and apply accepted
response-cookie changes before its next request. Slice 822 verifies
`credentials: include` and `credentials: omit`, ordinary and HttpOnly request
cookies, response updates and deletion, and the subsequent request. For direct
worker Fetch, `Request.credentials` defaults to `same-origin`. For each URL in
a redirect chain, the loader sends and accepts cookies only when the mode is
`include`, or when the mode is `same-origin` and that URL has the worker's
origin. `omit` sends no cookies and ignores response cookies. These modes are
defined by the [Fetch Standard credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode).
Slice 823 preserves the mode through direct worker Fetch requests and verifies
same-origin, cross-origin, redirect, and response-cookie behavior with a
two-origin process-backed regression (1 passed; 21.71 seconds). The separate
SharedWorker module-graph credentials regression also passed (1 passed; 24.50
seconds). Slice 824 carries page Fetch credentials through the controlled
ServiceWorker FetchEvent and a subsequent Service Worker `fetch(event.request)`.
Its two-origin process-backed regression checks all three modes, the
`Request.credentials` default, invalid-mode rejection, and cookie
send/accept behavior (see the [Slice 824 task](tasks/native-engine-browser-824.md)).
Slice 825 exposes `Request.referrer` and `Request.referrerPolicy` on page and
worker requests, preserves the effective source through clones and controlled
Service Worker handoff, applies the selected policy at each redirect target,
and distinguishes cache entries by effective referrer. Its process-backed
two-origin regression covers defaults, overrides, invalid values, policy
outcomes, redirects, and `fetch(event.request)` (1 passed; 37.37 seconds).
The fetch-side policy defaults to `strict-origin-when-cross-origin`; a redirect
response's Referrer-Policy value affects the following hop. Slice 826 applies
the HTTP Document response policy to page Fetch when its request-level policy
is empty; its process-backed regression passed (1 passed, 875 filtered; 32.45
seconds), covering ordered/unknown tokens, invalid-only fallback, explicit
override, and removal of a prior same-URL policy. Slice 827 applies parsed and
live `meta name="referrer"` updates to the Document-owned page-Fetch default;
its process-backed two-origin regression passed (1 passed, 875 filtered;
52.01 seconds), covering live insertion and attribute changes, aliases,
invalid/empty no-ops, update order, removal persistence, explicit override, and
actual outgoing `Referer` values. Meta policy remains scoped to the Document
and is not written into URL-keyed shared state. Element-level policy,
independent worker response containers, and policy inheritance remain open.
This is not complete Referrer Policy/WPT conformance; full Fetch/Web IDL
semantics, WPT conformance, and cross-platform parity remain open. See the
[Slice 827 task](tasks/native-engine-browser-827.md),
[Slice 826 task](tasks/native-engine-browser-826.md),
[Slice 825 task](tasks/native-engine-browser-825.md),
[Slice 822 task](tasks/native-engine-browser-822.md) and
[Slice 823 task](tasks/native-engine-browser-823.md).

### Fetch referrer policy containers

This elaborates the existing `network-origin` requirement for referrer
handling and the `html-dom` requirement for document policies; it does not
change GCWP-0.1's scope, version, or promotion thresholds. The policy container
created from an HTTP(S) Document response supplies the default referrer policy
for requests whose own `referrerPolicy` is empty. The Fetch `Request` property
remains empty in that case; the effective request policy comes from the
client's Document policy container. A response `Referrer-Policy` header uses
the last recognized policy token; unknown tokens are ignored, and no recognized
token leaves the default `strict-origin-when-cross-origin` policy in effect.
Redirect response policies update the request before the next hop as described
above.

The required referrer surface also includes policy delivery through `meta
name="referrer"`, applicable element `referrerpolicy` attributes, worker
response policy containers, and the relevant inheritance rules. These are
distinct initiator/owner paths and must not be inferred from page Fetch
coverage. Slice 826 covers an HTTP Document response header as the initial
default; slice 827 covers parsed and live `meta name="referrer"` updates to
the Document-owned page-Fetch default. Slice 828 implements the `img`
referrer-policy content attribute and limited-known-values IDL reflection.
Missing, empty, and invalid attributes inherit the owning Document's current
response/meta policy. Image redirects retain the original Document as the
referrer source and apply a recognized redirect response policy to the next
hop. The process-backed local HTTP test passed (1 passed, 876 filtered; 23.62
seconds), covering same-/cross-origin headers, element overrides, IDL
mutation, invalid fallback, and redirect updates. Other element initiators,
independent worker policy containers, and broader Referrer Policy/WPT
conformance remain open. See the [slice 826 task](tasks/native-engine-browser-826.md),
[slice 827 task](tasks/native-engine-browser-827.md), and
[slice 828 task](tasks/native-engine-browser-828.md).

Slice 829 implements element policy for the initial fetch of external classic
scripts discovered in markup or inserted dynamically. The script element's
valid policy overrides its owning Document's current response/meta policy;
missing, empty, and invalid values inherit that live Document policy. Redirect
requests retain the Document as referrer source, apply recognized redirect
response policies to the next hop, and compute the outgoing Referer during
cache revalidation. Its process-backed two-origin regression passed (1 passed,
877 filtered; 33.70 seconds), covering actual headers, IDL reflection and
mutation, response/meta fallback, redirects, and cache revalidation.
Module-script graphs, worker requests, other element initiators, broader
Referrer Policy/WPT conformance, and cross-platform parity remain separate
requirements. See the [slice 829 task](tasks/native-engine-browser-829.md).

Slice 830's contract extends element-level referrer policy to network
stylesheet requests initiated by HTML stylesheet links. It covers parser and
dynamic link fetches, `HTMLLinkElement.referrerPolicy`, the live Document
response/meta default, redirect policy updates, and cache revalidation. CSS
`@import` and stylesheet subresources remain distinct initiators. This is a
bounded extension of GCWP-0.1, not a profile-version or promotion-threshold
change; see the [slice 830 task](tasks/native-engine-browser-830.md).

Slice 831 extends script-element referrer policy to network module-script
entries and recursive static imports. The root fetch uses the Document URL as
its referrer; each imported module uses its referencing module's response URL,
including the final URL after redirects. A module response's recognized
`Referrer-Policy` header becomes the policy for that module's dependencies;
redirect response policy changes apply to the next redirect hop. Cached module
responses retain this metadata and 304 updates must preserve or replace it
according to the response headers. This is a bounded extension of GCWP-0.1,
not a profile-version or promotion-threshold change. Dynamic `import()`
inheritance from the active module, module workers/worklets, preloads, and
broader Referrer Policy/Fetch and WPT conformance remain separate requirements.
See the [slice 831 task](tasks/native-engine-browser-831.md).

Slice 832 carries the effective fetch referrer policy of each page module into
its runtime `import()` requests and through each dynamically loaded module's
static and later dynamic dependencies. The active module response URL remains
the referrer source, and a dynamically loaded module's recognized response
policy becomes its dependency policy. Ordinary `fetch()` requests are
unchanged. Worker/worklet module imports, classic-script dynamic imports, and
broader Referrer Policy/Fetch and WPT conformance remain separate requirements.
See the [slice 832 task](tasks/native-engine-browser-832.md).

Slice 833 carries the owning Document's current effective policy into the
initial DedicatedWorker and SharedWorker module-script request. The worker
module graph then uses each referencing module's final response URL and
effective response/inherited policy for its static and dynamic imports. This
process-backed HTTP regression passes for DedicatedWorker and SharedWorker,
including top-level dynamic-import settlement, redirect policy changes,
response-policy overrides, nested dependencies, and actual Referer headers.
The slice remains in progress pending Slice 832. It does not establish the
worker-global policy container used by ordinary Fetch/XHR, nor ServiceWorker or
classic-worker policy inheritance. Those remain explicit profile requirements;
see the
[slice 833 task](tasks/native-engine-browser-833.md).

Slice 834 initializes the DedicatedWorker/SharedWorker global referrer
default from the final network worker-script response policy, falling back to
the policy-container default when no recognized response token exists; `file:`
workers inherit the creator Document policy. Ordinary Worker Fetch and both
XHR modes use that default only when a request-level policy is empty, without
changing the public `Request.referrerPolicy` property or explicit overrides.
ServiceWorker policy containers, nested Workers, `importScripts()`, module
graphs, and broader Referrer Policy/Fetch or WPT conformance remain separate.
See the [slice 834 task](tasks/native-engine-browser-834.md).

Slice 835 carries the effective Worker-global referrer policy and the final
root Worker URL into statically discovered classic `importScripts()` graphs.
Every dependency, including nested imports, resolves against and uses the
root Worker URL as its request referrer; imported-script response policies do
not replace the Worker-global policy. The same classic dependency path is
covered for ServiceWorkers using their root response policy or the standard
default. Runtime/dynamic `importScripts()` timing, module workers, ordinary
ServiceWorker Fetch/XHR, broader Referrer Policy/Fetch or WPT conformance,
remote CI, and cross-platform certification remain open. See the
[slice 835 task](tasks/native-engine-browser-835.md).

Slice 836 applies the fetched ServiceWorker script resource's policy
container to module dependency requests and to Fetch and asynchronous XHR
started by the ServiceWorker itself. An empty request policy inherits the
worker-global default without changing the public `Request.referrerPolicy`;
explicit request policies remain authoritative. FetchEvent request provenance,
the ServiceWorker registration entry request, synchronous XHR, navigation
preload, dynamic module imports, broader Referrer Policy/Fetch or WPT
conformance, remote CI, and cross-platform certification remain separate.
See the [slice 836 task](tasks/native-engine-browser-836.md).

Slice 837 applies the active Document's creation URL and effective
response/meta policy container to ServiceWorker entry requests initiated by
`register()` and `ServiceWorkerRegistration.update()`, retaining that URL
across same-document History API changes. The captured policy is transported
with the command so a live meta update is not lost at the Document-to-host
boundary. This policy controls only the entry request; the fetched
ServiceWorker script response continues to own the worker's policy container
and dependency behavior from Slice 836. Background restoration and soft
updates without an active client retain separate provenance. See the
[slice 837 task](tasks/native-engine-browser-837.md).

### Explicit `tabindex` focus baseline

Within the current light-DOM focus scope, a valid explicit `tabindex` makes an
attached, rendered element programmatically focusable even when its semantic
role is not an interactive control. Nonnegative values participate in
sequential navigation: positive values sort first by value, ties and zero use
tree order. Negative values remain programmatically focusable but are omitted
from sequential navigation. Actually disabled supported native controls and
non-rendered targets are excluded. Native image-map areas retain their
image-anchor focus model.

The native element projection exposes `tabIndex`: a valid signed-long
`tabindex` attribute is returned directly; otherwise the HTML default is zero
for the standard focus-default element set and the first `summary` child of a
`details`, and minus one for other elements. Setting the property uses Web IDL
`long` conversion and writes the canonical integer to the content attribute,
so subsequent focus traversal observes the same live mutation.

Form-associated autonomous custom elements are excluded from sequential and
programmatic focus while disabled by their own `disabled` attribute or a
disabled fieldset ancestor, except when they are descendants of that fieldset's
first `legend` child. The disabled result is shared from the custom-element
lifecycle owner with the native focus owner, including content-process and
same-origin-frame routes. An ordinary custom element's `disabled` attribute
does not confer disabled-control behavior.

An element with the `inert` attribute, and its light-DOM descendants, are not
programmatically or sequentially focusable. The `HTMLElement.inert` property
reflects the boolean content attribute, and live property/attribute changes
affect subsequent focus decisions. This is the focus-specific baseline only;
it does not claim the complete flat-tree inert model or the broader effects of
inertness on hit testing, editing, selection, or accessibility.

Focus transitions dispatch `focus` then `focusin` on the newly focused target;
the old target receives `blur` then `focusout` before the new target's events.
`focus` and `blur` do not bubble; `focusin` and `focusout` do. The paired
`relatedTarget` identifies the other endpoint of the transition, or is null
when focus enters or leaves without a paired element. These events are
`FocusEvent` instances exposing the bounded `UIEvent.view`/`detail` and
`FocusEvent.relatedTarget` surface. Local, content-process, and same-origin
parent-frame projection tests cover direct focus, Tab, ordering, propagation,
and related targets. This is not complete focus-event Web IDL or reentrant
focus-algorithm conformance; see the [slice 777 task](tasks/native-engine-browser-777.md).

Ordinary HTML elements expose the current `GlobalEventHandlers` IDL property
set from the shared HTML element prototype, including `onfocus` and `onblur`.
`Window` exposes `GlobalEventHandlers` and `WindowEventHandlers`; `Document`
exposes `GlobalEventHandlers` plus `onreadystatechange` and
`onvisibilitychange`. `body` and `frameset` expose `WindowEventHandlers`, and
their IDL handler properties for those names and the six Window-reflecting
names (`blur`, `error`, `focus`, `load`, `resize`, `scroll`) share the owning
Document's `defaultView` handler state. Other `GlobalEventHandlers` on those
elements remain element-owned. Handler state is allocated only when assigned
and keyed by the native event owner, so refreshed same-origin frame
projections read, replace, and clear the same handler. Local, HTTP(S)
content-process, and same-origin parent-projection tests cover property
presence/null initialization, representative dispatch, target/`this`
identity, Window reflection, and local body click behavior. Slice 780 left
body/frameset inline content attributes on the generic element-owned path;
slice 781 below adds their Window-target routing. Full inline-handler,
event-source, Web IDL, and WPT conformance remain open. See the
[slice 780 task](tasks/native-engine-browser-780.md), the
[slice 779 task](tasks/native-engine-browser-779.md), and the
[slice 778 task](tasks/native-engine-browser-778.md).

Slice 781 routes body/frameset content attributes for the Window-targeted
event names through the same owner-keyed handler value and listener slot as
their IDL properties and the owning Window. CSP authorization is checked
against the element and exact source before the value is installed; a blocked
replacement does not overwrite an active routed handler. Unchanged projected
attributes do not reactivate the listener during snapshot refresh. Ordinary
element content attributes remain element-owned. This does not complete
inline-handler lexical environments, CSP reporting, or event/WPT conformance;
the separate Window `onerror` and typed `onbeforeunload` paths are recorded in
slices 782 and 784 below. See the
[slice 781 task](tasks/native-engine-browser-781.md).

Generated script-report `ErrorEvent`s are cancelable. At Window, the special
five-argument `onerror` invocation applies only to `ErrorEvent` objects of
type `error`; an exact `true` return sets cancellation, while a plain
`Event("error")` uses the ordinary event argument. Local tests cover direct
dispatch (including a non-cancelable synthetic `ErrorEvent`), a body handler
alias, and uncaught script reports. General error-reporting policy remains
separate work. See the
[slice 782 task](tasks/native-engine-browser-782.md).

Ordinary event-handler callbacks in the shared DOM IDL and HTML
content-attribute paths now set the canceled flag for an exact `false` return,
including on non-cancelable events; later listeners still run. This leaves the
special Window `onerror` and `onbeforeunload` return paths distinct.
Worker-specific event dispatch wrappers remain separate work. See the
[slice 783 task](tasks/native-engine-browser-783.md).

Slice 784 implements the typed Window `onbeforeunload` return path on native
`BeforeUnloadEvent`s: `null`/`undefined` do not cancel, other values are
converted to `DOMString` and cancel, including an empty string, and the
converted callback value fills `returnValue` only when it is empty. Explicit
`returnValue` assignments use `DOMString` conversion. This feeds the
already-existing host prompt gate without changing sticky-activation policy or
exposing page-provided prompt text. Plain `Event("beforeunload")` keeps
ordinary exact-`false` handler semantics rather than the special return path.
The internal event brand survives document-script bootstrap refreshes so
cached handlers work with newly created events. Conversion exception reporting
remains open; see the
[slice 784 task](tasks/native-engine-browser-784.md).

Slice 785 updates dedicated worker host-message dispatch to use the worker
realm's `MessageEvent` and to apply exact-false cancellation from the
`WorkerGlobalScope.onmessage` event-handler slot before continuing to later
listeners. This rule sets the canceled state even though worker message events
are non-cancelable; ordinary `addEventListener` callback returns remain
ignored. Its local regression verifies the event type and target identity,
cancellation before later listeners, non-false handler returns, and ignored
listener returns. Worker scheduling, specialized XHR/WebSocket/EventSource
dispatchers, worker error handlers, and full worker event/WPT conformance
remain separate. See the [slice 785 task](tasks/native-engine-browser-785.md).

Slice 786 gives shared-worker `connect` dispatch a worker-realm
`MessageEvent`: empty `data`, `source` set to the connection's inside port,
and a frozen `ports` array containing that same port. `onconnect` exact-false
cancellation is visible to later listeners even though the event is
non-cancelable; ordinary listener returns remain ignored. Connection task
ordering and ownership are unchanged. The focused two-connection regression
and scoped package check pass locally. See the
[slice 786 task](tasks/native-engine-browser-786.md).

Slice 787 adds the bounded classic dedicated-worker startup runtime-error
path: dispatch a cancelable worker-realm `ErrorEvent`, apply the global
`onerror` five-argument/exact-`true` cancellation rule, and forward uncanceled
errors to the owning `Worker` as an `ErrorEvent`. It preserves the worker for
subsequent messages. Module graph fetch/parse/resolve/link failures (the
bounded post-link evaluation-rejection counterpart is slice 788), later
callback exceptions, source-location fidelity, and full worker error/WPT
conformance remain separate. See the [slice 787 task](tasks/native-engine-browser-787.md).

Slice 788 adds the corresponding initial dedicated module-worker runtime
error: a rejected evaluation after successful module loading/parsing/linking is
reported first to the worker global, while fetch/parse/resolve/link failures
remain owner-side startup errors. Pending top-level-await evaluation retains
its existing `WouldBlock` handling and is not sent through the new error
reporter. See the
[slice 788 task](tasks/native-engine-browser-788.md).

Slice 789 implements runtime-error reporting for exceptions thrown by
`onmessage` and registered listeners during dedicated-worker message dispatch.
Each callback exception is reported at the worker global before dispatch
continues to later listeners; an uncanceled report is forwarded to the owning
`Worker` in the existing ordered message channel. The contract is limited to
dedicated-worker message events and leaves shared/service-worker and
specialized EventTarget dispatch unchanged. Focused classic/module local
regressions cover cancellation, forwarding order, later listeners, and worker
survival. See the
[slice 789 task](tasks/native-engine-browser-789.md).

Completed locally, slice 790 corrects worker CSP destination selection: root
worker creation uses the `worker-src` → `child-src` → `script-src` →
`default-src` fallback chain, while `importScripts()` and worker module
dependencies use the active worker's script-source policy. Blocked dedicated/
shared worker graphs report worker errors without terminating the page process;
blocked Service Worker registration/update rejects its API promise, and a
blocked update retains the installed registration. Report-only events keep
their effective-directive metadata without changing the request result. Focused
CSP, fixture, JSON-module, and runtime-import tests passed locally. This does
not claim complete CSP/WPT conformance or remote CI; see the
[slice 790 task](tasks/native-engine-browser-790.md).

Slice 791 reports exceptions thrown by SharedWorker `onconnect` and registered
`connect` listeners at the shared worker global before continuing dispatch.
The existing cancelable worker-global `ErrorEvent`, legacy `onerror`
five-argument call, exact-`true` cancellation, and later error/connect
listener delivery are preserved. Unhandled shared-global errors are not
forwarded to each page's `SharedWorker` object; dedicated-worker forwarding
remains its separate rule. A process-backed two-connection regression covers
handler and listener exceptions, cancellation state, later listeners and
connections, port acknowledgements, and absence of owner error events. This
does not cover MessagePort callbacks, Service Worker callbacks, or full worker
error/WPT conformance. The exploratory post-error echo then lacked a
worker-side MessagePort receiver, so its absence did not establish a delivery
failure. Slice 792 adds that receiver and verifies post-error request/reply,
SharedWorker-global MessagePort callback error reporting, and port recovery;
see the [slice 791 task](tasks/native-engine-browser-791.md) and
[slice 792 task](tasks/native-engine-browser-792.md).

Slice 793 reports DedicatedWorker MessagePort callback exceptions at the
worker global before continuing port listeners. Exact-`true` worker `onerror`
cancels forwarding. An uncanceled exception reaches the page `Worker` as one
ErrorEvent, and the port and worker continue to deliver messages. A
process-backed transferred-port test verifies handler/listener order, owner
event fields, replies, and later port use. SharedWorker, Service Worker, page,
and BroadcastChannel behavior remains unchanged. This does not claim full
worker or MessagePort WPT conformance; see the
[slice 793 task](tasks/native-engine-browser-793.md).

Slice 794 reports Service Worker MessagePort callback exceptions at
ServiceWorkerGlobalScope, applies exact-`true` global `onerror` cancellation,
and does not forward those errors to clients. A process-backed transferred-port
regression verifies later listener delivery, replies, subsequent port use,
active worker state, and absence of client-side error events. This focused
slice is not complete Service Worker, MessagePort, or WPT conformance; see the
[slice 794 task](tasks/native-engine-browser-794.md).

Slice 795 reports exceptions from a Service Worker global `onmessage` handler
or registered `message` listener at that global, without forwarding them to
clients. Global `onerror` exact-`true` cancellation, later listener execution,
client replies, a later healthy message, active worker state, and absence of
client error events are covered by a process-backed controlled-page regression.
This focused slice is not complete Service Worker or WPT conformance; see the
[slice 795 task](tasks/native-engine-browser-795.md).

Slice 796 reports synchronous exceptions thrown by Service Worker `onfetch`
and registered `fetch` callbacks at the Service Worker global and continues
later listeners. A subsequent callback may supply `respondWith()`; if no
listener responds, the normal network fallback remains available. Its
process-backed regression verifies error-event cancellation state, callback
order, fallback, worker survival, and a later healthy controlled fetch. This
focused slice does not establish complete Service Worker Fetch or WPT
conformance; see the [slice 796 task](tasks/native-engine-browser-796.md).

Slice 797 reports registered Service Worker `install` and `activate` callback
exceptions at the worker global, continues later listeners, and leaves
`waitUntil()` settlement responsible for lifecycle success or failure. Its
process-backed HTTP regression checks global error-event state/order, both
fulfilled lifecycle actions, activation and control, and absence of page error
events. It does not add `oninstall`/`onactivate` handler attributes or certify
rejected-lifetime, full lifecycle, or WPT conformance; see the
[slice 797 task](tasks/native-engine-browser-797.md).

Slice 798 exposes callable `oninstall` and `onactivate` handlers on the
Service Worker global and runs them in listener-registration order. Replacing
an active handler preserves its slot; clearing and reactivation removes the
old slot and appends a new one. Handler state survives lifecycle bootstrap
re-entry. Process-backed HTTP coverage verifies both callback contexts, the
ordering transitions, non-callable-object no-op conversion, global error
reporting/continuation, fulfilled install/activate waits, worker activation,
page control, and no page error event. Full Web IDL/EventTarget, rejected
lifetime, platform, remote-CI, and WPT conformance remain open; see the
[slice 798 task](tasks/native-engine-browser-798.md).

Slice 799 converts direct worker-global structured-clone decode failures into
`messageerror` delivery, adds the ordered `onmessageerror` EventHandler slot,
reports callback exceptions using the existing owner policy, and preserves
worker operation for later valid messages. Process-backed HTTP regressions
exercise Dedicated Worker and Service Worker dispatch. At that checkpoint the
Service Worker case reused the generic `MessageEvent` shell; Slice 800 now
provides the required `ExtendableMessageEvent`, source/origin metadata, and
extendable lifetime. MessagePort and page-side Worker proxy decode failures
are also outside this slice; see
[slice 799](tasks/native-engine-browser-799.md).

Slice 800 implements native page-to-Service-Worker `message` and
`messageerror` delivery as trusted `ExtendableMessageEvent`s, not
`MessageEvent`s. The event preserves the sending client's projected identity
and serialized origin, exposes the frozen ports array, and extends the worker
turn while active `waitUntil()` promises settle, including continuation-added
promises. Rejections are drained without becoming a synchronous
`postMessage()` exception or client error event. Focused local regressions
pass; this does not certify all Client/WindowClient Web IDL, other message
source kinds, remote CI, or full Service Worker conformance. See
[slice 800](tasks/native-engine-browser-800.md).

Slice 801 implements receive-side MessagePort recovery for host-delivered
clone envelopes that fail structured deserialization: deliver `messageerror`
without exposing partial data or ports, and preserve local and cross-realm
ports for later valid delivery. The process-backed regression passes with
transfer-free malformed input; transferred-resource rollback and complete
MessagePort/EventTarget/WPT conformance remain open. See
[slice 801](tasks/native-engine-browser-801.md).

Slice 802 implements page-side Dedicated Worker message recovery: a
host-delivered clone envelope that fails structured deserialization dispatches
`messageerror` at the Worker object without exposing partial data or ports,
while allowing a later valid Worker message. The focused process-backed
regression passes; transfer rollback and complete Worker/EventTarget/WPT
conformance remain open. See
[slice 802](tasks/native-engine-browser-802.md).

Slice 803 implements Service Worker-to-page client-message recovery:
malformed host-delivered clone data dispatches a `MessageEvent` named
`messageerror` on `navigator.serviceWorker`, retaining sender source/origin
without partial data or ports. Process-backed coverage verifies the handler
and listener plus a later real client message. Transfer rollback and complete
Service Worker/EventTarget/WPT conformance remain open; see
[slice 803](tasks/native-engine-browser-803.md).

Slice 804 implements cleanup of receiver-side `MessagePort` bridge proxies
created during a transfer-containing envelope whose object graph then fails
to deserialize. Process-backed coverage verifies provisional entries are
removed, preexisting endpoints survive, and later valid messages continue. It
does not restore the sender's already-transferred object. Host-route
retirement, remote-peer close notification, other transferables, and complete
MessagePort/WPT conformance remain open; see
[slice 804](tasks/native-engine-browser-804.md).

Slice 805 implements explicit `MessagePort.close()` for same-realm channel
pairs: the initiating endpoint closes, both peer references are removed, and
the still-open peer receives one generic `close` Event through its listener
and `onclose` handler. Process-backed coverage verifies one-shot delivery and
no later cross-pair messages. Cross-realm bridge teardown, GC-driven close,
and full MessagePort/EventTarget/WPT conformance remain open; see
[slice 805](tasks/native-engine-browser-805.md).

Slice 806 implements Page-to-Dedicated/Shared-Worker `MessagePort.close()`
across the Rust-owned route: owner-validated commands retire the route and
pending work, then deliver one generic `close` Event to the surviving
endpoint. The bounded queue reserves one record per live route so a close
cannot retire a route without queue capacity for its peer event. The
process-backed HTTP regression verifies both directions, initiator/peer state,
one shared Event through `onclose` and listeners, queued-message purge,
post-close stop, and independent-channel survival (1 passed, 858 filtered).
The fixture uses a Dedicated Worker; SharedWorker process coverage remains
open. At the Slice 806 checkpoint, Service Worker bridge close remained
outside the slice and explicitly returned an owner-specific error rather than
crossing registry ownership; Slice 807 now implements that separate route.
GC-driven close, full task-source/EventTarget behavior, WPT, remote CI, and
cross-platform certification remain open; see
[slice 806](tasks/native-engine-browser-806.md).

Slice 807 implements `MessagePort.close()` for page-to-Service-Worker bridges
owned by `NativeServiceWorkerRegistry`: retire only the validated route, purge
its pending messages, and deliver one generic close Event to the surviving
endpoint in either direction. The registry reserves bounded capacity for each
live route's close notification. Process-backed HTTP coverage verifies both
directions, event identity/state, repeated close, same-turn queue purge,
post-close suppression, and continued use of an unrelated bridge (1 passed,
859 filtered; 25.23 seconds). Multi-client scheduling,
GC/document-destruction close, full task-source and EventTarget/WPT
conformance, remote CI, and cross-platform certification remain open. See
[slice 807](tasks/native-engine-browser-807.md).

Slice 808 verifies bridge close for three connections to one real HTTP-loaded
SharedWorker. Page- and SharedWorker-initiated close deliver one generic close
Event to the surviving endpoint, while repeated close is idempotent, queued
and later messages on each retired route are suppressed, and the third
connection completes a request/reply after both closures. The process-backed
test confirms all three connections share one runtime; it passed locally (1
passed, 860 filtered; 29.63 seconds), with no registry code change required.
This fills the SharedWorker omission in Slice 806's Dedicated Worker fixture.
SharedWorker lifecycle, cross-page sharing, task-source, full EventTarget/Web
IDL and WPT conformance, remote CI, and cross-platform certification remain
open. See the [slice 808](tasks/native-engine-browser-808.md).

Slice 809 implements session-owned SharedWorker reuse for same-origin
top-level targets in one native browser session. Process-backed pages forward
SharedWorker creation to the backend's shared registry; each target retains
its own page runtime, and MessagePort effects route through live context/frame
owners in both directions. The HTTP regression proves one worker identity,
connections 1 and 2, and cross-target message relay after switching targets
(1 passed, 861 filtered; 36.22 seconds). This does not establish
cross-session/process sharing, last-client destruction, complete storage-key
or agent-cluster matching, or full SharedWorker/WPT conformance. See the
[slice 809 task](tasks/native-engine-browser-809.md).

The completed [slice 810 contract](tasks/native-engine-browser-810.md) closes
the session-level lifecycle gap: target-owned bridge teardown, correctly
routed page/worker close Events, survivor isolation, and final-Document-owner
runtime reaping. Explicitly closing one port while its Document remains active
does not terminate the shared worker. Slice 811 completes the bounded
owner-replacement path: ownership includes the committed Document generation,
committed cross-document navigation and successful recovery retire only the
outgoing Document, and descendant owners destroyed with a replaced ancestor
are retired before replacement worker effects. Same-document navigation
retains its owner, and process evidence verifies surviving and replacement
routes. Slice 812 implements dynamic iframe removal from a still-live parent
Document: detach/reinsert receives a fresh frame identity, removed descendant
contexts and SharedWorker routes are retired, surviving siblings remain live,
and no unload event is delivered. Its process-backed regression passed (1
passed, 864 filtered; 85.59 seconds). Storage partitioning beyond the current
origin key, agent-cluster policy, credential-aware module-worker fetching,
retained detached `WindowProxy`, iframe/WPT, and full SharedWorker conformance
remain open; see the [slice 812 task](tasks/native-engine-browser-812.md) for
exact evidence.

Slice 813 implements SharedWorker reuse by creator storage key, parsed
constructor URL, and name. Type, credentials, and `extendedLifetime` mismatches
dispatch an error to the incoming object without connecting it or disturbing
the incumbent; different names and URLs create separate globals. Its focused
process-backed HTTP regression passed (1 passed, 864 filtered; 42.33 seconds).
The tuple/opaque key-level tests are present but were not executed because the
separate library-test compilation exhausted available host memory. The current
[Storage Standard storage-key algorithm](https://storage.spec.whatwg.org/#storage-keys)
and [HTML Standard constructor algorithm](https://html.spec.whatwg.org/multipage/workers.html#shared-workers-and-the-sharedworker-interface)
remain authoritative. Partitioning beyond the current storage-key definition,
agent-cluster communication policy, and worker lifetime timers remain open.
See the [slice 813 task](tasks/native-engine-browser-813.md).

Slice 814 applies the module SharedWorker `credentials` option across root,
static-dependency, and dynamic-import requests; classic SharedWorker fetches
retain classic behavior. Its process-backed HTTP regression checks `omit`,
`same-origin`, and `include`, redirect response-cookie processing,
cross-origin CORS, failed-load retry, and the classic-worker boundary (1 passed,
865 filtered; 27.07 seconds). The scoped integration-test check passed with
existing dead-code warnings from the legacy HTML parser. Full WPT and
cross-platform coverage remain open. Cross-context merging/propagation and
durable persistence of cookies accepted by session-level SharedWorker fetches
are also not established by this slice. See the
[slice 814 task](tasks/native-engine-browser-814.md).

Slice 815 preserves the session SharedWorker loader's accepted response-cookie
writes and deletions when later worker creations replace its cookie profile
from a stale page snapshot. The real-HTTP process regression passed (1 passed,
866 filtered; 20.52 seconds); the Slice 814 credentials regression also passed
after this change (1 passed, 866 filtered; 24.17 seconds). The scoped check
passed with existing legacy HTML parser dead-code warnings. This is bounded to
the live SharedWorker coordinator and its 128-key change journal; propagation
to already-live page/frame loaders and durable profile persistence remain
separate open requirements. See the
[slice 815 task](tasks/native-engine-browser-815.md).

Slice 816 synchronizes credential-accepted worker response-cookie changes to
the owning live page/frame loader and configured native profile. The focused
process-backed cookie tests passed (2 passed, 866 filtered; 69.53 seconds),
and the Slice 814 credentials/redirect regression passed again (1 passed, 867
filtered; 24.08 seconds). Evidence covers the owning page's next request,
HttpOnly invisibility to script, profile reload, and deletion both immediately
and after another reload. The scoped integration-test check passed with 68
existing legacy HTML parser dead-code warnings. Fan-out to unrelated live
target/frame contexts remained open at that point. Slice 817 fans out accepted
SharedWorker cookie changes to all already-live target and frame processes
within one backend/profile. Its process-backed
group passed (3 passed, 866 filtered; 126.59 seconds), and the Slice 814
credentials/redirect regression passed (1 passed, 868 filtered; 25.05
seconds). Slice 818 returns ordinary content-process response-cookie journals
after profile merge and synchronizes same-backend live target/frame loaders,
content processes, and the SharedWorker loader/journal without duplicate
profile writes. Its process-backed regression passed (1 passed, 869 filtered;
69.98 seconds), verifying the peer and child-frame next requests, HttpOnly
visibility, latest-value coalescing and deletion, independent-profile
isolation, and cookies restored by a newly opened engine. Slice 819 also
passes (1 passed, 869 filtered; 86.02 seconds), proving that a live module
SharedWorker dynamic import after the page response sends the latest ordinary
and HttpOnly cookies and excludes the deletion. The scoped check
passed with existing legacy HTML parser dead-code warnings; formatting and
diff validation passed. Slice 820 synchronizes accepted cookie changes across
separately created live sessions sharing one explicit profile. Its HTTP regression
passed (1 passed, 870 filtered; 46.24 seconds): the receiver's next request
carried the latest ordinary and HttpOnly values, omitted the deleted cookie,
and a distinct profile remained isolated. The journal round-trip/backward-
decode test passed (1 passed, 1,639 filtered; 0.07 seconds). Its operation-
boundary delivery does not interrupt an in-flight request. Broader cookie/WPT
conformance and cross-platform coverage remain open. Slice 821 verifies
process-backed explicit cookie import and clear across live sessions (1 passed,
870 filtered; 45.52 seconds), including a peer-written cookie, HttpOnly
delivery, and separate-profile isolation. Slice 822 fixes missing local
SharedWorker constructor storage-key metadata and exercises direct worker
Fetch over HTTP. Four related process-backed tests passed (68.32 seconds), and
the existing module-import integration test passed separately (18.78 seconds).
Exact behaviors and unverified credentials-mode boundaries are in the
[slice 822 task](tasks/native-engine-browser-822.md). See the [slice 821
task](tasks/native-engine-browser-821.md), the
[slice 820 task](tasks/native-engine-browser-820.md), the
[slice 819 task](tasks/native-engine-browser-819.md),
the [slice 818 task](tasks/native-engine-browser-818.md),
the [slice 817 task](tasks/native-engine-browser-817.md), and
[slice 816 task](tasks/native-engine-browser-816.md).

This is the bounded baseline, not complete focus navigation: shadow scopes,
flat-tree and modal-dialog inertness, browser/platform sequential-focus
preferences, focus-chain handoff, and complete focus Web Platform Test
conformance remain separate profile requirements. See the
[slice 775 task](tasks/native-engine-browser-775.md) and
[slice 776 task](tasks/native-engine-browser-776.md) for local evidence and
exact remaining boundaries.

### XHR document responses

For a Window `XMLHttpRequest` whose response type is `document`, only an HTML
or XML final MIME type produces a response `Document`. HTML bytes use the HTML
parser with scripting disabled and a known definite encoding; XML uses the XML
parser with scripting support disabled and returns null on a well-formedness
or encoding failure. Parsed response documents do not load referenced
resources or apply XSLT. See the [XHR document-response algorithm](https://xhr.spec.whatwg.org/#document-response).

The native HTML-response route uses `encoding_rs`'s WHATWG label table and
replacement decoder for valid response/override MIME labels. If no supported
label is selected, it prescans at most the first 1,024 response bytes for
`meta` charset/pragma declarations, UTF-16 XML prefixes, and XML declaration
fallback, then uses UTF-8. A BOM overrides the chosen encoding. The original
response-byte cap remains in force and decoded source is bounded to three
times that cap before tree construction. Slice 750 verifies common Western,
UTF-16, Shift_JIS, and GBK paths locally; this is not a complete XHR/Encoding
WPT conformance claim. Worker-specific `responseType` behavior remains
separate from the HTML document parser route.

The detached HTML response-document projection preserves each parsed template
contents node as a separate `DocumentFragment`: the template's `.content`
identity is distinct from its children, and selectors on the response document
do not traverse into it. Slices 752 and 753 also preserve fragments in
top-level live navigation documents and same-origin frame documents, exposing
`.content`, inert owner-document identity, query/traversal boundaries,
fragment mutation, and template `innerHTML` replacement. Slice 754 adds shallow
and deep `cloneNode()` template-content cloning, including nested templates,
independent fragment identity, inert owner-document association, and
top-level/frame persistence. Slice 755 implements target-aware `importNode()`
for live top-level, same-origin-frame, and inert template-owner Documents,
including boolean/dictionary depth options and destination inert ownership for
nested template content. The [slice 756 contract](tasks/native-engine-browser-756.md)
implements bounded same-context adoption with identity preservation,
detachment, nested template-owner reassignment, and preflight bounds.
Slice 757 implements identity-preserving transfer among same-origin
top-level/frame Documents in one context tree, including destination-routed
mutations after realm refresh. The [slice 757 task](tasks/native-engine-browser-757.md)
records local verification; worker-loss rollback lacks fault-injection
evidence. Independent top-level/popup transfer and custom-element adoption
callbacks remain open. See the
[slice 751 contract](tasks/native-engine-browser-751.md),
[slice 752 contract](tasks/native-engine-browser-752.md), and
[slice 753 contract](tasks/native-engine-browser-753.md) and
[slice 754 contract](tasks/native-engine-browser-754.md), and
[slice 755 contract](tasks/native-engine-browser-755.md).
The [slice 758 contract](tasks/native-engine-browser-758.md) is complete
locally: autonomous custom-element registry and lifecycle code is implemented,
and template-content identity preservation now passes its focused process-backed
regression after cached fragments are refreshed during snapshot hydration. A
selected-frame regression covers independent top-level/frame registries and
frame-owner lifecycle execution. Another process-backed regression verifies
reentrant definition rejection and recovery, callback-triggered insertion
ordering, FIFO ordering for 300 reactions across queue compaction, bounded
`observedAttributes` iteration with iterator closing, and the 1,024-entry
`whenDefined()` capacity/recovery boundary. A 4,352-reaction process stress
case covers the 4,096 queue cap, explicit overflow reports, and later callback
recovery. A second process stress case re-queues reactions through the
internal scheduler bridge to cover the 10,000-reaction per-checkpoint work
limit, its `RangeError` report, and later callback recovery.
The same slice covers `importNode()` with the owning global registry and
typed invalid/unsupported registry failures. A frame-projected template
hydration fix preserves imported `.content` across the next script call; its
cross-document regression passes locally. Scoped registries are not yet
implemented.
The [slice 758 contract](tasks/native-engine-browser-758.md) completes the
autonomous global registry and lifecycle slice locally, with inline and
process-backed coverage, including same-origin frame registries and
`adoptedCallback` across top-level/frame ownership. The [slice 759
contract](tasks/native-engine-browser-759.md) implements
customized built-ins for the HTML interfaces represented by the native
element-interface table. It distinguishes the built-in local name and
interface from the custom registry name and internal `is` value; an `is`
content attribute alone does not change creation-time registry matching.
Process-backed coverage exercises parser and DOM creation, direct construction,
serialization, clone/import, lifecycle, selected frame registries, and native
button behavior. Unmapped built-in interfaces, scoped registries, custom
states, and full Web Platform Tests remain profile requirements.
The [slice 760 contract](tasks/native-engine-browser-760.md) implements
bounded form-associated autonomous custom elements: `attachInternals()`,
`setFormValue()`, and ordered `FormData(form)` construction. In-process and
content-process regressions pass, including script-refresh persistence. It
explicitly does not claim form lifecycle callbacks, native HTML form-navigation
submission, custom validity, accessibility semantics, or custom states.
The [slice 761 contract](tasks/native-engine-browser-761.md) implements
`formAssociatedCallback` and `formDisabledCallback`, with in-process and
content-process coverage for upgrade, owner/disabled transitions, and refresh.
Reset and state-restore callbacks were fail-closed until slice 762 added the
native reset path. Slice 762 implements cancelable reset, supported control
defaults, and ordered `formResetCallback()` reactions across top-level and
same-origin frame documents; local and content-process regressions cover
cancellation, reentrancy, callback exceptions, control-state persistence, and
reset defaults. Reset-button activation, state restoration, and complete
form-control Web IDL reflection remain profile requirements. Slice 763
implements native activation of form-associated reset buttons through semantic clicks,
JavaScript `.click()`, and same-origin frame routing. Local tests cover event
cancellation, form-owner changes, and no submit/validation/navigation.
Process-backed keyboard-generated button activation is implemented in
[slice 764](tasks/native-engine-browser-764.md), with inline/local parity in
[slice 765](tasks/native-engine-browser-765.md). Together these bounded slices
cover Enter/Space phases, cancellation, reset and submit defaults, and native
button target eligibility across Glass action paths. Slice 765's local form
navigation evidence uses configured `fixture://` URLs and the existing
`NativeResourceLoader` form-action policy; it does not establish external
HTTP(S) navigation or full keyboard conformance. See the
[slice 762](tasks/native-engine-browser-762.md) and
[slice 763](tasks/native-engine-browser-763.md) contracts.
Slice 766 implements Enter activation for focused native `<a href>` hyperlinks
through the existing click and navigation owners in local, process-backed, and
same-origin frame paths. Regressions cover cancellation, live post-click
`href`, event order, and Space non-activation. The profile remains unsatisfied
for link modifier gestures, image-map areas, complete keyboard conformance, and
the broader Core Web Profile; see the [slice 766 task](tasks/native-engine-browser-766.md).
Slice 767 verifies that Enter activation preserves native hyperlink default
actions: local and process-backed `_blank` links keep the opener active and
create an opener-owned target, canceled clicks suppress that target, and
download links retain their suggested filename and bytes without navigating
the opener. Target initialization runs in a separate Tokio task so popup
layout does not inherit the initiating action's stack. See the
[slice 767 task](tasks/native-engine-browser-767.md). Modifier gestures,
image-map areas, and complete keyboard conformance remain open.

For primary-button hyperlink activation, GCWP adopts a Glass-owned modifier
convention: Control, Meta, or Shift requests one new background browsing
context; the initiating context remains selected. This is a product-level
mapping, not a requirement of the HTML hyperlink algorithm. The click event
must expose the caller's Alt/Control/Meta/Shift state before default handling,
and canceling that event suppresses context creation. Existing `download`
attribute behavior and URL/security policy take precedence. Alt alone does not
implicitly download an arbitrary link. Because Glass exposes browsing contexts
rather than desktop windows/tabs, Shift uses the same background-target model
as Control and Meta. Right/middle-button gestures remain outside this slice.
Slice 770 implements the modifier-aware action path across local,
process-backed, and same-origin-frame behavior; focused local tests pass. See
[`native-engine-browser-770`](tasks/native-engine-browser-770.md) for exact
evidence. This is not complete pointer-input conformance.

Slice 771 brings client-side `<img usemap>` regions into native point hit
testing and link activation. Map association, normalized shape geometry,
topmost-area selection, click cancellation, live link defaults, downloads,
background targets, and `alt`-derived accessible names follow the existing
document and navigation owners across local, HTTP(S) content-process, and
same-origin-frame paths; see
[`native-engine-browser-771`](tasks/native-engine-browser-771.md). Complete
area keyboard, WPT, platform, and remote-CI conformance remain open.

Slice 772 completes linked image-map sequential focus and Enter activation
through per-image DOM focus anchors and the ordinary cancelable area-click/
link-default path. Local, HTTP(S) content-process, and same-origin-frame
evidence is tracked in
[`native-engine-browser-772`](tasks/native-engine-browser-772.md). Complete
focus-chain, keyboard, WPT, platform, and remote-CI conformance remain open.

Slice 773 extends the current light-DOM focus scope to rendered elements with
valid explicit `tabindex`, regardless of interactive semantic role. Positive
values precede natural order; negative values permit programmatic focus but
remain outside sequential navigation. Supported native disabled controls and
non-rendered targets stay excluded, and image-map areas keep their image-anchor
path. Local, HTTP(S) content-process, and same-origin-frame evidence is tracked
in [`native-engine-browser-773`](tasks/native-engine-browser-773.md). This does
not complete focus-chain, shadow-DOM, inertness, keyboard, WPT, platform, or
remote-CI conformance.

Slice 774 adds `tabIndex` getter/setter reflection to the native element
projection. Explicit signed-long values are read from `tabindex`; absent,
invalid, and out-of-range values use the element-specific HTML default. The
setter converts to Web IDL `long` and writes through the existing live
attribute-mutation path. Local, HTTP(S) content-process, and same-origin-frame
evidence is in [`native-engine-browser-774`](tasks/native-engine-browser-774.md).
Full focus navigation and Web IDL conformance remain open.

Slice 768 implements the bounded keyboard-input contract: unmodified
Space on a focused, enabled native checkbox or radio synthesizes click
activation on keyup through the shared checkable-control owner. It covers
cancellation-safe checked state, checkbox indeterminateness clearing, radio
non-toggle/group semantics, and bubbling input/change effects for accepted
activation across local, process-backed, and same-origin-frame paths. The
scoped package check passed; the task records the focused test setup correction
and exact rerun alongside the initial group results. See the
[slice 768 task](tasks/native-engine-browser-768.md). This remains a narrow
checkpoint, not complete keyboard or form conformance.

For native radio inputs, the profile adopts the WAI-ARIA Authoring Practices
radio-group keyboard convention: Right/Down select the next enabled member,
Left/Up select the previous member, focus follows selection, and navigation
wraps within the HTML radio group. A canceled keydown suppresses the default;
actual selection changes emit bubbling `input` then `change`. This is an
explicit Glass profile choice because the HTML Standard defines radio groups
and activation events but not directional-key mapping. Slice 769 implements
this behavior through the shared key-default owner across local,
process-backed, and same-origin-frame routes. It does not cover
toolbar-specific or ARIA-authored radio widgets; see
[slice 769](tasks/native-engine-browser-769.md).
Slice 764 implements keyboard activation for focused native button controls
in process-backed network documents: Enter activates on keydown and Space on
keyup after an uncanceled keydown. The shared click/default path handles reset
and submit controls in top-level and same-origin child Documents. Slice 765
adds matching in-process/local action routing and fixture-backed form
navigation evidence. Exact process-backed and local boundaries are recorded
in the [slice 764 task](tasks/native-engine-browser-764.md) and
[slice 765 task](tasks/native-engine-browser-765.md).
### Synchronous JavaScript dialog contract

The native page realm implements the HTML Standard's modal user-prompt
behavior for `alert()`, `confirm()`, and `prompt()`:

- `alert()` blocks the invoking script until accepted, then returns `undefined`;
  its no-argument overload uses the empty message, while
  `alert(undefined)` and `alert(null)` convert to `"undefined"` and `"null"`.
  Dialog text normalizes line endings and is byte-bounded before IPC.
- `confirm(message)` blocks the invoking script and returns `true` on accept
  or `false` on dismiss.
- `prompt(message, defaultValue)` blocks the invoking script and returns the
  accepted response string (initially `defaultValue`) or `null` on dismiss.
- A pending modal pauses subsequent page-script execution and task/microtask
  progress for that page. It does not block the Glass control plane: callers
  can inspect and resolve the exact pending dialog while the original browser
  operation remains pending. The script resumes once, in place; replaying it
  from the beginning or returning placeholder `false`/`null` values is not
  equivalent.
- Rust embedders opt into this blocking path with an explicit modal-enabled
  native-session constructor and a cloneable dialog controller. The controller
  reads and resolves a target/frame-owned dialog by its exact identity without
  acquiring the serialized page-operation lock. Ordinary native constructors
  retain the nonblocking dialog-event path when no responsive controller is
  installed; they must not strand a caller at a synchronous host callback.
- The resident Glass `BrowserService` exposes the same process-backed control
  out of band from its serialized browser command worker. `modalDialogs` is an
  explicit opt-in and remains disabled until the calling host has a responsive
  dialog surface; the default service session therefore retains nonblocking
  dialog-event behavior.
- Resolution is tied to the owning page target, frame, and current dialog
  identity. Navigation, target closure, worker failure, timeout, and explicit
  cancellation must release the suspended operation without applying a stale
  answer or committing partial state.

The method semantics follow the [HTML Standard's simple-dialog algorithms].
The profile does not replace user prompts with no-op defaults merely because a
caller selected the native backend.

[HTML Standard's simple-dialog algorithms]: https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#simple-dialogs

### Before-unload confirmation contract

Cross-document navigation, including asynchronous `BrowserSession` Back and
Forward traversal, dispatches `beforeunload` on the outgoing active Document
before `pagehide`, `unload`, replacement-resource requests, or history commit.
A canceled event (`preventDefault()` or a non-empty `returnValue`)
does not itself cancel navigation: Glass asks the user only when that Document
has sticky activation and the modal prompt is permitted. Script-created events
and `HTMLElement.click()` do not grant activation; trusted input delivered by
Glass's browser action path does. A successful document replacement resets the
new Document's activation state; a same-document navigation or dismissed
confirmation preserves the existing Document and its state.

The prompt is a `beforeunload` pending dialog with no page-controlled message
or response text. Glass presents user-agent-controlled generic copy, ignores
the page's `returnValue` text, and binds the decision to the exact dialog ID.
Accepting continues the original navigation exactly once. Dismissing leaves
the outgoing browsing-context subtree active and prevents any affected
Document's `pagehide`/`unload`, replacement requests, and history commit. If
an eligible prompt has no responsive host, navigation fails explicitly and
leaves the outgoing documents active; it must not silently accept, cancel, or
strand the owner. One navigation shares a single prompt budget across the
outgoing frame and its active descendants. A canceled event with no sticky
activation, with sandboxed modals, or after a prompt has already been shown
does not open another prompt and does not by itself cancel navigation. Every
affected `beforeunload` still runs before any affected `pagehide`/`unload`;
unload events run child-before-parent. The canonical asynchronous
`BrowserSession` history route applies this decision before loading a
cross-document target; same-document traversal remains in-place. Synchronous
`NativeEngine` history helpers must not bypass this lifecycle: local documents
run the outgoing events synchronously, failing explicitly if an activated
canceled event needs a user decision; a process-backed document must use the
asynchronous history API. A cross-document `HistoryGo` surfaced inline while
an outgoing process-backed lifecycle callback is active is rejected with a
typed re-entry error before nested traversal or history-selection mutation;
page History API commands outside outgoing lifecycle dispatch retain their
queued handling. Both history-API requirement paths preserve the active entry
and avoid loading a replacement.

For native iframe children, a `sandbox` attribute without the
ASCII-case-insensitive `allow-modals` token adds the sandboxed-modals
restriction to a newly active child Document. Changing the iframe's attribute
does not retroactively change the current Document; its next cross-document
navigation computes the replacement's restriction from the current token set
and inherited ancestor restrictions. A nested `allow-modals` token cannot
clear an inherited restriction. [Slice 743](tasks/native-engine-browser-743.md)
implements and locally verifies the shared-tree behavior for its specified
native navigation routes. Remote CI and cross-platform certification remain
open. This modal contract does not imply promotion of the rest of iframe
sandbox security.

The firing conditions and user-activation rule follow the [HTML Standard's
unloading-document algorithm]; the sandbox restriction follows the [iframe
sandbox token contract].

[HTML Standard's unloading-document algorithm]: https://html.spec.whatwg.org/multipage/browsing-the-web.html#unloading-documents
[iframe sandbox token contract]: https://html.spec.whatwg.org/multipage/iframe-embed-object.html#attr-iframe-sandbox

### Dynamic module loading contract

- Every `ImportCall`, including a literal specifier, resolves only when the
  call executes; module-graph discovery must not issue a speculative dynamic
  target request. Literal and runtime-valued specifiers use the same bounded
  host fetch queue.
- `ImportCall` options follow ECMAScript `EvaluateImportCall` ordering:
  evaluate the specifier expression and then the options expression; convert
  the specifier to a string; read `options.with`; enumerate its own enumerable
  string keys and read their values; then validate supported attributes. This
  profile supports the `type` attribute with value `json` and the default
  JavaScript type. Invalid options, getter/conversion errors, unknown keys,
  non-string values, and unsupported module types reject before fetching.
- Module identity is the resolved request URL plus type. A JavaScript import
  and JSON import of the same URL do not alias; redirects retain the request
  key while JavaScript descendants use the final response URL.
- A Document applies its import map using the active script's URL as referrer,
  then fetches the module asynchronously through the document's security and
  resource policies. Request URLs retain module identity; final response URLs
  provide descendant bases.
- Module workers use their own worker module map and URL/referrer rules. A
  Document import map is never exposed as a worker capability.
- A classic script loaded by `importScripts()` keeps its own response URL as
  the active-script base for `import()`. A redirected imported script resolves
  relative module specifiers from its final response URL, not the Worker entry
  URL; the host uses the owning Worker module loader and no Document import
  map.
- In a `ServiceWorkerGlobalScope`, dynamic `import()` rejects with a
  `TypeError` under the HTML Standard's `HostLoadImportedModule` algorithm.
  This applies to classic entry scripts and classic sources loaded through
  `importScripts()`, and to module entry scripts and their static dependencies.
  A dynamic-import target must not be fetched or speculatively prefetched;
  static `importScripts()` dependencies and static module imports remain
  supported.
- A rooted-file Document resolves runtime-valued imports using the active
  script/module referrer and current Document import map, then admits only
  configured-root `file:` resources through the local module loader. Loaded
  modules retain request-URL identity and response-URL descendant bases;
  missing, unsupported, and out-of-root targets reject the original import
  promise without escaping the file root or entering network transport.
- Slice 722 installs parser-sourced CSP meta policies for rooted-file
  Documents before their local subresources load. `script-src-elem`,
  `script-src`, and `default-src` govern inline scripts, classic/module roots,
  static dependencies, and invoked dynamic imports. This file profile binds
  `'self'` to the most-specific admitted file root; an explicit `file:` source
  cannot bypass root admission. Denied script bytes are rejected before read.
  Other file resource classes, report-only file policies, and full CSP
  conformance remain open. Slice 725 implements runtime meta insertion: its
  runtime callback captures a connected head policy before a later
  same-evaluation inline script, and the document ledger applies it to later
  resource checks. See
  [task 722](tasks/native-engine-browser-722.md).
- Slice 723 extends that file-document contract to initial and dynamic
  external stylesheet links and recursive local CSS imports. It applies the
  style-source fallback, same-root `'self'`, link nonce metadata, and pre-read
  rejection. Focused loader and process-backed tests plus the existing local
  stylesheet regressions pass. Inline style sources were a separate gap at the
  end of slice 723 and are now covered by slice 724; other file resource
  classes remain separate. See [task 723](tasks/native-engine-browser-723.md).
- Rooted-file inline style enforcement uses separate `style-src-elem` and
  `style-src-attr` fallback chains, each falling back through `style-src` to
  `default-src`. Element nonces/hashes and attribute `'unsafe-hashes'` follow
  the shared CSP source matcher; a nonce never authorizes a style attribute.
  The policy applies to parser-created styles and committed runtime mutations,
  and remains conjunctive across policies. Slice 724 implements this contract;
  changed style blocks rebuild from authorized sources. Dynamic CSP meta
  insertion and report-only file policies remain separate; see
  [task 724](tasks/native-engine-browser-724.md).
- For rooted-file Documents, runtime `Content-Security-Policy` meta elements
  connected under `head` append their captured `content` policy to the active
  policy container. Subsequent content checks use the additional conjunctive
  policy; changing or removing the meta element cannot relax it. Meta elements
  outside `head` are ignored. Slice 725 wires this contract into the direct
  local-document mutation path; report-only file policies and complete CSP
  conformance remain open. See [task 725](tasks/native-engine-browser-725.md).
- For rooted-file Documents, `img-src`, `font-src`, and `media-src` govern
  file-backed image, font, and media subresources, respectively, with
  `default-src` fallback. `'self'` is scoped to the Document's
  most-specific configured root; explicit file sources remain bounded by
  configured-root admission. Runtime-inserted policies apply before later
  resource bytes are read. Slice 726 implements and tests these file-resource
  checks, including same-evaluation runtime policy delivery; other resource
  classes, file-origin data/blob policy, report-only file policies, and
  complete CSP conformance remain open. See
  [task 726](tasks/native-engine-browser-726.md).
- Slice 727 implements and tests the rooted-file contract for embedded frame
  navigations.
  The current enforced policy container is checked before selecting the child
  document URL; `frame-src` falls back through `child-src` to `default-src`,
  policies remain conjunctive, and `'self'` means a file admitted by the
  Document's most-specific configured root. Explicit file sources remain
  subject to configured-root admission. Runtime-added head policies govern
  frames created after capture; a denied frame follows the existing
  `about:blank` blocked-frame behavior, while an explicit `about:blank` frame
  remains an allowed initial empty document ([WPT coverage][wpt-about-blank]).
  This slice does not claim report-only file policy or complete CSP
  conformance. See
  [task 727](tasks/native-engine-browser-727.md).
- Local implementation evidence: slices 713–717 cover runtime-valued imports
  in page classic/module scripts, dedicated classic/module Workers, and
  classic/module SharedWorkers, including nested computed imports through their
  host fetch queues. Slice 715 also covers rejection and connected-port
  settlement. Slice 716 covers computed imports in statically preloaded
  classic `importScripts()` dependencies against their final response URLs,
  including nested imports, dedicated/shared settlement, and local fixtures.
  Slice 717 covers initial rooted-file classic/module scripts, dynamically
  attached classic/module scripts, an installed Document import map, nested
  imports, duplicate module identity, and out-of-root rejection. Rooted-file
  parser-sourced import-map initialization is implemented in slice 718: maps
  are registered in parser order and govern mapped static and runtime imports
  through the rooted loader; the focused fixture covers later mappings,
  conflict preservation, malformed-map isolation, nested modules, and
  out-of-root rejection. Slice 719 enforces Service Worker dynamic-import
  rejection for classic entry/importScripts sources and module
  entry/static-dependency sources. The process-backed test confirms
  `TypeError` settlement, argument evaluation, static dependency availability,
  no prefetching, no request for uncached dynamic targets, and no additional
  request when a dynamic target is already in the static module graph. Slice
  720 implements static JSON import attributes for page, Worker, Service
  Worker, and configured-root file graphs. Slice 721 implements
  invocation-driven literal and runtime-valued imports, dynamic JSON options,
  typed identity, and spec-ordered option conversion across those owners.
  Other module types and complete module scheduling remain unverified. See
  [task 713](tasks/native-engine-browser-713.md),
  [task 714](tasks/native-engine-browser-714.md),
  [task 715](tasks/native-engine-browser-715.md),
  [task 716](tasks/native-engine-browser-716.md),
  [task 717](tasks/native-engine-browser-717.md),
  [task 718](tasks/native-engine-browser-718.md), and
  [task 719](tasks/native-engine-browser-719.md),
  [task 720](tasks/native-engine-browser-720.md),
  [task 721](tasks/native-engine-browser-721.md), and
  [task 722](tasks/native-engine-browser-722.md).
- Supported dynamic-import options (`type: "json"`) and bounded nested
  imports follow the selected ECMAScript and HTML host algorithms. Unsupported
  module types reject explicitly. Promise/microtask ordering and module
  evaluation errors must remain consistent with those algorithms; a bounded
  implementation must report unsupported profile behavior rather than
  silently returning a partial namespace.

## Explicit exclusions

The following are outside `GCWP-0.1` unless a later profile revision adds them:

- Flash, NPAPI/PPAPI, obsolete plugins, browser extensions, and vendor-specific
  account services;
- proprietary DRM/Widevine and codecs whose licensing or platform contract is
  not available to Glass;
- unstable draft APIs and browser-internal privileged pages;
- OS browser chrome that belongs to Glass rather than the page engine;
- undocumented compatibility quirks that cannot be represented in a stable
  test or security contract.

An exclusion is not permission to misrender or silently skip a required
dependency. If an excluded feature is encountered, the engine must produce a
typed diagnostic, preserve origin/security invariants, and keep the rest of
the session recoverable.

## Supported platform matrix

The promotion matrix is:

| Platform | Required release target | Required evidence |
| --- | --- | --- |
| Linux | x86_64 GNU/Linux, headless software path | native-only conformance, security, package, and binary gates |
| macOS | x86_64 and arm64 where the existing release matrix supports them | native-only conformance, capture, package, and binary gates |
| Windows | x86_64 MSVC, headless/software path first | native-only conformance, process isolation, package, and binary gates |

GPU acceleration is an optimization, not a prerequisite for the first
promotion. The software/headless path must be complete and deterministic;
GPU paths must not change security or semantic behavior.

## Conformance and acceptance thresholds

The pinned WPT manifest and its revision are a later checked-in artifact under
the BE-09 conformance task. It must contain stable test IDs, platform results,
and exclusion reasons; an aggregate pass percentage alone is insufficient.

Promotion requires all of the following:

- 100% of mandatory URL, origin, security, HTML-parser, and DOM-invariant
  tests;
- at least 98% in every selected stable profile suite, with zero unexplained
  failures and no skipped test counted as a pass;
- a versioned synthetic/real-site corpus covering navigation, redirects,
  script/modules, asynchronous waits, DOM/AX, actions, frames/popups,
  storage, upload/download, dialogs, permissions, screenshots, and recovery;
- differential traces and visual/geometry results against at least one
  reference browser, with every difference classified as expected profile
  behavior, an approved exclusion, infrastructure, or a defect;
- reproducible native-only results on Linux, Windows, and macOS from clean
  release artifacts; and
- security, memory, CPU, startup, navigation, cancellation, and crash-recovery
  budgets recorded before the final promotion run.

The thresholds are fixed for `GCWP-0.1`. A failing result is fixed, explicitly
excluded through a profile revision, or a release blocker; it is never hidden
by changing the threshold after the run.

## Performance and resource budgets

BE-00 establishes measurement rules; BE-09 records hardware-specific baselines
and promotion results. Every benchmark reports cold and warm native startup,
time to first DOM, time to interactive for the corpus, screenshot latency,
steady-state RSS, peak RSS, CPU time, cancellation latency, and target growth.

The first promotion target is:

- no unbounded memory growth across a repeated corpus run;
- cancellation and close complete within the declared operation timeout and
  leave no child process or socket behind;
- native warm navigation and capture remain within 2x the Chromium reference
  for the same synthetic corpus, unless a profile record explains the cost;
- native steady-state RSS remains within 1.5x the reference for the same
  scenario, with content-process and browser-process usage reported
  separately; and
- build configuration remains stable, with no routine `cargo clean` or
  target-wide rebuild used to obtain a result.

These are promotion budgets, not current implementation claims.

## Security boundary

Before promotion, page content is treated as hostile. The design must provide
an out-of-process content boundary, OS-appropriate sandboxing, origin/site
isolation, permission mediation, bounded IPC, resource quotas, crash
containment, and redacted diagnostics. Network credentials, cookies, page
secrets, evaluated source, and downloaded content must not enter ordinary logs.

The in-process local engine is retained for development and deterministic unit
tests only. It cannot satisfy `runtime` or `network-origin` production gates by
itself. Linux content workers now require Bubblewrap 0.8 or later and disable
and assert against nested user-namespace creation; this is defense in depth,
not complete OS sandboxing or a production security certification.

## Revision policy

`GCWP-0.1` is immutable once the first conformance run begins. A profile
revision must:

1. add a new profile identifier;
2. record added, removed, or reclassified capabilities and exclusions;
3. update the WPT manifest and real-site corpus;
4. preserve old conformance evidence as historical; and
5. update issue #40, the architecture doc, the task records, and user-facing
   capability documentation in one change.

No implementation slice may claim browser completeness by changing this
profile retroactively.
