# Native browser engine

Status: Browser-complete expansion is active through the completed
`native-engine-browser-215` slice; the bounded foundation below remains
experimental until the issue #40 production gates pass.
Current foundation scope: Phase 2 semantic DOM/interaction slices, initial Phase 3
presentation/layout/display-list/software-surface/PNG-capture/box-model/
viewport-scroll/side-specific-border/bounded-pattern-border/bounded-corner-radius/
bounded-inline-flow/bounded-fixed-line-height/bounded-direct-text-flow/
bounded-word-wrap/bounded-physical-box-edges/bounded-whitespace-boundaries/
bounded-overflow-hit-test-projection/bounded-css-diagnostics/bounded-pixel-golden-capture/
bounded-descendant-selectors/bounded-overflow-clip/bounded-hard-line-breaks/
bounded-pre-line-breaks/bounded-preformatted-whitespace/
bounded-pre-wrap-whitespace/bounded-nowrap-whitespace/bounded-inherited-line-height/
bounded-clip-aware-root-overflow/bounded-axis-specific-overflow/
bounded-min-max-dimensions/bounded-opacity-groups/bounded-text-alignment/
bounded-functional-alpha-colors/bounded-fixed-cell-text-decoration/
bounded-inherited-text-transform/bounded-first-line-text-indent/
bounded-explicit-text-indent-inherit/
bounded-inherited-word-spacing/bounded-inherited-letter-spacing/
bounded-inherited-font-weight/bounded-inherited-font-style/
bounded-inherited-word-break slices,
bounded-text-overflow/bounded-vertical-align/bounded-flex-row/
bounded-grid-track-layout/
bounded-inline-svg-shape-paint/
bounded-inline-svg-stroke-paint/
bounded-svg-line-polygon-paint/
bounded-straight-svg-path-paint/
bounded-svg-curve-path-paint/
bounded-dom-namespace-identity/
bounded-namespace-qualified-attributes/
bounded-flex-row-gap/bounded-flex-row-justification/bounded-flex-item-order/
bounded-flex-cross-axis-alignment/bounded-flex-direction/bounded-flex-column-direction/
bounded-flex-wrap/
bounded-flex-wrap-reverse,
bounded-flex-column-wrap/bounded-flex-column-wrap-reverse,
bounded-flex-cross-line-alignment/bounded-flex-cross-line-space-around/
bounded-flex-cross-line-space-evenly/bounded-flex-cross-line-stretch/
bounded-flex-cross-line-normal/bounded-flex-cross-line-gap,
bounded-flex-gap-family/bounded-flex-gap-family-revert-layer/
bounded-flex-gap-family-css-wide-resets,
bounded-flex-sizing-css-wide-resets,
bounded-flex-flow-css-wide-resets,
bounded-flex-order-css-wide-resets,
bounded-flex-justify-content-css-wide-resets,
bounded-flex-align-items-css-wide-resets,
bounded-flex-align-self-css-wide-resets,
bounded-flex-align-content-css-wide-resets,
bounded-flex-place-content-css-wide-resets,
bounded-inherited-text-presentation-revert-layer,
bounded-inherited-text-spacing-revert-layer,
bounded-inherited-vertical-align-revert-layer,
bounded-local-text-geometry-revert-layer,
bounded-local-dimension-revert-layer,
bounded-local-text-indent-overflow-css-wide-resets,
bounded-local-box-model-revert-layer,
bounded-paint-color-revert-layer,
bounded-paint-color-important/bounded-physical-border-color-important,
bounded-logical-border-color-important/bounded-text-presentation-important/
bounded-local-presentation-important,
bounded-flex-gap-important/bounded-dimension-important/bounded-box-model-important/
bounded-logical-box-model-edges/
bounded-box-model-css-wide-resets/
bounded-box-model-explicit-inheritance/bounded-dimension-explicit-inheritance/
bounded-dimension-css-wide-resets/bounded-inherited-text-css-wide-resets/
bounded-overflow-important,
bounded-overflow-inherit,
bounded-overflow-css-wide-resets,
bounded-overflow-no-clip-keywords,
bounded-physical-border-width-important,
bounded-physical-border-style-important,
bounded-inherited-color-current-color,
bounded-inherited-color-css-wide-keywords,
bounded-overflow-revert-layer,
bounded-border-radius-revert-layer,
bounded-display-visibility-revert-layer,
bounded-border-revert-layer,
bounded-border-color-revert-layer,
bounded-border-color-current-color,
bounded-border-color-css-wide-keywords,
bounded-border-width-css-wide-keywords,
bounded-border-style-css-wide-keywords,
bounded-border-radius-css-wide-keywords,
bounded-complete-border-css-wide-keywords,
bounded-logical-border-family,
bounded-physical-border-radius-corner-longhands,
bounded-logical-border-radius-corner-longhands,
bounded-flex-auto-margins,
bounded-flex-wrapped-auto-margins,
bounded-flex-directionality/bounded-final-line-alignment/bounded-final-line-justification,
bounded-text-justification-control/bounded-text-decoration-lines/
bounded-text-decoration-combinations,
bounded-text-alignment-revert-layer,
bounded-inherited-alignment-css-wide-resets,
bounded-inherited-text-decoration-style-css-wide-resets,
bounded-inherited-text-decoration-thickness-css-wide-resets,
bounded-inherited-text-decoration-skip-ink-css-wide-resets,
bounded-inherited-text-decoration-line-css-wide-resets,
bounded-text-decoration-skip-spaces-line-edges,
bounded-text-decoration-skip-spaces-initial/bounded-text-decoration-skip-spaces-inherit/
bounded-text-decoration-skip-spaces-unset/bounded-text-decoration-skip-spaces-revert,
bounded-cascade-layers/bounded-text-decoration-skip-spaces-revert-layer/
bounded-text-decoration-skip-ink-revert-layer/
bounded-text-decoration-color-revert-layer,
bounded-text-decoration-color-current-color,
bounded-text-decoration-color-css-wide-keywords,
bounded-text-decoration-line-revert-layer,
bounded-base64-data-url/bounded-fragment-navigation-history/
bounded-local-link-activation/bounded-fragment-target-scroll/
bounded-relative-local-links/bounded-percent-decoded-fragment-targets/
bounded-legacy-name-fragment-targets/bounded-text-fragment-targets/
bounded-text-fragment-affixes/bounded-root-horizontal-scroll/
bounded-prompt-lifecycle/bounded-request-lifecycle/bounded-external-link-activation/
bounded-anchor-download-ownership/bounded-native-target-lifecycle/
bounded-style-declaration-surface/bounded-dataset-surface,
including bounded style
inheritance, paint clipping, solid/dashed/dotted border painting, rounded
fill/border masks, inline-box line placement, fixed pixel line-height floors,
and content-box geometry,
bounded base64 data-URL loading, plus feature-gated
runtime/CLI integration; not a stable browser compatibility or security
boundary.
This document is the repository contract for the Glass-owned native browser
engine described by [issue #40](https://github.com/wanazhar/glass/issues/40).
The current implementation is an experimental backend inside `glass-browser`;
the expanded goal is a production-capable native browser engine that can
replace the CDP path for the declared Glass Core Web Profile. It is not a
third crate, a protocol adapter, or an embedded copy of another browser.

## Browser-complete expansion target

Issue #40 now has two explicit boundaries. The completed bounded slices are
the kernel and evidence-producing foundation. The new completion boundary is
normal web browsing and Glass automation without a Chromium process, a CDP
endpoint, or a silent fallback when the native backend is selected.

“Browser-complete” means complete for a versioned Glass Core Web Profile, not
an unbounded promise to implement every obsolete, proprietary, experimental,
or vendor-service feature. The profile must cover external HTTP(S) navigation,
standards HTML parsing and DOM behavior, ECMAScript execution and Web IDL
bindings, the selected stable CSS/layout profile, rendering and hit testing,
accessibility, input, frames/tabs/popups, history, storage, cookies,
downloads/uploads, prompts, permissions, and the origin/security model needed
to run hostile web content. Unsupported features outside the profile must be
typed, documented, versioned, and never silently treated as supported.

The standards and compatibility anchors are the
[WHATWG HTML Standard](https://html.spec.whatwg.org/multipage/),
[WHATWG Fetch Standard](https://fetch.spec.whatwg.org/),
[W3C CSS Snapshot](https://www.w3.org/TR/css/),
[ECMAScript](https://tc39.es/ecma262/), and the
[Web Platform Tests](https://web-platform-tests.org/). Mature Rust or isolated
system components may provide primitives, but Glass must own the browser
integration, security policy, lifecycle, observable semantics, and stable
backend contract. “From scratch” does not require reimplementing every
cryptographic primitive or JavaScript VM if doing so would make the engine
less safe or less interoperable.

The versioned implementation contract is
[`GCWP-0.1`](../plan/native-engine-browser-profile.md). New work must name the
profile capability family and milestone it advances; a bounded fixture slice
cannot claim browser-complete status by itself.

The first executable browser-complete batch is recorded in
[`native-engine-browser-001`](../plan/tasks/native-engine-browser-001.md). Its
`NativeRuntime` owns runtime state, typed task/microtask ordering, cancellation,
bounded privacy-safe traces, startup rollback, and terminal close. It is still
synchronous and in-process; later BE-01 work must add asynchronous workers,
IPC, crash recovery, and the production content-process boundary.

The next executable network batch is recorded in
[`native-engine-browser-002`](../plan/tasks/native-engine-browser-002.md). It
adds an asynchronous, bounded HTTP(S) HTML document loader and routes native
session initialization/navigation through it. Redirects, HTML MIME checks,
UTF-8 decoding, response-size limits, credential rejection, final URL
preservation, and tuple-origin construction are covered. This is not yet the
BE-02 security milestone: subresources, charset sniffing, cookies/cache,
CORS/CSP, mixed-content policy, permissions, and process isolation remain
open.

The following runtime batch is recorded in
[`native-engine-browser-003`](../plan/tasks/native-engine-browser-003.md). It
adds a bounded typed Tokio worker over the single runtime state and routes
asynchronous native initialization/navigation commits through it. Worker
cancellation, trace access, and crashed-channel errors are explicit. This is
still an in-process boundary; content-process isolation, OS sandboxing,
supervisor restart, and cross-process quotas remain open.

The completed process-control batch is recorded in
[`native-engine-browser-004`](../plan/tasks/native-engine-browser-004.md). It
adds a second `glass-browser` binary, `glass-native-content-worker`, and a
bounded request-ID-correlated length-framed IPC channel. External HTTP(S)
initialization/navigation now requires ping/start/commit acknowledgements and
backend shutdown uses a bounded close acknowledgement. This is a liveness and
ownership boundary only: the parent still fetches and constructs the document,
so resource transfer, content execution isolation, OS sandboxing, supervisor
recovery, and cross-process quotas remain open.

The completed resource-transfer batch is recorded in
[`native-engine-browser-005`](../plan/tasks/native-engine-browser-005.md). The
helper now invokes the shared bounded HTTP(S) loader and returns final URL plus
base64-encoded typed DOM through the capped IPC frame. The child owns bounded
HTML tree construction; the parent validates request ID, URL, origin, snapshot
links, and quotas before reconstructing the arena. Load deadlines and
malformed-transfer failures poison the child without CDP fallback. Stylesheet
source parsing, computed style, and DOM mutation still run in the parent, so
this remains a partial content-process boundary rather than full isolation.

The completed computed-style batch is recorded in
[`native-engine-browser-006`](../plan/tasks/native-engine-browser-006.md).
External child snapshots now include one typed computed-style record per DOM
node. The parent validates the cardinality and uses the immutable cache for
layout, paint, hit testing, and visibility without reparsing stylesheet
sources; local resources keep the direct stylesheet path.

The completed mutation-ownership batch is recorded in
[`native-engine-browser-007`](../plan/tasks/native-engine-browser-007.md).
External content workers now retain the parsed document and transactionally
apply bounded click/type control mutations, returning a fresh snapshot and
typed privacy-safe effects. The parent validates and publishes the revision
once; mutation deadlines and malformed transfers poison the worker without
false success or CDP fallback. Scroll and link navigation remain explicit
parent-owned handoffs. Diagnostics transfer, script execution, OS sandboxing,
supervisor recovery, and full content recovery remain open.

The completed recovery batch is recorded in
[`native-engine-browser-008`](../plan/tasks/native-engine-browser-008.md).
Content-worker spawn, exit, transport, timeout, protocol, rejection, and
invalid-transfer failures now have a typed class. A poisoned worker is never
reused or hidden behind a parent/CDP retry; external navigation is the explicit
fresh-worker recovery boundary and closing an already-exited worker is
idempotent. This is still not an OS sandbox, site-isolation boundary, or
cross-platform crash/restart certification.

The completed sandbox-launch batch is recorded in
[`native-engine-browser-009`](../plan/tasks/native-engine-browser-009.md).
Linux launches through required Bubblewrap namespaces and read-only runtime
mounts with private `/tmp` and `no_new_privs`; macOS uses a deny-by-default
Seatbelt profile; Windows uses a retained Job Object with active-process and
kill-on-close limits. Missing policy support returns a typed failure rather
than silently running an ordinary child. Network mediation, origin/site
isolation, restricted tokens, and complete cross-platform security evidence
remain separate promotion gates.

The completed redirect/charset batch is recorded in
[`native-engine-browser-010`](../plan/tasks/native-engine-browser-010.md).
The shared resource loader now applies strict HTTP(S)-only, credential-free,
eight-hop redirects, revalidates the final tuple origin, and decodes bounded
UTF-8, UTF-16, Latin-1, and Windows-1252 HTML responses. The child and parent
share the policy. The completed native-engine-browser-011 batch adds
process-owned, session-only bounded cookies and a bounded fragment-free
in-memory document cache, with explicit no-cache and variant denials and no
sensitive-data logging. Full HTTP freshness/revalidation, origin/referrer
request policy, CORS/CSP, mixed content, service workers, permissions,
subresources, complete encoding sniffing, and browser security promotion
remain open.

The completed native-engine-browser-012 batch adds explicit
strict-origin-when-cross-origin referrer derivation for top-level HTTP(S)
navigation. Same-origin requests send a fragment-free full referrer,
cross-origin requests send only the source origin, and opaque or
HTTPS-to-HTTP transitions send none. Manual redirect handling validates every
location and recomputes the policy per hop, while redirect cookies remain
transactional until final document validation. CORS/CSP, mixed content,
service workers, permissions, subresources, full cache semantics, and script
request mediation remain open.

The completed native-engine-browser-013 batch adds the first subresource
owner: bounded link stylesheet discovery and child-side text/css loading.
Relative URLs, CSP style-src/default-src, HTTPS mixed-content blocking,
per-hop redirect checks, cookies, referrers, response MIME, and byte quotas
are enforced before accepted CSS enters a child-owned reparse. Images,
media, fonts, scripts, fetch/XHR, service workers, permissions, broad
CORS/CSP, and browser security promotion remain open.

The completed native-engine-browser-014 batch centralizes the next network
policy boundary. The loader now names resource families for CSP evaluation,
recognizes their directive-specific source lists with `default-src` fallback,
resolves credential-free HTTP(S) subresources through one helper, and applies
one HTTPS mixed-content decision at the initial URL and every stylesheet
redirect hop. A private CORS mode also computes the serialized `Origin`
request value and validates exact or non-credentialed wildcard response
authorization, with credentialed reads requiring an explicit credentials
response. Stylesheets use the shared URL/CSP/mixed-content path. No script,
fetch/XHR, image, font, media, frame, worker, preflight, service-worker, or
permission caller exists yet; policy primitives are not browser support by
themselves.

The completed native-engine-browser-015 batch gives that policy a bounded
child-owned GET caller. `NativeEngine::fetch_async` sends a typed request over
the existing worker IPC; the child applies `connect-src`/`default-src`, URL
and HTTPS mixed-content checks, bounded redirects, explicit credential use,
and cross-origin `Origin`/ACAO authorization before transferring a capped
response. HTTP status and content-type metadata are preserved, cookies are
committed only after successful final validation, and the document revision is
unchanged. The operation has no custom methods, headers, body, streams,
preflights, JavaScript/Web IDL binding, service-worker route, or remaining
resource caller, so it is not `window.fetch` or browser-complete networking.

The completed native-engine-browser-016 batch adds the first real JavaScript
owner. The native backend now dispatches bounded script requests into a
persistent QuickJS realm: local documents use an owner-side realm, while
HTTP(S) documents execute in the sandboxed content worker. Full navigation
replaces the realm, same-document navigation retains it, and results cross the
semantic boundary only as bounded JSON. Source, result, memory, stack, and
five-second execution limits are explicit. The following host-view batch adds
the first page-owned surface without changing that boundary.

The completed native-engine-browser-017 batch refreshes a bounded read-only
`window`/`document` projection before each evaluation in both local and
child-owned realms. It exposes location/origin, viewport, title/text, form
state, and explicit ID/class/tag element finders; synchronous completion values
remain direct JSON, while top-level `await` drives bounded QuickJS jobs before
the same result conversion. The projection remains a bounded snapshot rather
than live Web IDL identity, but the following mutation and event-owner slices
now cross explicit typed boundaries. DOM insertion/removal, ancestor event
propagation, Rust-action listener dispatch, default-action ordering, timers,
modules, page-script loading, Fetch/XHR, service workers, and the remaining
resource classes stay open.

The completed native-engine-browser-018 batch connects the projection to a
bounded owner-command bridge. `click()`, form-state setters, and attribute
set/remove emit typed commands; Glass validates them against a cloned native
document, commits the batch as one revision, and reuses the child process for
external pages. The next host refresh observes the committed state. Live Web
IDL identity, ancestor event propagation, Rust-action listener dispatch,
script navigation, timers, modules, page-script loading, Fetch/XHR, service
workers, and remaining resource classes stay open.

The completed native-engine-browser-019 batch adds a bounded event/focus owner
inside the same persistent realm. Elements, `document`, and `window` retain
deduplicated target-local listeners across evaluations; bounded `Event` and
`CustomEvent` values support synchronous dispatch and cancellation. Scripted
focus/blur transitions emit typed commands, and scripted click activation is
suppressed when a target listener calls `preventDefault()`. The parent and
child owners validate and commit focus transitions with the same one-revision
batch contract. Ancestor propagation, listeners for Rust semantic actions,
default-action ordering, mutation invalidation, timers, modules, page-script
loading, Fetch/XHR, service workers, and remaining resource classes stay open.

The completed native-engine-browser-020 batch adds a bounded document event
graph. Projected elements carry nearest-element parent indices and expose
non-enumerable `parentElement`/`parentNode` links. Listener records retain
capture and once options; dispatch walks a snapshot-based
window/document/ancestor path through capture, target, and bubble phases with
`stopPropagation()` and `stopImmediatePropagation()`. Rust semantic actions
still do not re-enter the page realm.

The completed native-engine-browser-021 batch routes committed Rust semantic
action effects back into the existing local or sandboxed child realm as
bounded typed host-event source. Callback commands return through the existing
clone-and-transfer owner path; no executable callback crosses IPC and the
parent never evaluates an external page. This first bridge is post-action:
the action and callback mutation use separate revisions, callback errors cannot
undo the action, and `preventDefault()` does not yet suppress it. The
transactional click preflight is the next refinement.

The completed native-engine-browser-022 batch moves cancelable click delivery
ahead of activation. Local and child-owned semantic clicks clone the document,
run focus and click listeners, apply callback commands, honor
`preventDefault()`, and commit one final state/effect revision. The child uses
one typed preflight request and transfers only its validated result; the parent
never evaluates the network page. Pages without an initialized local realm
retain the Rust-only path. Type/input/change ordering, link navigation/default
actions, timers, modules, Fetch/XHR, and remaining resource classes stay open.

The completed native-engine-browser-023 batch gives Rust-owned type actions the
same clone-and-owner discipline. Local and child type paths apply the value,
dispatch focus/blur, input, and change in order, apply callback commands, and
commit one final revision. The active host command sink lets persistent
callbacks use captured element methods without writing to an old evaluation
buffer, while snapshot properties/tree links remain refresh-bound rather than
claiming live Web IDL identity. Insertion/removal, `beforeinput`, composition,
keyboard input, submission, link navigation/default actions, timers, modules,
Fetch/XHR, and remaining resource classes stay open.

The completed native-engine-browser-024 batch connects top-level script link
clicks to navigation ownership. Local clicks reuse resource/history handling;
external child clicks transfer a bounded link request after child-owned
validation and the parent performs the next child-owned load. Full navigation
resets the realm and same-document navigation retains it. Click and navigation
currently use separate revisions; target contexts, form submission,
unload/navigation task ordering, timers, modules, Fetch/XHR, and remaining
resource classes stay open.

The completed native-engine-browser-025 batch resolves relative and
root-relative HTTP(S) link references against the current document and keeps
fragment navigation same-document in the history/revision owner. Local and
external fragment links retain the page realm without a redundant fetch;
external non-fragment links remain child-owned. Target contexts, form
submission, lifecycle/default-action ordering, timers, modules, Fetch/XHR, and
remaining resource classes stay open.

The completed native-engine-browser-026 batch adds bounded inline page-script
loading. Local prepared navigations execute accepted inline JavaScript sources
in a fresh owner-side realm before publication; HTTP(S) documents execute them
inside the sandboxed content process, whose globals and listener records remain
available to later script and action requests. Commands still cross the typed
document-clone boundary, load-time link activation is rejected, and local
script failures do not publish a partial navigation. External scripts,
modules, parser timing, timers, Fetch/XHR, CSP script enforcement, and full Web
IDL identity remain open.

The completed native-engine-browser-027 batch adds bounded classic external
page-script loading for HTTP(S) content. Accepted `src` scripts are resolved in
DOM order and fetched by the sandboxed content process under the existing
script CSP/default-src, mixed-content, redirect, referrer/cookie, JavaScript
MIME, and byte-limit policies. External and inline sources execute in the same
persistent child realm; module/unknown types are not fetched, and the parent
receives no executable source. Local subresource ownership, module graphs,
parser timing, timers, Fetch/XHR, and full Web IDL identity remain open.

The completed native-engine-browser-028 batch adds bounded GET form submission.
The persistent realm exposes `submit()` and `requestSubmit()`, submit-button
clicks use the same typed path, and enabled named controls are URL-encoded
before local or child-owned navigation. Child form mutations transfer only a
validated node/URL handoff; POST/multipart, full constraint validation,
complete submission-event parity, target contexts, unload ordering, and full
form parity remain open.

The completed native-engine-browser-029 batch adds bounded page-module roots.
The parser classifies inline and external `type="module"` sources separately
from classic scripts, local and HTTP(S) owners execute them through QuickJS's
module evaluator in document order, and external module names retain the
validated final URL. Module commands still use the cloned-document boundary,
and child globals/listeners remain child-owned. Static import graphs,
dynamic `import()`, import maps, parser timing, local external subresources,
and full Web IDL identity remain open.

The completed native-engine-browser-030 batch adds bounded static module
graphs. The content process discovers static import/export URLs, resolves them
against each importing module's final URL, applies the owner-document script
policy and existing network limits, and installs the validated graph into an
in-memory QuickJS loader before root evaluation. Duplicate and cyclic entries
are bounded; bare specifiers, import maps, dynamic `import()`, parser timing,
and local external module subresources remain open.

The completed native-engine-browser-031 batch adds bounded literal dynamic
imports. Literal `import("...")` calls reuse the admitted HTTP(S) module graph,
and a capped QuickJS job drain lets module namespace promise callbacks publish
their typed document effects. Computed specifiers, bare packages/import maps,
non-HTTP(S) modules, and full browser task/microtask/parser timing remain open.

The completed native-engine-browser-032 batch adds bounded task turns. The
realm exposes `queueMicrotask` and next-host-turn `setTimeout`/`clearTimeout`,
drains QuickJS jobs after evaluation and event callback turns, and retains the
timer registry across same-document actions while resetting it on full
navigation. Wall-clock delays, intervals, animation/idle callbacks, parser
timing, and full task-source ordering remain open.

The completed native-engine-browser-033 batch adds bounded native semantic
keyboard input. Local and child-owned `KeyPress` actions target the focused
text control, dispatch cancelable `keydown` with bounded `key`/`code` metadata,
apply printable or Backspace/Delete edits, then dispatch `input` and `keyup`.
Persistent host wrappers refresh from the committed Rust snapshot before
callbacks, so retained element references observe current values. Selection,
caret movement, IME/composition, navigation keys, modifier shortcuts,
`beforeinput`, and form-submit defaults remain open.

The completed native-engine-browser-034 batch closes the bounded GET-form
submit lifecycle. `requestSubmit()` and submit-button defaults dispatch a
bubbling, cancelable `submit` event before query serialization; callback
mutations are included in the resulting URL, while direct `form.submit()`
remains event-free. Local and child semantic submit-button clicks use their
existing navigation owners, and child click IPC now transfers the navigation
record explicitly.

The completed native-engine-browser-035 batch extends that lifecycle to
bounded `method="post"` forms using
`application/x-www-form-urlencoded`. The encoded body travels through the
same typed parent/content-process request path, preserves the submit event and
callback-mutation contract, and keeps the existing redirect, referrer, cookie,
MIME, and response limits. Multipart/text/plain encoding, full constraint
validation, submitter serialization, target contexts, and unload ordering
remain open.

The completed native-engine-browser-036 batch adds bounded parser-time script
ordering. Classic parser-blocking scripts, external `async` scripts, deferred
classics, and default-deferred module roots retain timing metadata and execute
through one deterministic local/child ordering helper. Incremental parsing,
wall-clock completion races, script lifecycle events, dynamic insertion, and
full task-source timing remain open.

The completed native-engine-browser-037 batch delivers bounded page lifecycle
events after that schedule: `DOMContentLoaded` targets the document and `load`
targets the window, in that order, in both local and child-owned realms.
Listener mutations retain the existing typed clone-and-commit boundary.
Resource-specific events, unload/pagehide, completion-order races, dynamic
insertion, and full task-source timing remain open.

The completed native-engine-browser-038 batch adds bounded interactive-form
validation and submitter metadata in both owners. Required text controls,
checkboxes, radio groups, textareas, and single-select controls dispatch
non-bubbling `invalid` events and block `submit`/navigation until valid;
`requestSubmit(button)` and submit-button activation expose the bounded button
snapshot as `event.submitter`, while direct `form.submit()` remains the
validation-free path. Full `ValidityState`, type-specific constraints,
`formnovalidate`, multipart encoding, and target contexts remain open; bounded
external form association and submitter name/value serialization are covered
by the later browser-form slices.

The completed native-engine-browser-039 batch makes the lifecycle phase
observable in both owners: `document.readyState` is `loading` during the
accepted script schedule, `interactive` before `DOMContentLoaded`, and
`complete` before window `load`, with document `readystatechange` events at
the latter two transitions. Pages without scripts still receive a persistent
realm exposing final `complete` state. Resource-specific completion events,
wall-clock races, incremental parsing, unload/pagehide/pageshow, dynamic
insertion, and full task-source timing remain open.

The completed native-engine-browser-040 batch carries the initiating submit
control through bounded GET and urlencoded-POST successful-control
serialization. Both owners honor form `novalidate` and submitter
`formnovalidate` while retaining submit-event and callback mutation behavior;
the parent validates the typed submitter handoff before navigation. Image
submit coordinates, target contexts, multipart or
`text/plain` encoding, full constraint-validation APIs, and FormData/Web IDL
parity remain open.

The completed native-engine-browser-156 slice connects the parent-owned frame
registry to page-script frame properties. Direct `iframe` and `frame` elements
now expose stable `contentWindow` proxies, same-origin `contentDocument`
snapshots, and bounded child selectors/collections; the embedding realm also
projects `window.frames`, `parent`, `top`, and `frameElement`. Direct
WindowProxy `postMessage()` and `location.assign()`/`replace()` are routed back
through the owning target, with target-origin checks and descendant cleanup;
child navigation refreshes its document projection without replaying a stale
embedding `src`. The transfer remains parent-owned and snapshot-based, so
complete cross-realm identity, nested child-window projection in every event
path, popup policy/geometry, full frame lifecycle, and complete browser parity
remain issue #40 gates.

The native-engine-browser-041 batch adds a bounded resource completion point
for the process-backed document path. Successfully fetched external stylesheet
links and classic/module scripts now dispatch non-bubbling, non-cancelable
`load` events on their owning elements after accepted resource/script work and
before `DOMContentLoaded`; callback mutations remain in the persistent realm
and cross the existing typed owner boundary. Dynamic insertion, image/font/media
events, resource failures/error events, network concurrency, resource timing,
and full task-source scheduling remain open.

The completed native-engine-browser-042 batch closes the bounded replacement
navigation lifecycle boundary. Full replacement navigations now deliver
`pagehide` then `unload` to the outgoing local or child-owned window before
resource replacement, and `pageshow` to the newly published window after its
accepted ready/load schedule; initial publication also delivers `pageshow`.
Transition effects and callback mutations use the existing typed owner paths,
while same-document fragments remain in-place. Cancelable `beforeunload`,
bfcache/history-traversal parity, popup/opener contexts,
visibility state, and the full HTML navigation task model remain open.

The completed native-engine-browser-043 batch makes same-document fragment
navigation observable. GET fragment changes now retain the document and realm,
avoid a network reload, update the child URL owner when process-backed, and
dispatch a non-bubbling, non-cancelable window `hashchange` with `oldURL` and
`newURL`; callback mutations remain typed and bounded. `beforeunload`,
`popstate`, bfcache/history traversal lifecycle parity, and the full HTML
navigation task model remain open.

The completed native-engine-browser-044 batch adds bounded external form
ownership. Controls with `form="id"` associate with the matching form even
outside it, explicit ownership overrides ancestry, unresolved references do not
fall back, and local/child validation plus GET/urlencoded-POST serialization
preserve document order. External submit buttons use the typed
`requestSubmit(button)` path. Multipart/text/plain, full constraint
validation, target contexts, and the remaining browser-context primitives
remain open.

The completed native-engine-browser-045 batch adds bounded POST encodings.
`multipart/form-data` text fields use deterministic collision-checked
boundaries and `text/plain` uses CRLF-delimited `name=value` records; the
selected content type crosses the parent/content-process request boundary and
survives redirects that retain POST. File parts, FormData/Web IDL identity,
full constraint validation, target contexts, and the remaining
browser-context primitives remain open.

The completed native-engine-browser-046 batch adds bounded navigation
cancellation and history events. Replacement navigation dispatches cancelable
window `beforeunload` before `pagehide`/`unload` and resource loading, honors
`preventDefault()` and non-empty `returnValue`, and leaves the current page
intact when canceled. Same-document history traversal dispatches window
`popstate` before `hashchange` through the local/child typed owner paths.
Prompts, bfcache/session-history parity, cross-document restoration, popup/
opener contexts, and full HTML task-source semantics remain open.

The completed native-engine-browser-047 batch makes timer turns honor due
times. Local and child realms normalize bounded `setTimeout` delays, drain only
due callbacks on a later host turn in due-time/ID order, and remove canceled
timers through `clearTimeout`. There is no background page event loop yet;
intervals, animation/idle callbacks, task-source fairness, and full wall-clock
scheduling remain open.

The completed native-engine-browser-048 batch applies bounded submitter
overrides. Local and child-owned submissions now resolve `formaction`,
`formmethod`, and `formenctype` from the validated submitter before building
the request, so the effective method, action, and supported POST encoding are
consistent through script preflight and the content-process boundary. Form
target contexts, dialog submission, file parts, and general form-control/Web
IDL identity remain open.

The completed native-engine-browser-049 batch adds bounded repeating timer
turns. Local and child-owned realms expose `setInterval`/`clearInterval`; each
due interval runs at most once on a supplied host turn and reschedules from
that turn's monotonic time, including cancellation from inside its callback.
There is still no background page loop or task-source fairness, and animation
and idle callbacks remain open.

The completed native-engine-browser-050 batch expands interactive validation
for common controls. Local and child-owned forms now enforce required,
email/URL, UTF-16 `minlength`/`maxlength`, and numeric `min`/`max`/`step`
constraints before dispatching the existing bounded `invalid` events in
document order. Pattern/file constraints, `ValidityState` and
`checkValidity` Web IDL identity, custom validity, and validation UI remain
open.

The completed native-engine-browser-051 batch exposes a bounded script
`fetch()` promise path for process-backed HTTP(S) documents. Explicit script
evaluations can issue policy-checked GET requests and receive bounded
`ok`/status/URL/content-type/text/JSON response methods; promise callback
mutations are committed through the child owner. Page-load fetch scheduling,
non-GET requests, upload streams, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed native-engine-browser-052 batch carries those bounded GET fetch
requests through initial page-script and lifecycle evaluation. The child now
settles fetch promises and applies callback DOM mutations before publishing the
initial document snapshot, while callback navigation during publication stays
explicitly denied. Non-GET requests, upload streams, AbortController, service
workers, XHR/WebSocket, and full Fetch Web IDL identity remain open.

The completed native-engine-browser-053 batch adds bounded same-origin POST
fetches to explicit and initial page-script evaluation. It accepts an optional
string body and `Content-Type`, keeps the existing CSP, mixed-content, cookie,
referrer, redirect, response-size, and CORS checks, and applies method/body
rewriting for 301/302/303 while retaining POST data for 307/308. Cross-origin
preflight/simple-POST coverage, custom headers, multipart/FormData/blob/stream
bodies, AbortController, service workers, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed native-engine-browser-054 batch adds bounded cross-origin POST
fetch mediation. Simple content types use the Origin/response-CORS path;
non-simple content types perform an OPTIONS preflight that validates the
authorized origin, method, and supported `content-type` header before the
actual request. Preflight cache, custom headers, private-network access,
opaque `no-cors` responses, multipart/FormData/blob/stream bodies,
AbortController, service workers, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed native-engine-browser-055 batch adds a bounded asynchronous
`XMLHttpRequest` GET/POST bridge on the existing fetch/CORS command path. The
page realm exposes string-body requests, the supported `Content-Type` header,
bounded response status/text/URL/header access, and
`readystatechange`/`load`/`error` callbacks. Synchronous XHR,
upload/progress, binary response types, timeout/abort, streaming,
WebSocket/EventSource, and full Web IDL identity remain open.

The completed native-engine-browser-056 batch adds bounded string-only
`FormData` bodies to fetch and XHR. Append/set/delete/read helpers produce a
deterministic multipart body and boundary-bearing content type through the
existing request owner. File/blob parts, file chooser/upload progress,
streaming, URLSearchParams, and full FormData/Web IDL iterator identity remain
open.

The completed native-engine-browser-057 batch adds bounded string-only
`URLSearchParams` bodies to fetch and XHR. Append/set/delete/read helpers
produce URL-encoded POST data with `+` spaces and the matching charset-bearing
content type through the existing request owner. Full constructors, sorting,
iterator/Web IDL identity, and streaming remain open.

The completed native-engine-browser-058 batch adds bounded temporal form
validation. Local and child-owned forms strictly parse `date`, `month`, `time`,
and `datetime-local` values and enforce their bounded `min`/`max`/`step`
constraints using the correct date/day, month, or second units before the
existing ordered `invalid` event and submit paths. Pattern/file constraints,
custom validity, and full `ValidityState` Web IDL identity remained open at
that checkpoint; custom validity is covered by the later 059 API slice.

The completed native-engine-browser-059 batch adds the bounded script-visible
validation API. Local and child-owned controls expose `validity`,
`validationMessage`, and `willValidate` snapshots; `checkValidity()` and
`reportValidity()` dispatch the existing ordered non-bubbling `invalid` events;
and `setCustomValidity()` persists through the typed state wire. Pattern/file
validation, picker/UI behavior, and full live `ValidityState` Web IDL identity
remain open.

The completed native-engine-browser-060 batch adds bounded form construction.
Local and child-owned `new FormData(form)` calls collect named, enabled text
controls in document order, including controls associated through an external
`form` attribute, while submitter-only controls and unchecked checkbox/radio
controls are excluded. File controls fail closed with a `TypeError`; File/Blob
parts, picker/upload behavior, and full FormData Web IDL identity remain open.

The completed native-engine-browser-061 batch adds bounded pattern validation.
Local and child-owned text-like controls apply Rust-owned whole-value
`pattern` checks and expose `patternMismatch` through the existing validity API
and submission preflight. Invalid or unsupported regex syntax follows the
HTML invalid-pattern fallback and is ignored. Full JavaScript RegExp `v`-flag
and Unicode-set parity, file constraints, picker/UI behavior, and full live
`ValidityState` Web IDL identity remain open.

The completed native-engine-browser-062 batch completes the text-only
FormData control set for textarea, single-select, and initially selected
multi-select controls. Local and child-owned constructors preserve the
Rust-owned values and option order, while interactive multi-select actions,
`optgroup` disabled inheritance, File/Blob parts, and full FormData Web IDL
identity remain open.

The completed native-engine-browser-063 batch adds bounded multi-select
interaction. Local and child-owned option clicks toggle multiple selections,
script `option.selected` writes preserve them, `select.value` deterministically
selects the first matching option, and the host view exposes bounded
`multiple`, `options`, and `selectedOptions`. Modifier-key/range selection,
keyboard listbox behavior, text selection/IME, `optgroup` disabled inheritance,
and option-collection Web IDL identity remain open.

The completed native-engine-browser-064 batch wires the existing semantic
storage contract through the native dispatcher and `BrowserRuntimeSession` for
bounded local/session key-value read, write, and clear operations. State is
backend-instance scoped and deliberately not page-visible, durable,
origin-keyed, cookie-synchronized, or IndexedDB-backed; cookie-scope requests
remain explicitly unsupported. This removes a backend-contract denial without
claiming browser storage parity.

The completed native-engine-browser-065 batch installs bounded page-visible
`localStorage` and `sessionStorage` objects in the shared QuickJS bootstrap
used by local documents and the sandboxed content worker. `length`, `key`,
`getItem`, `setItem`, `removeItem`, and `clear` retain independent realm-local
maps under the existing key, value, and entry limits. State is not yet
origin-keyed across navigation or durable, and storage events, cookie
synchronization, IndexedDB, quota policy, and full Storage Web IDL identity
remain open.

The completed native-engine-browser-066 batch transfers those bounded page
storage maps through fresh-realm navigation. The runtime owner consumes
`localStorage` and `sessionStorage` mutations, keys tuple origins separately,
uses a fragment-free document key for opaque local documents, and seeds both
fresh local realms and the sandboxed content worker without putting storage
state on the DOM wire. The state remains volatile and separate from the
semantic `StorageRequest` maps; durable profiles, storage events, cookie
synchronization, IndexedDB, quota policy, and full Storage Web IDL identity
remain open.

The completed native-engine-browser-067 batch adds a bounded text-backed
`Blob`/`File` surface to the shared JavaScript bootstrap. Local and child-owned
realms can create capped text parts, read `size`/`type`/`name` metadata, call
`text()`/`slice()`, and pass the values through FormData `append`/`set`; the
existing fetch owner emits deterministic multipart filename and content-type
parts. Binary buffers, streams, file pickers and disk file controls, upload
progress, and full Blob/File/FormData Web IDL identity remain open.

The completed native-engine-browser-068 batch adds an explicit opt-in,
bounded JSON profile path for page `localStorage`. The engine owner and
sandboxed content worker load and save the same origin-keyed local map;
`sessionStorage` is intentionally not serialized and starts empty for each
engine. Profile locking, storage events, cookie profile persistence, IndexedDB,
quota policy, and full Storage Web IDL identity remain open.

The completed native-engine-browser-069 batch synchronizes bounded network
page `document.cookie` access with the existing Rust-owned session cookie jar.
Non-HttpOnly cookies are exposed to the page, setter lines are validated and
stored by the transport owner, and subsequent navigation/fetch requests use
the same domain/path/Secure/expiry rules; HttpOnly cookies remain request-only.
At the 069 boundary, cookie persistence, cross-document storage events, and
full cookie/Web IDL parity remained open; the bounded local-storage event
slice is recorded below.

The `7ac4c39d` navigation-semantics correction repaired revision allocation,
nonfatal page-script exceptions, POST form handoff, hidden-action rejection,
timer-clock determinism, and lifecycle effect ordering; the native integration
gate was re-run at 359 passed before the next feature slice.

The completed native-engine-browser-070 batch adds bounded same-profile local
`storage` events for concurrently alive local native documents. An explicit
profile path selects an in-process coordinator; effective `localStorage`
mutations are origin-filtered, exclude their source document, update the
receiver's Rust and JavaScript stores, and dispatch bounded event descriptors
with key/value/url/storage-area data. The completed native-engine-browser-071
batch extends the same coordinator through separate sandboxed HTTP(S) content
workers: worker responses carry bounded effective local-storage changes, and a
receiving worker applies queued changes and dispatches `StorageEvent` before
its next page operation. Session-storage browsing-context routing and
cross-process profile-writer coordination remain deliberately open at that
boundary. The completed native-engine-browser-072 batch now wraps the retained
JSON profile read path in a shared OS advisory lock and the complete snapshot
write/rename-or-copy path in an exclusive lock. Contention is bounded and
typed, and Linux exposes the lock file to the sandboxed worker. This protects
physical profile I/O; stale independent full-state snapshots still require a
future ownership or merge protocol.

The completed native-engine-browser-073 batch adds an explicit bounded
`NativeEngineConfig.context_id`, carries it through local and sandboxed page
realms, and records it as private source metadata on storage changes. Local
storage events retain same-profile origin routing; session-storage events are
accepted only by other live engines with the same browsing-context identity,
while different identities receive neither the event nor the mutation. Native
backend context validation and responses use the configured identity, and the
content-worker protocol version advances with the new event descriptor. The
coordinator remains process-local, session state remains volatile, and stale
profile ownership/merge plus independent-process event delivery remain open.

The completed native-engine-browser-074 batch adds a revision to the bounded
JSON Web Storage profile and makes each exclusive write re-read the latest
profile before applying the writer's local-storage mutation deltas. Independent
stale snapshots no longer erase unrelated local-storage keys; same-key writes
retain the lock-order last-writer rule, and profiles without the optional
revision field remain readable. The profile remains a local-storage snapshot,
session storage remains volatile, and cross-process event delivery, cookie
profile persistence, and IndexedDB remain open.

The completed native-engine-browser-075 batch replaces the process-local
storage-event coordinator with a bounded profile-adjacent newline-delimited
journal. Each live profile-backed engine has a unique writer ID and byte
cursor; local and session changes are appended under the retained profile lock,
and receivers poll before page operations, exclude their writer, and apply
origin/context routing before updating state and dispatching the page event.
Malformed records are typed failures, incomplete tails are safely repaired at
the next append, and the journal is capped at 4 MiB. Bounded retention and
crash recovery are recorded in the following slice. IndexedDB and the remaining
browser-complete gates remain open.

The completed native-engine-browser-076 batch adds opt-in cookie-profile
persistence to the same bounded versioned JSON profile. An explicit profile
path restores accepted Rust-owned cookies for page `document.cookie`, HTTP
navigation/fetch, and sandboxed content-worker resource paths; profiles without
the optional cookie field remain readable. Cookie updates share the retained
profile lock, re-read the latest snapshot, merge key-level changes, and use the
existing atomic replacement path. Expiry uses bounded wall-clock metadata, and
session cookies are retained for the explicit profile lifetime. Without a
profile path cookies remain volatile; the profile is sensitive
credential-bearing state, and IndexedDB plus full cookie policy/Web IDL parity
remain open.

The completed native-engine-browser-077 batch adds bounded reader leases at
`P.readers` beside the `P.events` journal. Leases carry a byte cursor and
heartbeat, expire after 15 minutes, and are refreshed at most every 30 seconds
or when a reader advances. Appends compact only complete records acknowledged
by every live lease, shift retained cursors under the same `P.lock`, and keep
the 4 MiB journal cap visible as typed backpressure when an active reader pins
too much unconsumed data. Missing leases and out-of-range cursors trigger a
reload from the authoritative revisioned profile snapshot; the engine sends a
full bounded state replacement to local or sandboxed realms, and does not
replay event callbacks discarded by recovery. A live engine preserves its
volatile session state, while the content worker remains a consumer of typed
IPC and never opens the journal or lease file. IndexedDB and full cookie
policy/Web IDL parity remain open.

The completed native-engine-browser-078 batch adds a bounded origin-keyed
IndexedDB subset to the same profile. Local documents and sandboxed content
workers expose positive-version `indexedDB.open`, upgrade callbacks,
`deleteDatabase`, database enumeration, object-store creation/deletion,
readonly/readwrite transactions, ordered request callbacks, key paths,
auto-increment keys, and JSON-bounded `get`/`getAll`/`count`/`put`/`add`/
`delete`/`clear` operations. Each origin is capped at 16 databases, each
database at 128 stores, each store at 128 records, each value at 8 KiB, and
the full IndexedDB snapshot at 64 KiB. The parent owns profile persistence;
the worker receives and returns validated snapshots through the existing IPC
contract and never opens profile files. Indexes, cursors, key ranges, binary
structured-clone values, full version-change/transaction scheduling, quota
APIs, and independent cross-process IndexedDB journal/delta merging remained
open at that checkpoint. Page-setup scripts suppress the timer pump until
initial publication, and the engine resets the deterministic timer clock at
the page-operation boundary.

The completed native-engine-browser-079 batch carries bounded IndexedDB
changes beside the profile-adjacent Web Storage journal. Local realms and
sandboxed content workers compute validated origin deltas; the parent applies
them to its full state, merges them into the latest locked profile snapshot,
and appends them in journal order. Live receivers apply IndexedDB-only records
even when no Web Storage event is present, then replace the current origin in
the persistent local realm or content worker through the existing bounded
state-sync IPC. Worker responses carry deltas rather than full snapshots, and
the worker remains unable to open profile, journal, lock, or lease files.
Reader-lease recovery still reloads the authoritative full profile. Disjoint
live database writers are covered for both local realms and content workers;
same-database structural conflicts retain the explicit journal-order
last-writer rule. Indexes, cursors, key ranges, non-JSON structured-clone
values, full transaction/version-change coordination, quota APIs, and the
remaining browser-complete gates remain open.

The completed native-engine-browser-080 batch adds bounded IndexedDB query
primitives. Object stores persist string-key-path indexes with unique and
multi-entry options; `IDBKeyRange` provides exact and open/closed bound
queries; and stores and indexes expose ordered get/count/delete operations,
key/value cursors, cursor continuation, advancement, update, and delete.
Index entries are derived from the bounded record map at query time, with
numeric-before-string key ordering and a 128-record scan ceiling. Index
metadata travels through its own profile delta, preserving unrelated record
changes during stale-writer merge. Compound keys, array keys outside
`multiEntry`, non-JSON structured-clone values, full transaction/version-change
coordination, quota APIs, and the remaining browser-complete gates remain
open.

The completed native-engine-browser-081 batch adds bounded same-realm
IndexedDB version-change coordination. Persistent connections receive
`versionchange`; a higher-version `open()` emits one `blocked` event while a
prior connection remains open, and the pending request resumes after the
final connection calls `close()`. The resumed version-change transaction
publishes the current object-store list and runs the existing upgrade
callbacks. Cross-process live connection identity, complete `deleteDatabase`
blocking, rollback, and the remaining browser-complete gates remain open.

The completed native-engine-browser-082 batch completes the bounded same-realm
IndexedDB deletion lifecycle. `deleteDatabase()` dispatches
`versionchange` with `newVersion: null`, emits one `blocked` event while a
prior connection remains open, and removes the database only after the final
connection calls `close()`. Missing deletes are successful no-ops, and a later
open can recreate the database. Cross-process live connection identity, full
factory operation-queue ordering, rollback, and the remaining
browser-complete gates remain open.

The completed native-engine-browser-083 batch adds bounded IndexedDB
transaction rollback for ordinary writes. Each transaction snapshots the
bounded JSON database state; request failure and explicit `abort()` restore the
snapshot, preserve the typed error, and deliver one `onabort` without a false
`oncomplete`. Concurrent transaction scheduling, upgrade-failure rollback,
structured-clone values, quota APIs, and the remaining browser-complete gates
remain open.

The completed native-engine-browser-084 batch adds a bounded StorageManager
quota surface. `navigator.storage.estimate()` reports deterministic
JSON-length usage against the fixed 4 MiB native profile quota, while
`persist()` and `persisted()` are stable asynchronous APIs that explicitly
return `false` because permission policy is not implemented. Quota prompts,
reservation, cross-process arbitration, structured-clone values, and the
remaining browser-complete gates remain open.

The completed native-engine-browser-085 batch adds bounded same-realm
IndexedDB transaction serialization. Requests for one database run through an
ordered queue, request callbacks precede the next operation, and queued
transactions capture rollback state only when they begin. A later transaction
therefore cannot restore an older snapshot over an earlier transaction’s
committed work. Cross-realm/process scheduling, upgrade-failure rollback,
structured-clone values, quota permission policy, and the remaining
browser-complete gates remain open.

The completed native-engine-browser-086 batch adds bounded IndexedDB
structured-clone extensions. Tagged JSON preserves `undefined`, non-finite and
negative-zero numbers, `Date`, `RegExp`, `Map`, and `Set` values across API
reads, worker transfer, profile persistence, and restart; cyclic,
unsupported, reserved-tag, invalid-date, and oversize values fail closed.
Binary buffers, typed arrays, Blob/File payloads, BigInt, transfer lists, and
full clone/prototype parity remain open.

The completed native-engine-browser-087 batch carries the existing bounded
text-backed `Blob` and `File` objects through IndexedDB. Tagged JSON preserves
their text payload, MIME type, file name, and non-negative `lastModified`; API
reads, worker transfer, profile persistence, and restart reconstruct fresh
objects. Byte-exact binary buffers, typed arrays, streams, transfer lists, and
full clone/prototype parity remained open at that checkpoint.

The completed native-engine-browser-088 batch adds bounded binary structured
cloning to IndexedDB. `ArrayBuffer`, common numeric typed arrays, optional
BigInt typed arrays when exposed by the runtime, and `DataView` are encoded as
visible byte vectors and reconstructed after API reads, profile persistence,
and the shared worker transfer path. `SharedArrayBuffer` fails closed with
`DataCloneError`; backing-buffer aliasing, detached/transfer-list semantics,
binary Blob/File methods, streams, and full clone/prototype parity remain open.

The completed native-engine-browser-089 batch adds bounded binary reads to the
existing text-backed `Blob`/`File` surface. `arrayBuffer()` and `bytes()`
return fresh UTF-8 bytes with deterministic surrogate handling and the same
value bound in local and content-worker realms. Existing text-backed `size`,
`text()`, `slice()`, FormData, and IndexedDB behavior remain stable; binary
Blob construction, streams, upload progress, transfer semantics, and full
Blob/File Web IDL parity remain open.

The completed native-engine-browser-090 batch lets `fetch()` send existing
text-backed `Blob` and `File` payloads directly through the local and
content-worker network bridge. A non-empty normalized Blob/File MIME type is
used as `Content-Type` when the caller supplies no explicit supported header;
the existing request, origin, CORS, credentials, redirect, and size policies
remain authoritative. XHR Blob bodies, binary Blob construction, streams,
abort, upload progress, and full Fetch/Blob Web IDL parity remain open.

The completed native-engine-browser-091 batch extends the same bounded
Blob/File request-body path to asynchronous XHR. `send()` preserves the native
object marker so the shared fetch bridge emits the text payload and normalized
MIME type; the worker HTTP path verifies exact transfer. Binary responses,
upload progress, timeout/abort, streams, synchronous XHR, binary Blob
construction, and full XHR/Blob Web IDL parity remain open.

The completed native-engine-browser-092 batch adds bounded
`AbortController`/`AbortSignal` semantics to fetch. Signals retain the first
abort reason, dispatch one event, support listener removal and
`throwIfAborted`, and reject associated observable fetch promises; late host
responses are ignored. The existing network operation may still finish within
its bounded budget, so socket-level cancellation, XHR `abort()`, timeout,
progress, `AbortSignal.timeout/any`, and full Web IDL parity remain open.

The completed native-engine-browser-093 batch expands the bounded
`URLSearchParams` surface to query strings, pair sequences, records, instance
copies, stable sorting, size, callbacks, optional-value deletion, and snapshot
iterators. Form-urlencoded output now applies the bounded reserved-character
escaping rules. Full live Web IDL iterator identity, exotic iterable inputs,
and complete URLSearchParams parity remain open.

The completed native-engine-browser-094 batch adds bounded response body
variants to fetch. `blob()`, `arrayBuffer()`, and `bytes()` return fresh
text-backed/UTF-8 copies alongside the existing independent `text()` and
`json()` methods. The host response boundary remains UTF-8-lossy text, so
streaming bodies, byte-preserving non-UTF-8 responses, BYOB readers, and full
Fetch/Response Web IDL parity remain open.

The completed native-engine-browser-095 batch replaces the response's ad-hoc
content-type lookup with a bounded read-only header view. Case-insensitive
`get`/`has`, `entries`/`keys`/`values`, `forEach`, and default iteration operate
over a normalized snapshot of the one validated content type transferred by
the network owner. Multiple/raw response headers, trailers, mutation, and
full Headers/Web IDL parity remain open; bounded request-header dictionaries
are covered by the later 097 slice.

The completed native-engine-browser-096 batch adds bounded asynchronous XHR
abort semantics through a request-local AbortController. An active abort
resets the XHR to `UNSENT`, emits one bounded `readystatechange`/`abort` pair,
and stale fetch continuations cannot deliver `load` or `error`. The existing
host request may still finish because transport cancellation is not yet part
of the IPC contract; timeout/progress and complete XHR/Web IDL parity remain
open.

The completed native-engine-browser-097 batch adds bounded plain-object Fetch
request headers. Names and values are normalized and revalidated at both the
JavaScript and Rust boundaries; forbidden/internal names remain Glass-owned,
same-origin custom headers reach the HTTP request, and cross-origin
non-safelisted names are included in sorted CORS preflight authorization.
Authorization is removed across cross-origin manual redirects. Full Headers
constructor/identity and mutation parity, response-header exposure, raw
headers, and trailers remain open.

The completed native-engine-browser-098 batch preserves the raw bounded HTTP
response bytes through a base64 IPC payload. `Response.blob()`,
`arrayBuffer()`, `bytes()`, and binary `Blob.slice()` now return fresh
byte-preserving results, while `text()`/`json()` and `Blob.text()` retain
replacement-decoded UTF-8 views. Streaming/BYOB, transfer identity, binary
Blob/File construction, and binary/stream FormData parity remain open.

The completed native-engine-browser-099 batch carries bounded raw-byte-backed
Blob/File request bodies through the existing Fetch and asynchronous XHR
bridge. The content process decodes and size-checks an optional base64 body
before the resource loader sends bytes directly; text-backed bodies and
multipart FormData retain their established contracts. Binary Blob/File
construction, multipart FormData byte parity, streaming, upload progress,
and complete Fetch/XHR/Blob parity remain open.

The completed native-engine-browser-100 batch adds bounded binary Blob/File
construction from `ArrayBuffer` and typed-array parts. Constructed values retain
their exact bytes and byte length and feed the 099 Fetch/XHR request-body
bridge; text-only parts retain their established behavior. Streaming multipart
FormData, streams, upload progress, and full Blob/File Web IDL parity remain
open.

The completed native-engine-browser-101 batch adds bounded normalized response
header snapshots to Fetch and asynchronous XHR. Duplicate names are combined,
same-origin responses expose the bounded valid set, and cross-origin responses
expose only CORS-safelisted or explicitly exposed names. `Set-Cookie`, invalid
raw header bytes, trailers, mutation, and full Headers Web IDL parity remain
open.

The completed native-engine-browser-102 batch extends multipart Fetch and
asynchronous XHR to preserve raw-byte-backed Blob/File parts. The existing
boundary, filename, MIME, and text-field contracts remain authoritative;
streaming FormData, upload progress, iterator identity, and complete
FormData/Blob Web IDL parity remain open.

The completed native-engine-browser-103 batch adds a bounded mutable request
`Headers` surface. Fetch accepts records, pair sequences, and native `Headers`
instances; names and values use the existing bounds and forbidden-header
policy, while response headers remain read-only snapshots. Full Headers Web
IDL identity and exotic iterable parity remain open.

The completed native-engine-browser-104 batch adds bounded asynchronous XHR
`arraybuffer` and `blob` response types. Raw response bytes now reach the
`response` value without text conversion, while the existing text mode remains
unchanged.

The completed native-engine-browser-105 batch adds a bounded asynchronous XHR
`timeout` contract. A finite non-negative timeout up to 4 seconds is carried
through the existing Fetch/content-worker transport; zero disables the extra
deadline, while a timed-out request reaches `DONE` with status zero, cleared
response state, one bounded `readystatechange`, and `ontimeout`. The native
request remains bounded by the existing response and worker limits, and late
completion cannot publish a load/error callback. Upload progress, transport
cancellation beyond the bounded request deadline, streaming, synchronous XHR,
and complete XHR Web IDL parity remain open.

The completed native-engine-browser-106 batch adds bounded successful CORS
preflight caching. A validated positive `Access-Control-Max-Age` is retained
for the document origin, target URL, method, credentials mode, and sorted
requested-header set; the cache is capped at 64 entries and 10 minutes, while
failed, invalid, and zero-age preflights are never cached. Private-network
access, opaque `no-cors` responses, and complete Fetch/CORS Web IDL parity
remain open.

The completed native-engine-browser-107 batch adds bounded FormData iterator
objects. `entries()`, `keys()`, `values()`, and `[Symbol.iterator]()` now
return self-iterating snapshot iterators with deterministic `next()` completion;
`forEach()` and multipart serialization retain the same ordered entry owner and
Blob/File value normalization. Live mutation during iteration, exotic
iterables, and complete FormData/Web IDL parity remain open.

The completed native-engine-browser-108 batch adds bounded iterable
initialization to URLSearchParams. Map, Set, and other pair-iterable inputs are
validated as bounded two-value sequences and retain insertion order before the
existing mutation, sorting, encoding, and snapshot-iterator owners apply.
Live iterator mutation/identity and complete URLSearchParams Web IDL parity
remain open.

The completed native-engine-browser-109 batch adds bounded Fetch mode policy.
Fetch defaults to `cors`; cross-origin `same-origin` targets and
non-safelisted cross-origin `no-cors` request headers/content types fail closed
before network I/O, while successful cross-origin `no-cors` requests omit the
Origin header and return an opaque script response with status zero, an empty
URL/header view, and rejected body reads. Same-origin no-cors retains the
ordinary bounded response shell. Service-worker/private-network integration,
streaming, redirect parity, and complete Fetch/Response Web IDL parity remain
open.

The completed native-engine-browser-110 batch adds bounded Fetch redirect
policy. `follow` remains the default and reports `redirected` plus the final
URL after a followed hop; `error` rejects before following; `manual` returns a
filtered `opaqueredirect` response with status zero and no URL, headers, or
body; and `same-origin` rejects a cross-origin redirect hop. The existing
bounded redirect, method-rewrite, URL, CSP, mixed-content, referrer, cookie,
and CORS owners remain in force. Full redirect-status/referrer parity,
service-worker/private-network routing, streaming, and complete Fetch/Response
Web IDL parity remain open.

The completed native-engine-browser-111 batch adds bounded static AbortSignal
constructors. `AbortSignal.timeout()` schedules one host-turn `TimeoutError`
abort within the existing timer budget, while `AbortSignal.any()` accepts a
bounded signal iterable, preserves the first reason, handles empty input, and
removes source listeners after abort. Transport cancellation, XHR integration,
and complete AbortSignal/Web IDL parity remain open.

The completed native-engine-browser-112 batch makes URLSearchParams
`entries()`, `keys()`, and `values()` live, owner-backed, and self-iterating.
Later bounded appends and value updates are visible through the existing cursor,
`[Symbol.iterator]` remains the `entries` method, and encoding/body ownership is
unchanged. Complex deletion/reordering mutation semantics and complete Web IDL
descriptor parity remain open.

The completed native-engine-browser-113 batch adds the bounded static
`AbortSignal.abort(reason)` constructor. It creates an already-aborted signal
with the default or supplied reason and reuses the existing `aborted`,
`reason`, `throwIfAborted()`, and fetch-consumer behavior without allocating a
timer or post-construction event. Transport cancellation, XHR integration, and
complete AbortSignal/Web IDL parity remain open.

The completed native-engine-browser-114 batch makes FormData `entries()`,
`keys()`, and `values()` live, owner-backed, and self-iterating. Later bounded
appends and value updates are visible through the existing cursor, while
multipart serialization and `forEach()` retain their existing owners. Complex
deletion/reordering semantics and complete FormData/Web IDL parity remain open.

The completed native-engine-browser-115 batch makes mutable request Headers
`entries()`, `keys()`, and `values()` live, owner-backed, and self-iterating.
Bounded `append()` and `set()` mutations are visible through the existing
normalized entry owner, while immutable response-header views remain snapshots.
Full Headers Web IDL parity, raw response headers, and trailers remain open.

The completed native-engine-browser-116 batch adds a bounded one-chunk
`ReadableStream` body to ordinary Fetch responses. `body instanceof
ReadableStream`, default-reader `read()`/completion, lock/release, cancel, and
async-iterator hooks are observable while the existing independent text, JSON,
Blob, ArrayBuffer, and bytes readers remain valid. Opaque and
`opaqueredirect` responses retain `body === null`; progressive transport
streaming, backpressure, body disturbance/`bodyUsed`, BYOB readers,
transport-level cancellation, and complete ReadableStream/Response Web IDL
parity remain open.

The completed native-engine-browser-117 batch adds bounded `Response.clone()`
projections with fresh response-header and body owners. Ordinary clones retain
the existing metadata and independent text, JSON, Blob, ArrayBuffer, bytes, and
one-chunk stream views; opaque and `opaqueredirect` clones retain their
filtered shells and `body === null`. Full body disturbance/`bodyUsed`, clone
rejection for locked or consumed bodies, shared tee/backpressure semantics,
Request/Response constructors, and complete Response Web IDL parity remain
open.

The completed native-engine-browser-118 batch adds bounded `Request` objects
and `fetch(request, overrides)` input. Request construction and cloning retain
the existing GET/POST, header, mode, redirect, credentials, body, and signal
bounds, while Fetch merges explicit overrides before using the existing
transport and security owners. Full Request body streams, disturbance rules,
URL/cache/referrer/integrity fields, duplex, and complete Request/Headers Web
IDL parity remain open.

The completed native-engine-browser-119 batch adds bounded `URL` objects for
HTTP(S)-focused relative resolution, component inspection, and snapshot
`searchParams`. URL objects are accepted by bounded Request and Fetch input,
while Rust/content-worker URL and origin policy remains authoritative at the
transport boundary. URL setters, full percent-encoding/IDNA/IPv6/default-port
parity, live search-parameter synchronization, non-HTTP scheme parity, and
complete URL/Web IDL identity remain open.

The completed native-engine-browser-120 batch adds bounded Response identity
and constructors. Fetched and constructed responses are `instanceof Response`;
`new Response`, `Response.json`, `Response.error`, and `Response.redirect`
reuse the existing bounded body/header/clone projections, with null bodies
remaining `body === null`. Stream input, body disturbance/`bodyUsed`, full
factory and redirect/error internals, trailers, shared tee/backpressure, and
complete Response Web IDL identity remain open.

The completed native-engine-browser-121 batch gives Fetch and asynchronous XHR
response-header views native `Headers` identity while retaining immutable
normalized snapshots. Lookup, duplicate combination, iteration, filtering, and
`forEach()` remain unchanged; response `append()`/`set()`/`delete()` reject
with a typed immutability error. Raw header bytes, trailers, live mutation,
descriptor parity, and complete Headers/Fetch/XHR Web IDL parity remain open.

The completed native-engine-browser-122 batch makes bounded URL
`searchParams` owners live with their URL query state. `append()`, `set()`,
`delete()`, and `sort()` update `search`/`href`; bounded `search` and `hash`
assignment updates the same URL owner, and Request/Fetch URL handoff observes
the current href. Full URL setter/parser and encoding parity, default-port/
IDNA/IPv6 behavior, and complete URL/Web IDL identity remain open.

The completed native-engine-browser-123 batch adds bounded URL `pathname` and
`href` setters. Pathname assignment normalizes dot segments in place; href
replacement refreshes components and the existing `searchParams` owner, while
query/fragment synchronization and Request/Fetch handoff remain active.
Authority/protocol setters, complete parser/encoding/IDNA/IPv6/default-port
behavior, and complete URL/Web IDL identity remain open.

The completed native-engine-browser-124 batch adds bounded HTTP(S) URL
authority/protocol setters. `protocol`, `host`, `hostname`, `port`,
`username`, and `password` rebuild the same URL owner, refresh origin/href, and
retain path/query/fragment and live `searchParams` state. Full encoding, IDNA,
IPv6/default-port canonicalization, non-HTTP schemes, and complete URL/Web IDL
identity remain open.

The completed native-engine-browser-125 batch adds a bounded live
`window.location` projection and navigation handoff. `assign()`, `replace()`,
`reload()`, `href`, and URL-component setters emit typed commands that the
existing Rust navigation owner resolves, validates, loads, and commits for
both local and content-process documents. Same-document fragments preserve the
document owner; `replace()` replaces the current history entry. Page-load and
ownerless lifecycle re-entrant navigation failed explicitly at that checkpoint;
the publication subset is now covered by native-engine-browser-126, while
nested contexts, full Location/Web IDL descriptors, and complete URL parsing
remain open.

The completed native-engine-browser-126 batch adds bounded page-publication
navigation. Initial local and content-process page scripts can return one
validated location handoff from parser/lifecycle publication, including
`DOMContentLoaded`, `load`, and `pageshow`; the Rust owner resolves the target,
loads local or HTTP(S) content, commits the document, and preserves push or
replace history semantics. Content-process load handoffs are consumed before
the browser-side commit, and post-commit `pageshow` handoffs are consumed by a
separate owner-aware path. A fixed eight-handoff limit rejects navigation
loops, and malformed or multiple handoffs fail closed. Outgoing
`beforeunload`/`pagehide`/`unload`, `hashchange`, nested contexts, complete
Location/Web IDL descriptors, and complete URL parsing remain open.

The completed native-engine-browser-127 batch bridges the engine-owned
inspection and targeting owners into the public native session and one-shot
CLI. `css=` locators reuse the bounded stylesheet selector parser for compound
and descendant matching, with unique-target enforcement before mutation.
Native sessions now expose semantic-node JSON data and logical PNG capture;
the CLI maps native `evaluate`, `click-at`, `key`, `scroll`, `dom`, and PNG
`screenshot` commands without creating a CDP session. Rich form-value and
semantic-region observation, revision guards, full selector/Web IDL parity,
and default/native production promotion remain subsequent issue #40 gates.

The completed native-engine-browser-128 batch extends the stable semantic
action boundary with clear, check, uncheck, and exact select operations. The
Chromium adapter maps those intents to `BrowserSession`'s existing form
actions. Native local documents and external HTTP(S) documents use dedicated
clear/select mutations and the existing click owner for check/uncheck; the
content worker returns one validated snapshot and bounded event list while
the parent assigns the revision. The one-shot native CLI exposes all four
actions, and non-native partial adapters reject them rather than silently
claiming support. Full form/Web IDL parity, rich observation, and native
default promotion remain issue #40 gates.

The completed native-engine-browser-129 batch extends that same action owner
with explicit key-down, key-up, and modifier-aware shortcut intents. Local and
HTTP(S) content-process documents use one event bridge for key/code and
Alt/Ctrl/Meta/Shift fields, cancellation, bounded default text editing, and
one parent revision per action. Chromium maps the intents to its existing
keyboard methods, while partial adapters reject them explicitly. Persistent
selection/caret ranges, IME/composition, repeat, text services, rich
observation, and production replacement of CDP remain open.

The completed native-engine-browser-130 batch adds bounded selection and caret
state to focused text controls. Local and HTTP(S) content-process documents
transfer range offsets and direction through the same validated wire; the
JavaScript host exposes `selectionStart`, `selectionEnd`,
`selectionDirection`, `setSelectionRange`, and `select`. Ctrl/Meta+A,
range replacement/deletion, and left/right/Home/End movement with Shift
extension now use the native default-action owner while keydown cancellation
still prevents editing. Grapheme/bidi caret geometry, clipboard,
IME/composition, repeat, and accessibility selection events remain open.

The completed native-engine-browser-131 batch closes the first public-session
revision gap. `BrowserRuntimeSession` now serializes its read/dispatch pair and
exposes guarded navigation and semantic actions; a stale expectation returns
the same typed `ActionContractError` used by the normal Glass session. Native
one-shot `navigate`, `click`, `type`, `clear`, `check`, `uncheck`, `select`,
`key`, `key-down`, `key-up`, `shortcut`, and `scroll` commands pass through
their existing `--expected-revision` fields. The backend still owns revision
increments, and no guard or unchecked operation creates a CDP fallback.

The completed native-engine-browser-132 batch carries that ownership through
the MCP transport. A native MCP connection now keeps a separate lazy
`BrowserRuntimeSession`, closes it at stdio EOF, and routes core navigation,
compact/deep evidence, semantic actions, script evaluation, logical PNG
capture, target listing, and storage reads through native owners. Offline MCP
tools remain browser-free, the default Chromium path is unchanged, and richer
workflow/profile/frame/download/prompt tools fail closed instead of allocating
Chromium when native is selected. The core MCP route is covered by a focused
no-Chromium test; it does not yet certify universal MCP workflow parity or
production CDP replacement.

The completed native-engine-browser-133 batch adds a read-only native target
preflight projection across the engine, runtime session, CLI, and MCP
surfaces. It resolves the current revision's semantic locator, reports the
node and visible viewport geometry, classifies hidden/disabled/read-only,
unsupported-action, outside-viewport, ambiguous, missing, and stale outcomes,
and exposes navigation/form hints without mutating document state. The
Chromium preflight owner is unchanged. This is a targeting-contract slice,
not a universal workflow or CDP-replacement certification.

The completed native-engine-browser-134 batch extends that targeting seam to
agent inspection. One atomic native page/node/layout snapshot now produces a
standard Glass semantic observation with a bounded main region and supported
interactive targets; the existing pure intent resolver then powers native
`findTarget` with the same constraint, confidence, ambiguity, revision, and
fingerprint semantics as the Chromium session. Native `inspectPage` and
`findTarget` are available through the runtime session, CLI, and MCP without
creating Chromium. Landmark-specific regions, rich accessibility trees,
structured extraction, and universal workflow parity remain open.

The completed native-engine-browser-135 batch extends the same observation and
preflight owners into synchronization. Native `wait` supports lifecycle, URL,
text, semantic-region, JavaScript-boolean, and target-state conditions;
native `verify` supports URL, title, visibility, text, revision, and bounded
composed predicates. Both return the existing Glass result/error envelopes
through the runtime session, CLI, and MCP, use bounded polling, and gate
JavaScript waits through the active evaluate capability. A consecutive
geometry sample is used for target stability. Request lifecycle accounting,
popup/dialog/download topology, action-specific `act-and-verify`, and
universal workflow parity remain open contracts.

The completed native-engine-browser-136 batch closes the native discovery to
mutation handoff for supported semantic intents. Native `actAndVerify` now
validates and resolves a caller-selected candidate, dispatches click, type,
clear, check, uncheck, or select under the same session lock, emits the
standard execution/action result envelopes, and releases the lock before
running the optional bounded verification predicate. The native CLI and MCP
surfaces route the operation without allocating Chromium. Specialized
request-ledger, popup/dialog/download witnesses and universal workflow parity
remain separate production gates.

The completed native-engine-browser-137 batch repairs semantic storage
ownership. Native localStorage and sessionStorage backend operations now use
the engine's origin-keyed page realm through bounded script mutations and
engine-owned state reads; a backend write is therefore immediately visible to
page JavaScript, and a page-state clear is returned by the backend. The
semantic cookie map remains rejected until native cookie metadata can be
returned without losing domain, path, security, and expiry attributes. Target
and frame ownership, request-ledger, popup/dialog/download witnesses, and full
browser parity remain issue #40 work.

The completed native-engine-browser-138 batch exposes the native cookie
profile through the production-facing runtime seams. Active HTTP(S) cookie
inspection includes HTTP-only entries and preserves the metadata represented
by Glass's public cookie type; metadata-preserving import and clear commands
reach the content worker as explicit bounded IPC operations, refresh the page
cookie view, and persist through the existing profile journal. CLI and MCP
route the native cookie surfaces without allocating Chromium. A map-shaped
cookie write remains rejected because name/value alone cannot safely express
domain, path, expiry, or security attributes.

The completed native-engine-browser-139 batch closes the asynchronous history
and basic topology projection seam. Native back/forward traversal now routes
local and HTTP(S) history entries through the existing runtime worker and
sandboxed content process, while boundary traversal remains explicit and
cannot become a fresh navigation or CDP fallback. The native runtime, CLI,
and MCP project one active page target (`native-context`) and one explicit main
frame (`native-context:main`) through the standard target/frame records;
selection is idempotent for those identities and fails closed for unknown
ones. Native target archives use the same bounded redacted projection. This
does not yet claim target creation/closure, child-frame execution, popup
witnesses, downloads, or browser-wide topology parity.

The completed native-engine-browser-140 batch adds the native prompt lifecycle
to the same ownership model. Local page realms, page-load/lifecycle scripts,
and the external content worker forward bounded `alert`, `confirm`, and
`prompt` metadata into a FIFO parent queue; runtime, CLI, and MCP expose
`dialogOpen`, `acceptDialog`, and `dismissDialog` without allocating Chromium.
Page-load response metadata is merged with runtime synchronization records so
the worker cannot discard a prompt before publication. The current script
bridge returns deterministic `false`/`null` values for confirm/prompt while it
records pending state; suspended decision-aware continuation and response
injection remain a separate scheduler milestone.

The completed native-engine-browser-141 batch adds bounded request-lifecycle
accounting to the same parent-owned model. Native navigation loads, direct
fetches, and external content-process evaluations mark one request operation
as in flight and then advance a completion sequence; the native runtime's
`network-quiet` wait polls that ledger through CLI and MCP without creating
Chromium. The quiet result is bounded and diagnostic-safe, exposing only
in-flight count, elapsed quiet time, required quiet time, and completion
count. Because the current content worker still owns a whole bounded load or
script operation at this seam, this does not claim per-resource subresource,
redirect, service-worker, or transport-cancellation lifecycle parity.

The completed native-engine-browser-142 batch closes the direct external-link
activation seam. A semantic anchor click in a process-backed HTTP(S) document
now remains in the content-process event bridge, carries the click
preventDefault result across IPC, and only then hands an allowed link to the
asynchronous native navigation owner. The click revision and navigation
revision remain separate and use the normal lifecycle, history, content
worker, and request-ledger paths. A canceled link click mutates only the
observable click state and does not navigate. Download attributes, popup or
new-target behavior, child-frame ownership, and full browser topology remain
open.

The completed native-engine-browser-143 batch adds the first usable native
download owner. An allowed anchor carrying `download` is kept on the current
committed page and queues a bounded parent-owned transfer; the same rule is
applied to the script-navigation handoff. The navigation-mode loader permits
cross-origin HTTP(S) download bytes without exposing them to page script,
applies bounded redirect and response-size policy, and returns the transfer to
the parent for exclusive, sanitized, collision-free file creation. Runtime,
CLI, and MCP expose completion into an existing destination directory, while
stable GUIDs, target/frame ownership, SHA-256 evidence, bounded list/cancel
state, and download-start verification remain on the same engine owner. This
does not yet claim chooser UI, programmatic or object-URL downloads,
streaming/progress, service-worker interception, or multi-target/frame
download parity.

The completed native-engine-browser-144 batch adds the first native target
registry. A session starts with its stable caller context as the selected
target, can initialize up to 32 total native engines, and exposes
collision-free IDs, opener linkage, redacted target projections, explicit
selection, parked-target retention, and close-all cleanup. Selection swaps
complete engine ownership, so navigation, history, prompts, downloads,
request accounting, and storage-reader leases remain attached to the target
that owns them. Runtime, CLI, and MCP reach the same lifecycle owner without
Chromium or CDP.

The completed native-engine-browser-145 batch adds target-owned child-frame
execution. Native `iframe`/`frame` owners are discovered in document order,
including nested descendants and bounded `srcdoc` documents, and each
published frame has a live native engine owner with stable parent linkage.
Explicit frame selection swaps complete child state into the active owner, so
the existing Glass navigation, evidence, script, action, storage, prompt,
download, wait, capture, and semantic-inspection routes are frame-correct;
parked parents and siblings retain their state. Frame and target shutdown
drain all owners. Native internal frames report `out_of_process=false` while
external documents retain the existing sandboxed content worker boundary.
CSP frame-source enforcement, frame lifecycle event parity, shared
same-origin frame scripting, postMessage, and complete browser topology remain
open gates.

The completed native-engine-browser-146 batch adds the first default-action
popup owner. An allowed local or content-process anchor click with
`target="_blank"` keeps the opener committed, queues a bounded popup intent,
and materializes one initialized parked page target with opener linkage. The
same owner path handles page-script `element.click()` activation; generic
native action/script dispatch and the runtime, CLI, and MCP
`clickExpectPopup` route all reach that target registry without CDP. Download
attributes retain precedence over popup creation, and initialized popup state
is independently selectable and closable. Named browsing contexts,
`window.open`, popup permission/geometry, shared opener scripting, and complete
browser topology remain issue #40 gates.

The completed native-engine-browser-147 batch closes the first frame-policy
ownership gap. The content worker extracts the effective `frame-src`,
`child-src`, or `default-src` source list from the document response and sends
only that bounded descriptor across version-4 framed IPC. The parent frame
registry evaluates the descriptor with the same source matcher used by other
native subresource checks before creating a child owner. A denied URL therefore
produces a live `about:blank` frame context without issuing the denied request;
an allowed same-origin URL uses the existing sandboxed content worker. The
embedding policy is retained by the child owner and checked on later direct,
link, redirect, and history navigation as well. Frame lifecycle/load events,
CSP reporting and complete source grammar, shared frame scripting,
postMessage, and full browser topology remain issue #40 gates.

The completed native-engine-browser-148 batch adds script-created browsing
contexts. The native JavaScript host resolves `window.open` URLs, routes
reserved same-context names through normal navigation, and emits typed popup
intents for `_blank` and named targets. Local and HTTP(S) documents transfer
those intents through version-5 content-worker IPC; the parent-owned target
registry initializes parked targets, preserves opener selection, reuses named
targets, resets their frame state on navigation, and bounds nested popup
cascades. The returned WindowProxy-shaped value is bounded until direct
cross-context property scripting has its own ownership contract.

The completed native-engine-browser-149 batch adds parent-owned cross-context
messaging. Local and HTTP(S) realms expose bounded WindowProxy
`postMessage()` and message-event delivery; protocol-6 content-worker effects
are decoded without trusting worker-supplied source metadata. The target
registry resolves private handles, named targets, and direct context IDs,
applies explicit `targetOrigin` matching, and routes `event.source` replies
through the same queue while preserving the selected opener. Payloads are
bounded JSON clones, so transferables and arbitrary cross-origin Window
property access remain outside this slice. `window.opener`, mutable
`window.name`, popup permission/geometry, frame lifecycle/shared scripting, and
complete browser topology remain issue #40 gates.

The completed native-engine-browser-150 batch adds browsing-context identity.
Local and HTTP(S) realms now carry bounded `window.name` and opener metadata
through target creation and content-worker startup. `window.opener` is null
for root contexts and a bounded proxy for popup contexts; child scripts can
read the opener name, mutate their own name, send opener messages, and have a
later named-target open reuse the renamed context. The opener name is captured
at child creation; live cross-context property scripting, popup
permission/geometry, frame lifecycle/shared scripting, and complete browser
topology remain separate issue #40 gates.

The completed native-engine-browser-151 slice adds live target closure through
the WindowProxy effect boundary. Local and HTTP(S) realms emit a bounded
`closeWindow` request with a private handle or direct context identity; the
parent registry resolves it after popup creation/message cascades and invokes
the existing target owner shutdown. The requesting proxy marks itself closed,
the target disappears from topology, and repeated or stale requests are
idempotent. The established protocol-6 worker envelope carries the additive
effect field without changing source ownership. Live observation updates for
all extant proxy objects remain a later identity gate.

The completed native-engine-browser-152 slice adds bounded cross-context
location control through the same WindowProxy effect boundary. Local and
HTTP(S) realms expose a URL snapshot plus `href`, `assign()`, `replace()`, and
`reload()`; each mutating operation emits a validated navigation request rather
than exposing an engine reference. The content-worker envelope transfers the
request as an additive protocol-6 effect, and the parent target registry
resolves direct context IDs, source-owned private handles, or names before
reusing the existing navigation/history/frame/storage owner. Popup creation
also carries a validated opener URL snapshot for child `window.opener` reads.
Live proxy observation, popup policy/geometry, frame lifecycle/shared scripting,
and complete browser topology remain later issue #40 gates.

The completed native-engine-browser-153 slice closes the stale-identity gap for
cached WindowProxy objects. The parent target registry emits bounded snapshots
for every live or recently closed context and source-owned private handle;
local and content-worker realms queue those snapshots and apply them after
their host bootstrap, preserving proxy object identity while refreshing URL,
name, target identity, and `closed` state. Closed target tombstones and private
handle mappings prevent stale handles from resolving to a different named
context. Synchronization remains parent-owned and bounded, while complete
Window Web IDL identity, popup policy/geometry, frame lifecycle/shared
scripting, and full browser topology remain later issue #40 gates.

The completed native-engine-browser-154 slice closes lifecycle navigation
re-entry. Local and content-process `beforeunload`, `pagehide`, `unload`,
`popstate`, and `hashchange` callbacks can return one validated location
handoff to the existing parent-owned loader/history/frame owner. Cancellation
is preserved as a separate result from a completed lifecycle with no handoff;
the lifecycle that emitted a handoff is not dispatched a second time, while
the newly committed document follows its normal lifecycle. Same-document and
full-navigation loops remain bounded and ambiguous multiple handoffs fail
closed. Popup permission/geometry, shared frame scripting, full Window Web IDL
descriptors, and complete browser topology remain issue #40 gates.

The completed native-engine-browser-155 slice adds a bounded Web IDL identity
foundation to both local and HTTP(S) page realms. Window, Document, Node,
Element, supported HTML element classes, Location, NodeList, HTMLCollection,
Event, CustomEvent, and StorageEvent values now pass ordinary browser-style
identity checks; document and element metadata, `defaultView`, and stable
window relationship properties are also projected. Element `ownerDocument`
resolves through the current realm document so reused host objects do not
retain an obsolete bootstrap snapshot. This is a prototype/identity layer
over the existing Rust DOM, not a second DOM implementation: full Web IDL
descriptors and methods, live mutation, shadow/custom elements, ranges,
cross-origin frame properties, shared frame scripting, and complete browser
parity remain open.

The completed native-engine-browser-157 slice extends the page-script frame
bridge recursively. Each direct binding carries a bounded child-binding tree,
so same-origin projected child documents expose nested frame elements,
`contentWindow`, `contentDocument`, `window.frames`, numeric child windows,
and stable nested `parent`/`top`/`frameElement`/`document` relationships.
Nested message and navigation effects resolve through the existing parent-owned
flat registry, including parked descendants. Revision/URL/origin/topology
keys refresh cached projections without sharing engine pointers or allowing a
cross-origin document snapshot through. Selected-child realm relationship
metadata is now installed by slice 158; complete cross-origin Window Web IDL
behavior, live cross-realm identity, full frame lifecycle/topology, and
complete browser parity remain issue #40 gates.

The completed native-engine-browser-158 slice installs the selected-frame
script context. Every realm now carries its frame identity independently of
storage ownership, and the parent registry transfers bounded parent/top
Window/document descriptors through local and content-worker execution. The
active selected frame is merged into the recursive parent/top projection, so
`window.parent.document.defaultView`, `parent.frames[n]`, `frameElement`, and
the selected global's nested WindowProxy all preserve identity. The bridge
normalizes `contextId` window descriptors and `frameId` embedded bindings into
one cache key. The content-worker envelope is protocol 7 so an older helper
cannot silently construct a realm with the wrong frame identity. This remains
a parent-owned snapshot boundary; frame-target routes use frame IDs while
popup and target-owner effects retain target-context identity. Cross-origin
Window Web IDL, full frame lifecycle/load ordering, live cross-realm identity,
and complete browser parity remain issue #40 gates.

The completed native-engine-browser-159 slice closes a cross-origin frame
security defect in that projection. WindowProxy state is now caller-relative:
cross-origin `document`, history/storage/IndexedDB and other bounded sensitive
Window properties raise a typed `DOMException` named `SecurityError` (legacy
code 18), while `contentDocument` and `frameElement` do not disclose the child
document or embedding node. A selected cross-origin child likewise observes a
null `window.frameElement`; same-origin frame identity and the safe URL,
location, name, close, message, and topology surface remain intact. Cached
proxy origin state is refreshed after navigation so a target crossing origins
cannot retain the old access level. Complete Window Web IDL descriptors, live
cross-realm identity, full frame lifecycle/load ordering, and complete browser
parity remain issue #40 gates.

The completed native-engine-browser-160 slice adds same-origin frame DOM
command routing. Projected `contentDocument` elements can now send the
already-supported bounded focus, blur, click, value, selection,
checked/selected, validity, and attribute operations to the child frame's
actual native document owner; the selected-child parent projection is covered
as well. The backend validates the source and target origins and the command
crosses the content-worker boundary as bounded data, so no JavaScript object or
engine pointer is shared. Structural DOM creation/removal, cross-realm event
listener identity, complete frame lifecycle/load ordering, and browser-wide
Web IDL parity remain promotion gates.

The completed native-engine-browser-161 slice adds transactional subtree text
mutation. `textContent` and `innerText` setters now replace the target's native
children, detach replaced descendants from selectors, semantic projections,
layout, and frame discovery, and retain unaffected arena identities. The
command is available in local realms, content-worker realms, and same-origin
frame projections. Dynamic element creation, live child-node collections,
cross-realm listener identity, complete frame lifecycle/load ordering, and
browser-wide Web IDL parity remain promotion gates.

The completed native-engine-browser-162 slice adds bounded structural DOM
mutation. `innerHTML` replaces an element subtree through the existing native
fragment tokenizer and exposes deterministic escaped markup through the script
snapshot; `remove()` and `removeChild()` detach existing subtrees. The typed
commands are available in local realms, content-worker realms, and same-origin
frame projections, with detached arena entries filtered from selectors, layout,
visible text, and frame discovery. The parser remains bounded rather than a
full WHATWG fragment implementation, and dynamic element creation, live
child-node collections, cross-realm listener identity, complete frame
lifecycle/load ordering, and browser-wide Web IDL parity remain promotion
gates.

The completed native-engine-browser-163 slice adds Rust-owned DOM node
construction. Local and content-worker page realms can create bounded
elements and text nodes while detached, set their attributes/text, nest them,
move existing subtrees, and commit deterministic `appendChild()` or
`insertBefore()` order. The host preserves parent identity and serializes the
constructed subtree during the current evaluation; the native snapshot is the
authority on the next evaluation. Same-origin frame projections retain the
typed command allow-list but still need a detached construction factory,
while live child-node collections, cross-realm listener identity, complete
frame lifecycle/load ordering, and browser-wide Web IDL parity remain
promotion gates.

The completed native-engine-browser-164 slice completes same-origin frame
construction. Projected child documents can create detached elements and text
nodes, compose and reorder them, and send one bounded `FrameScriptBatch` to
the child frame's native document owner. The child queues those typed commands
through its normal Rust transaction, preserving origin validation, temporary
node quotas, subtree ownership, and one revision; no object or arena pointer
crosses the realm boundary. Live child-node collections, cross-realm listener
identity, complete frame lifecycle/load ordering, and browser-wide Web IDL
parity remain promotion gates.

The completed native-engine-browser-165 slice adds owner-backed live DOM tree
identity to local and same-origin frame projections. `childNodes` and
`children` retain their collection objects while indexed access, `length`,
iteration, `item()`, and HTML collection `namedItem()` reflect structural
changes. Element and text hosts expose first/last child, element and sibling
traversal, `hasChildNodes()`, `contains()`, `replaceChild()`, and
`isConnected`; document root lists participate in parent-node and containment
queries. Parser text-node projection, full Web IDL descriptors, observer
delivery, and browser-wide parity remain separate promotion work.

The completed native-engine-browser-167 slice adds a shared bounded selector
and class-token surface to local and same-origin frame projections. Scoped
element queries, document queries, `matches()`, and `closest()` support
compound, descendant, child, comma-list, attribute, and common state or
structural pseudo-class selectors. `classList` retains its owner and routes
validated token mutations through the existing typed attribute command. The
full CSS selector grammar, complete Web IDL descriptors, cross-realm listener
identity, and browser-wide parity remain separate promotion work.

The completed native-engine-browser-168 slice adds live bounded `style` and
`dataset` projections to local and same-origin frame elements. Declaration
reads, indexed names, camelCase/dashed property access, `cssText`,
`setProperty()`/`removeProperty()`, priorities, dataset camelCase mapping,
enumeration, writes, and deletion reuse the existing attribute command owner,
so layout still consumes Rust-owned committed style state. Full CSSOM/value
validation, computed-style descriptor parity, complete cross-process event
delivery, and browser-wide parity remain separate promotion work.

The completed native-engine-browser-170 slice adds cross-process event
observation for same-origin frame activity. Bounded child effect metadata is
delivered to the parent projection across active and parked frame owners;
parent bindings are refreshed before target resolution, and capture/target/
bubble delivery terminates at the projected child document and window. Parent
issued focus/blur/click preflight is not replayed, ancestor and cross-origin
targets remain isolated, and selected-frame navigation/action/script effects
are covered by the same route. Full event ordering, observer APIs, Web IDL
identity, and browser-wide parity remain issue #40 gates.

The completed native-engine-browser-171 slice extends that bridge across child
navigation and `postMessage`. Runtime effect bundles retain typed child event
metadata, pending frame scripts, browser queues, and `window.name`; the backend
validates same-origin ancestry, projects child lifecycle/hash-change events to
each eligible ancestor, and captures effects generated by parent handlers for
the bounded upward cascade. Initial document-load observer delivery,
stale-generation rejection, complete Window Web IDL identity, and browser-wide
parity remain issue #40 gates.

The completed native-engine-browser-172 slice carries bounded page-load event
metadata across the content-worker boundary and commits it at the receiving
document revision. Frame bindings and parent/top Window snapshots include the
document generation; stale DOM/document effects are dropped after navigation,
while stable frame-window lifecycle events remain deliverable. Complete
observer APIs, resource scheduling, cross-realm identity, Window Web IDL, and
browser-wide parity remain issue #40 gates.

The completed native-engine-browser-173 slice adds bounded `MutationObserver`
delivery to the shared local and content-worker JavaScript host. Observer
registrations retain callback identity across same-document evaluations;
attribute, character-data, child-list, reparenting, removal, and
text/HTML-replacement command batches produce ordered records with optional old
values and subtree filtering, and delivery runs at the existing Promise-job
checkpoint. Detached record payloads remain data owned by the page realm.
Projected cross-realm observer delivery, layout/resource observers, complete
resource scheduling, Web IDL identity, and browser parity remain open.

The completed native-engine-browser-174 slice extends mutation observation into
same-origin frame projections. Frame-qualified node keys keep child and parent
indexes isolated; projected DOM command batches generate records against the
projected frame document, including detached-node observation, before the
validated typed handoff to the child owner. Cross-origin disclosure remains
blocked. Independently running child-task effects, layout/resource observers,
complete resource scheduling, Web IDL identity, and browser parity remain open.

The completed native-engine-browser-175 slice connects script-visible geometry
to the Rust layout owner. Layout border/content boxes are transferred through
the typed document snapshot; local, content-worker, and same-origin projected
elements expose DOMRect and bounded dimensions, while `ResizeObserver` records
are delivered at the established Promise-job checkpoint. Content-worker scroll
offsets are synchronized before script/input evaluation, and position-only
scrolling does not trigger resize records. Resource/layout observer breadth,
fractional geometry, complete scheduling, Web IDL descriptor parity, and
browser-wide parity remain issue-40 promotion gates.

The completed native-engine-browser-176 slice extends the same host-owned
checkpoint with persistent `requestAnimationFrame`/`cancelAnimationFrame`,
navigation-scoped `performance.now()`, and bounded `IntersectionObserver`
entries. Threshold crossings are computed from the current viewport/root
intersection over the Rust layout snapshot and delivered as Promise jobs;
content-worker and projected same-origin realms retain the existing typed
geometry/scroll ownership. Independent vsync, fractional/composited layout,
resource observers, complete scheduling, Web IDL descriptor parity, and
browser-wide parity remain open promotion gates.

The completed native-engine-browser-177 slice carries the History API through
both native execution boundaries. `pushState()`, `replaceState()`, bounded
structured state, `history.length`, and bounded traversal now remain coherent
after direct evaluation, local input/lifecycle dispatch, and process-backed
HTTP(S) event callbacks. Content-worker mutation replies include a validated
history envelope and synchronize URL/state/length before the next callback;
the parent applies those commands once and rejects ambiguous traversal plus
competing navigation. Cross-document session history, bfcache, cross-origin
history, complete Web IDL descriptors, and browser-wide parity remain issue
#40 promotion gates.

The completed native-engine-browser-178 slice adds host-owned document-fragment
construction and mutation across local, process-backed HTTP(S), and same-origin
frame realms. Fragment identity, ownership, nested-fragment flattening,
append/prepend, sibling insertion, replacement, and replace-children operations
now use the same projected-tree cache and removal semantics as attached nodes;
removing the final child also synchronizes empty content instead of leaving a
stale markup snapshot. Fragment staging remains a JavaScript-side construction
surface until attachment, so direct fragment `MutationObserver` records and a
fragment `innerHTML` setter are not yet claimed. Full Web IDL descriptor parity,
complete observer semantics, and browser-wide parity remain issue #40
promotion gates.

The completed native-engine-browser-179 slice adds bounded `innerHTML`
construction for detached fragments and live selector traversal over newly
created trees. The shared host tokenizer creates nested element/text nodes,
decodes common and numeric entities, applies quoted/unquoted/boolean
attributes, and stops correctly at void elements; fragment and element
`querySelector*`/collection methods now walk the current projected tree in the
same script turn. The same behavior is available in local, content-worker, and
same-origin frame realms, while raw-text/foreign-content parsing,
full malformed-HTML tree-builder rules, and complete Web IDL parity remain
open issue #40 promotion work.

The completed native-engine-browser-180 slice makes detached document
fragments valid `MutationObserver` targets. The shared observer queue records
fragment child-list insertion/removal and sibling context directly while the
existing typed command recorder continues to own attached-element changes;
moving an attached node into a fragment emits the corresponding native
removal. Local, content-worker, and same-origin frame realms use the same
bounded delivery checkpoint and subtree matching. Full observer ordering and
descriptor conformance remain issue #40 promotion work.

The completed native-engine-browser-169 slice adds frame-qualified EventTarget
behavior to same-origin projected elements, detached projected elements,
documents, and window proxies. The shared dispatcher derives capture/bubble
paths from the projected parent-node tree and stops at the frame-local document
and default window; detached nodes do not leak events to an owner document.
Projected click/focus/blur dispatch parent-side events while retaining their
typed child command handoff. Complete cross-process event observation, full Web
IDL descriptor parity, and browser-wide parity remain separate promotion work.

The completed native-engine-browser-166 slice adds attached text-node records
to the bounded script snapshot. Local, content-worker, and same-origin frame
realms now reconstruct parsed text nodes in native child order and retain their
identity across evaluations, exposing `nodeValue`, `data`, `parentNode`,
siblings, and live collection filtering. Text mutation still routes through
the existing typed Rust transaction; full Web IDL descriptors, observer
delivery, and browser-wide parity remain separate promotion work.

The engine remains inside the existing two-crate workspace. Internal modules,
helper binaries, and an out-of-process content worker are allowed; a third
installable crate is not. Until the production gates pass, the native feature
and backend remain explicitly experimental/default-off and Chromium/CDP
remains available as the production compatibility path.

## Purpose and boundary

The native engine owns a deterministic, headless browser-platform kernel. The
current slices own lifecycle, bounded target topology with one explicitly
selected target at a time, local document resources
plus bounded external HTTP(S) HTML documents,
HTML-to-DOM parsing, history, revisions, bounded semantic evidence, and a small
revisioned semantic interaction/effects model for text, checkbox, radio, and
single-select controls, plus bounded visibility/actionability and raw-text/RCDATA
parser gates, a narrow CSS presentation subset, and deterministic integer-pixel
normal-flow geometry with point hit testing, a Rust-only clear/fill/text/border
display list, a bounded logical RGBA software surface and PNG capture,
inherited text color through DOM parent links, bounded `overflow:hidden`/
`overflow:clip` paint clipping, bounded side-specific
solid/dashed/dotted/double/groove/ridge/inset/outset-border paint primitives,
bounded exact omitted-component `border:none` and physical `border-top:none`,
`border-right:none`, `border-bottom:none`, and `border-left:none` no-paint
shorthands,
bounded circular border radii, bounded outer/content box geometry with bounded
min/max width/height constraints, explicit root
horizontal and vertical viewport scrolling, bounded inline-box line placement, and bounded fixed pixel
line-height flow with bounded inheritance, bounded case-insensitive
15-layer/unlayered `line-height: revert-layer` rollback with inherited/root
fallback, bounded direct-text fragments at
actual flow origins,
source-order text paint, bounded word-aware wrapping, bounded physical
four-side padding/margin edges, bounded source-whitespace boundaries, and
bounded hard line breaks in supported inline flow, and
bounded inherited `white-space: pre-line`, `white-space: pre`,
`white-space: pre-wrap`, and `white-space: nowrap` source whitespace flow, and
bounded case-insensitive 15-layer/unlayered `white-space: revert-layer`
rollback with five-mode inherited/root fallback, and
bounded inherited physical `text-align:left|center|right` line placement, and
bounded functional `rgba(R, G, B, A)` alpha colors for background, border, and
text paint, bounded inherited `color` literals/alpha with case-insensitive
`currentColor` resolution from inherited color or the bounded black fallback,
and bounded case-insensitive `inherit|unset|initial|revert` resolution with
the inherited parent-color/black-root fallback and explicit initial black,
bounded fixed-cell `text-decoration:none|underline|overline|line-through`
paint including distinct shorthand combinations, bounded inherited
`text-decoration-style:solid|dashed|dotted|double|wavy` presentation, where
`double` paints two thickness-preserving solid bands separated by one pixel and
`wavy` repeats a fixed eight-pixel phase `[0,1,2,1,0,-1,-2,-1]`,
bounded inherited
`text-decoration-thickness:1px|2px|3px|4px` positive-y bands with
thickness-scaled integer dash/dot periods, bounded inherited
`text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for
underline and overline while preserving line-through; its inherited owner also
accepts standalone case-insensitive `inherit|initial|unset|revert`, using the
computed parent for inherited forms, finite `auto` at the root for `initial`,
and the current one-author-origin parent fallback for `revert`, while
`revert-layer` remains the named-layer rollback and mixed/unsupported forms
remain bounded diagnostics, bounded inherited
`text-decoration-skip-spaces:none|all|start|end|start end` fixed-cell
ASCII-space interval skipping across underline, overline, and line-through,
with line-edge provenance assigned during block-owned flow flush, and bounded
inherited signed
fixed-pixel
`text-underline-offset:-4px..=4px` that moves only the underline toward
decreasing or increasing y; its inherited owner also accepts standalone
case-insensitive `inherit|initial|unset|revert`, using the computed parent for
inherited forms, finite `0px` at the root for `initial`, and the current
one-author-origin parent fallback for `revert`, while `revert-layer` remains
the named-layer rollback and mixed/unsupported forms remain bounded
diagnostics, bounded
local `text-decoration-color` literal/alpha values and case-insensitive
`currentColor` resolution through the 15-layer registry with a separate
glyph/decoration paint color and the existing `Option<NativeColor>` fallback,
and bounded case-insensitive `inherit|unset|initial|revert` reset handling,
bounded inherited
`text-decoration-line` and `text-decoration` rollback through the same
15-layer registry with their shared three-bit line-state owner and inherited
fallback, and bounded inherited `text-align`, `text-align-last`, and
`text-justify` rollback through the same 15-layer registry, and bounded
inherited ASCII `text-transform:none|uppercase|lowercase` layout, and
bounded inherited non-negative fixed-pixel `word-spacing` across the supported
fixed-cell whitespace modes, and
bounded inherited non-negative fixed-pixel `letter-spacing` after rendered
fixed-cell characters in each emitted fragment, bounded inherited
`font-weight:normal|bold|400|700` fixed-cell raster presentation, bounded
inherited `font-style:normal|italic` fixed-cell raster presentation, and
bounded inherited `word-break:normal|break-all` fixed-cell wrapping, and
bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
fixed-cell inline and inline-block line items. These supported
text-presentation declarations also accept a terminal case-insensitive
`!important` marker through a private doubled author-origin partition with
reversed named-layer order, inline important precedence, invalid-later
preservation, and `revert-layer` rollback. The local `display`, `visibility`,
and `opacity` declarations also accept that bounded priority through a private
doubled local partition. The normal-only flex and gap declarations also accept
that bounded priority through a private doubled flex/gap partition; shorthand
expansions carry one declaration's priority to each supported component, while
the existing computed-value and layout/artifact owners remain unchanged. The
six normal-only dimension declarations (`width`, `height`, `min-width`,
`max-width`, `min-height`, and `max-height`) also accept that bounded priority
through private doubled dimension candidate streams; important-over-normal
ordering, reversed named-layer priority, inline precedence, invalid-later
preservation, and `revert-layer` rollback are carried into the existing
optional computed dimensions without changing the geometry/artifact owners.
The normal-only physical box-model declarations (`box-sizing`, `padding`, and
`margin` with their four physical longhands) also accept that bounded priority
through private doubled candidate streams with per-edge importance, preserving
content-box/border-box conversion, auto-margin provenance, and the existing
geometry/artifact owners. The same six dimension owners also accept standalone
case-insensitive `inherit`: explicit values copy the parent's computed
optional pixel value, including a parent `None`/auto result, while omitted
dimensions remain local and do not inherit. Percentages, negative lengths,
intrinsic sizing, aspect ratio, and new used-value state remain outside this
bounded extension. The same six dimension owners also accept standalone
case-insensitive `initial`, `unset`, and one-author-origin `revert`, each
resolving to the existing `None`/auto fallback; `revert-layer` remains a
separate lower-layer rollback and mixed reset tokens remain unsupported. The
normal-only `overflow`, `overflow-x`, and
`overflow-y` declarations also accept that bounded priority through private
doubled x/y candidate streams and shorthand/x/y importance bits, preserving
important-over-normal ordering, reversed named-layer priority, inline
precedence, invalid-later preservation, independent axis projection, and
`revert-layer !important` rollback through the existing clip, root-overflow,
layout, display-list, raster, capture, point-hit, and semantic/source-order
owners. Nested scrolling, visible/auto/scroll used-value parity, and
remaining properties retain their existing bounded behavior. The horizontal-tb
logical `padding-block`/`padding-inline` and `margin-block`/`margin-inline`
shorthands plus their block/inline start/end longhands project through resolved
`ltr`/`rtl` direction into those physical edge owners, sharing their per-edge
important-over-normal, reversed-layer, inline-important, invalid-later, and
`revert-layer` behavior; logical `box-sizing`, vertical writing modes, and
other logical properties remain outside the boundary. The existing geometry,
normal-flow/flex, overflow, display-list, raster, capture, point-hit, and
semantic/source-order owners remain unchanged, and
bounded block-level `display:flex` single-row placement for eligible direct
element children, and
bounded one-value non-negative fixed-pixel `gap` spacing between visible flex
items, and bounded `justify-content:normal|flex-start|center|flex-end|space-between|
space-around|space-evenly|stretch` free-space placement for eligible fixed-width flex
rows,
bounded explicit case-insensitive `gap:inherit` propagation through the private
parent-style chain, copying computed row and column gap components while omitted
`gap` remains local with the bounded `0` fallback; bounded explicit
case-insensitive `row-gap:inherit` and `column-gap:inherit` copy their
corresponding parent components while omitted longhands remain local with the
bounded `0` fallback,
bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
source-order ties,
bounded explicit case-insensitive `order:inherit` propagation through the
private parent-style chain, while omitted `order` remains local with the
bounded `0` fallback; visual order remains separate from semantic/source
order,
bounded non-inherited `flex-grow:0..=1024`, `flex-shrink:0..=1024`, and
`flex-basis:auto|Npx` sizing with native fallbacks, finite `flex` shorthand
expansion, bounded grow/shrink allocation, base-size selection, and existing
min/max constraints,
bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
placement using explicit content height or the auto row's maximum item outer
height,
bounded non-inherited `flex-direction:column|column-reverse` placement for
eligible fixed-height, no-wrap flex containers with vertical justification,
row-gap, flexible lengths, horizontal item alignment, reverse placement, and
shared descendant/artifact consumers,
bounded `flex-wrap:wrap` for fixed-height `column|column-reverse` containers
with vertical line formation, horizontal `column-gap`, per-line flex sizing
and justification, bounded `align-content`, and shared descendant/artifact
consumers, and bounded `margin:auto` main- and cross-axis resolution for
eligible no-wrap row/row-reverse and fixed-height column/column-reverse
containers,
bounded rectangular
`overflow:hidden`/`overflow:clip` clips shared by paint,
viewport projection, and point hit testing, and bounded read-only diagnostics
for unsupported CSS input, plus bounded local same-document fragment
navigation and Rust history traversal, and bounded local opacity groups that
composite reduced-opacity rendered subtrees through transparent software layers.
The 049 boundary adds independent bounded `overflow-x:hidden`/`clip` and
`overflow-y:hidden`/`clip` clips through that same rectangular owner; it does
not add nested scrolling, scrollbars, or visible-overflow propagation.
The 050 boundary adds bounded physical `min-width`/`max-width` and
`min-height`/`max-height` constraints through the same box-model owner;
intrinsic and percentage sizing remain outside the boundary. The later 064
boundary adds only bounded fixed-width single-row `display:flex` placement;
the 065 boundary adds a bounded fixed-pixel `gap`; and the 066 boundary
adds bounded fixed-width row free-space placement. General flex/grid sizing
remains outside the claim. The 068 boundary adds bounded non-inherited
cross-axis alignment for eligible rows without changing their horizontal or
semantic/source order. The 069 boundary adds bounded non-inherited
`flex-direction:row|row-reverse` to those rows: reverse placement walks the
order-sorted sequence from the physical right edge, keeps physical margins
attached, maps existing gap and justification to physical edges, shifts
overflow as one bounded row, and reuses shared subtree artifacts, root
scrolling, hit testing, and paint while leaving semantic/source order
unchanged.
The completed 070 boundary adds bounded non-inherited `flex-wrap:nowrap|wrap`
to those rows. `wrap` forms integer physical lines from measured outer item
widths and the existing gap, reuses the 069 per-line direction and the
existing justification/alignment/artifact consumers, and stacks maximum-height
lines through root overflow while leaving cross-line distribution, row/column
gaps, flex sizing, and `wrap-reverse` outside the boundary. The design is
`8772a6a`, the implementation is `5c16185`, and local validation evidence is
recorded in `docs/plan/tasks/native-engine-070.md`; remote CI remains pending
because the branch is local-only.
The completed 071 boundary adds bounded non-inherited
`align-content:flex-start|center|flex-end|space-between` to wrapped rows. It
distributes only positive explicit-content-height remainder before or between
the existing physical lines, translates complete line artifact ranges after
per-line `align-items`, and leaves `nowrap` and auto-height geometry
unchanged. The contract is recorded in
`docs/plan/tasks/native-engine-071.md`; design is `71bd380`, implementation is
`cc1d602`, the final single-line coverage test is `050d41b`, and local
validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.
The completed 072 boundary adds bounded non-inherited
`align-content:space-around` to wrapped rows. It uses saturating integer
slot-center offsets from positive explicit-content-height remainder while
preserving the 071 line-record, artifact, overflow, and semantic owners. The
contract is recorded in `docs/plan/tasks/native-engine-072.md`; design is
`c8e5170`, implementation is `a1c8b56`, and local validation and cleanup
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.
The completed 073 boundary adds bounded non-inherited
`align-content:space-evenly` to wrapped rows. It uses equal leading,
inter-line, and trailing integer slots from positive explicit-content-height
remainder while preserving the 072 line-record, artifact, overflow, and
semantic owners. The contract is recorded in
`docs/plan/tasks/native-engine-073.md`; design is `23b62a3`, implementation is
`b4833a9`, and local validation and cleanup evidence are recorded in the task
file. The default-stack overflow in one existing large-Clap parser test is
documented there; the full native library suite passes with an explicit 8 MiB
test-thread stack. Remote CI remains pending because the branch is local-only.
The completed 074 boundary adds bounded non-inherited
`flex-wrap:wrap-reverse` to eligible fixed-width rows. It keeps source-order
line formation, reflects physical line origins from the cross-axis end, reuses
all bounded `align-content` offsets, and translates each complete line artifact
range with a signed document-pixel delta. The contract is recorded in
`docs/plan/tasks/native-engine-074.md`; design is `5f6c61a`, implementation is
`96fd15c`, and local validation and cleanup evidence are recorded in the task
file. The previously documented default-stack issue in one large-Clap parser
test remains a harness follow-up; the full native library suite passes with an
explicit 8 MiB test-thread stack. Remote CI remains pending because the branch
is local-only.
The completed 075 boundary adds explicit bounded non-inherited
`align-content:stretch` to wrapped fixed-width rows. Positive explicit
cross-axis remainder expands formed line boxes by deterministic integer shares,
then the existing `align-items` and complete-artifact passes place normal and
wrap-reverse rows through one coordinate owner. The contract is recorded in
`docs/plan/tasks/native-engine-075.md`; design is `4637863`, implementation is
`e26c0a4` with the diagnostics-fixture correction in `cc8b538`, and local
validation and cleanup evidence are recorded in that task file. Remote CI
remains pending because the branch is local-only.
The completed 076 boundary adds the explicit bounded non-inherited
`align-content:normal` keyword to the same wrapped fixed-width rows. It routes
positive explicit cross-axis remainder through the completed stretch line-box
owner for normal and wrap-reverse stacking while preserving the established
omitted-value `flex-start` fallback. Its contract is recorded in
`docs/plan/tasks/native-engine-076.md`; design is `b6c647e`, implementation is
`e3933b3`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 077 boundary adds explicit bounded non-inherited `row-gap`
spacing between adjacent formed lines in eligible wrapped fixed-width flex
rows. It accounts for that cross-line space once before the existing
`align-content` free-space owner and preserves the current main-axis-only `gap`
behavior. The contract is recorded in
`docs/plan/tasks/native-engine-077.md`; design is `99d70ef`, implementation is
`4b27b15`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 078 boundary completes the bounded flex gap family with one- and
two-value integer-pixel `gap`, explicit `row-gap`/`column-gap` longhands, and
source-order-aware shorthand/longhand cascade. One-value `gap` intentionally
supplies both axes, superseding the 077 compatibility behavior where it only
supplied the main axis. Its contract is recorded in
`docs/plan/tasks/native-engine-078.md`; design is `c6ebecd`, implementation is
`1bca33f`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 079 boundary adds bounded non-inherited integer `flex-grow`
weights to eligible row-flex items. Positive line free space is allocated
before justification with deterministic prefix-floor shares, while max-width
caps freeze and redistribute remainder through the same geometry owner. Its
contract is recorded in `docs/plan/tasks/native-engine-079.md`; design is
`a45fb01`, implementation is `1a930a3`, and local validation and cleanup
evidence are recorded in that task file. Remote CI remains pending because the
branch is local-only.
The completed 080 boundary adds bounded non-inherited integer `flex-shrink`
weights to eligible row-flex items. Negative line free space is allocated using
original-base-width weighted prefix-floor shares, while effective outer
`min-width` floors freeze and redistribute the remaining deficit through the
same geometry owner. Its contract is recorded in
`docs/plan/tasks/native-engine-080.md`; design is `46227de5`, implementation is
`b1414931`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 081 boundary defines bounded `flex-basis:auto|Npx` for eligible
row-flex items. Explicit bases override item `width`, use the existing
box-sizing and min/max conversion helpers, remain available to the completed
grow/shrink owner before line formation, and preserve one coordinate/state
owner across descendants and all visual, overflow, interaction, and semantic
consumers. The contract is recorded in
`docs/plan/tasks/native-engine-081.md`; design is `04cc4a3e`, implementation is
`299c93f9`, and local validation and cleanup evidence are recorded in that task
file. Remote CI remains pending because the branch is local-only.
The completed 082 boundary defines bounded `flex` shorthand expansion into the
existing non-inherited `flex-grow`, `flex-shrink`, and `flex-basis` computed
components. Common `none`, `auto`, bounded integer-factor, and bounded
pixel/`auto` basis forms share the existing declaration precedence, while
unsupported CSS-wide, percentage, fractional, and ambiguous forms remain
typed diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-082.md`; design is `71d1060d`, implementation is
`40f6fb1c`, and local validation and cleanup evidence are recorded there.
Remote CI remains pending because the branch is local-only.
The completed 083 boundary defines bounded `flex-flow` shorthand expansion into
the existing non-inherited `flex-direction` and `flex-wrap` computed
components. One-token forms reset the omitted component to its initial row or
nowrap value; two-token forms accept one bounded direction and one bounded wrap
keyword in either order. Unsupported columns, duplicates, CSS-wide, logical,
and ambiguous forms remain typed diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-083.md`; design is `80c6836e`, implementation is
`0291bf90`, and the strict-Clippy fix is `16e6d9aa`. Local validation and exact
target-cleanup evidence are recorded there. Remote CI remains pending because
the branch is local-only.
The completed 084 boundary adds bounded non-inherited `align-self:auto|flex-start|
center|flex-end` for eligible direct flex items. `auto` resolves to the parent
`align-items` value at the existing cross-axis placement decision; explicit
values override only that item and translate its complete subtree artifacts.
Stretch, baseline, logical, CSS-wide, and ambiguous forms remain typed
diagnostics. The contract is recorded in
`docs/plan/tasks/native-engine-084.md`; design is `36085e9c`, implementation is
`f2f99f66`, and focused/full/strict/documentation/cleanup evidence is recorded
there. Remote CI remains pending because the branch is local-only.
The completed 085 boundary expands bounded `place-content` shorthand into the
existing `align-content` and `justify-content` computed components. One shared
token is limited to values valid in both bounded axes; two tokens use explicit
cross-axis/main-axis order. Unsupported CSS-wide, logical, safe/unsafe,
ambiguous, and unsupported justify forms remain typed diagnostics. The
contract is recorded in `docs/plan/tasks/native-engine-085.md`; design is
`adc61a1f`, implementation is `02f866e6`, and focused/full/strict/documentation/
cleanup evidence is recorded there. Remote CI remains pending because the
branch is local-only.
The completed 086 boundary extends bounded non-inherited `align-self` with
`stretch` for eligible direct flex items. Auto-height items fill the existing
line cross size through the current box-model and complete-artifact owners;
explicit heights remain unchanged and use the bounded flex-start fallback.
The contract is recorded in `docs/plan/tasks/native-engine-086.md`; design is
`f92b7b9a`, implementation is `5ea2c8d1`, and strict layout lint cleanup is
`2c07c749`. Focused, full, strict, documentation, release-certification, and
exact target-cleanup evidence are recorded in the task file. Remote CI remains
pending because the branch is local-only.
The completed 087 boundary extends the parent `align-items` grammar with
explicit `stretch`. Children whose `align-self` remains `auto` reuse the 086
used-size and complete-artifact owner; explicit child overrides remain
authoritative and explicit heights stay fixed under the bounded flex-start
fallback. The omitted native fallback remains `flex-start`. The contract is
recorded in `docs/plan/tasks/native-engine-087.md`; design is `4708f663` and
implementation is `e0d051ce`. Focused/full/strict/release-certificate,
documentation, and exact-target evidence are recorded in the task file. The
first all-in-one certification run exposed one environment-sensitive Rust
Analyzer probe; its exact retry passed. Remote CI remains pending because the
branch is local-only.
The completed 088 boundary adds explicit parent `align-items:normal`. In the
supported row/row-reverse flex context, `normal` resolves `align-self:auto`
through the completed stretch used-size owner while retaining a distinct
computed keyword; the omitted native fallback remains `flex-start`. The
contract and evidence are recorded in `docs/plan/tasks/native-engine-088.md`;
design is `162543f1` and implementation is `19d8d3f6`. Remote CI remains
pending because the branch is local-only.
The completed 089 boundary adds explicit child `align-self:normal`. In the
supported row/row-reverse flex context, the explicit item value reuses the
completed stretch used-size owner regardless of parent `align-items`, while
`align-self:auto` remains parent-controlled and the computed keyword remains
distinct. The contract and evidence are recorded in
`docs/plan/tasks/native-engine-089.md`; design is `95caf4a9` and
implementation is `1ee55c43`. Remote CI remains pending because the branch is
local-only.
The completed 090 boundary adds explicit `justify-content:space-around` to the
bounded fixed-width row/row-reverse flex-line owner. Positive main-axis free
space is distributed with deterministic integer cumulative offsets around the
existing item, gap, margin, and flex-sizing geometry; row-reverse mirrors the
same offsets and downstream artifacts remain shared. The complete contract and
evidence are recorded in `docs/plan/tasks/native-engine-090.md`; design is
`97c69d9d` and implementation is `d814784f`. Remote CI remains pending because
the branch is local-only.
The completed 091 boundary adds explicit `justify-content:space-evenly` to that
same bounded fixed-width row/row-reverse owner. Positive main-axis free space
is distributed into equal deterministic integer slots after existing item,
gap, margin, and flex-sizing geometry, with row-reverse mirroring and the same
shared downstream artifact consumers. Its contract and evidence are recorded
in `docs/plan/tasks/native-engine-091.md`; design is `be1a8e20` and
implementation is `118590f7`. Remote CI remains pending because the branch is
local-only.
The completed 092 boundary adds explicit `justify-content:normal` to that
owner. The computed keyword remains distinct while used placement reuses the
completed `flex-start` path, including explicit gaps, margins, row-reverse,
wrapping, and shared downstream artifacts. Its contract and evidence are
recorded in `docs/plan/tasks/native-engine-092.md`; design is `4cbaf338` and
implementation is `ce19db39`. Remote CI remains pending because the branch is
local-only.
The completed 093 boundary adds explicit `justify-content:stretch` to that
owner. The computed keyword remains distinct while used placement reuses the
completed `flex-start` path, including explicit gaps, margins, row-reverse,
wrapping, and shared downstream artifacts; the shared one-token and two-token
`place-content:stretch` forms are also covered. Its contract and evidence are
recorded in `docs/plan/tasks/native-engine-093.md`; design is `273b31db` and
implementation is `653f025e`. Remote CI remains pending because the branch is
local-only.
The completed 094 boundary extends that same owner to explicit
`flex-direction:column|column-reverse` for eligible fixed-height, no-wrap
containers. Vertical main-axis justification uses `row-gap`; bounded
grow/shrink/basis, horizontal item alignment, reverse placement, and complete
descendant/artifact consumers are mapped through the existing state owner.
Auto-height columns, wrapping, column-gap line distribution, logical writing
modes, and browser-wide Flexbox remain outside the contract recorded in
`docs/plan/tasks/native-engine-094.md`; design is `aea47b17`, implementation
checkpoints are `7dc92517` and `4e212151`, and local certification/cleanup
evidence is recorded there. Remote CI remains pending because the branch is
local-only.
The completed 095 boundary extends the completed column owner to
`flex-wrap:wrap` for fixed-height columns. It forms vertical main-axis lines,
uses `column-gap` between horizontal line boxes, reuses per-line bounded
flex-sizing/justification and existing `align-content`, and keeps complete
descendant/artifact consumers on the shared geometry path. `wrap-reverse`,
auto-height columns, and browser-wide Flexbox remain outside the contract
recorded in `docs/plan/tasks/native-engine-095.md`. Design is `598228a7`,
implementation is `96ea62a3`, and local certification evidence is recorded in
the task file. Remote CI remains pending because the branch is local-only.
The completed 096 boundary extends that same fixed-height column owner to
`flex-wrap:wrap-reverse`. It reflects horizontal line boxes and each line's
cross-axis `align-items`/`align-self` placement from the physical cross-end,
reuses 095 line formation, `column-gap`, `align-content`, main-axis direction,
and shared descendant/artifact consumers, and preserves source/semantic order.
The contract is recorded in `docs/plan/tasks/native-engine-096.md`; design is
`f5026f3c`, implementation is `6862aff6`, and local certification/cleanup
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.
Auto-height columns, intrinsic or percentage sizing, logical writing modes, and
browser-wide Flexbox remain outside the contract. Remote CI remains pending
because the branch is local-only.
The completed 097 boundary extends the existing no-wrap row and fixed-height
column owners to bounded `margin:auto` edges. It preserves numeric margins
while retaining auto-edge provenance, resolves positive main-axis space before
`justify-content` and positive cross-axis space before item alignment, and
keeps reverse directions and complete descendant/artifact consumers on the
shared geometry path. Wrapped lines, auto-height columns, intrinsic or
percentage sizing, and normal-flow auto margins remain outside the contract
recorded in `docs/plan/tasks/native-engine-097.md`; design is `2901c830`,
implementation is `815794ce`, and local certification/cleanup evidence is
recorded there. Remote CI remains pending because the branch is local-only.
The completed 098 boundary extends that auto-edge owner into eligible
`flex-wrap:wrap|wrap-reverse` row/row-reverse and fixed-height
column/column-reverse lines. Auto margins remain zero during line formation and
per-line sizing, then resolve against each final line after `align-content`
before the existing justification and alignment owners. Reverse directions,
wrap-reverse physical reflection, deterministic integer shares, and complete
descendant/artifact consumers remain shared; auto-height columns, new
intrinsic/percentage sizing, normal-flow auto margins, logical writing modes,
baseline alignment, grid, and browser-wide Flexbox remain outside
`docs/plan/tasks/native-engine-098.md`. The design is `a4b05f07`, the
implementation is `77a4b629`, and the final test-only checkpoint is `866a8862`;
local certification and cleanup evidence are recorded in the task file. Remote
CI remains pending because the branch is local-only.
The completed 099 boundary adds inherited `direction:ltr|rtl` to the existing
bounded Flexbox axis mapping under the current horizontal-tb assumptions. Rows
map logical inline start to the physical left/right main start, columns retain
their vertical main axis while reflecting horizontal cross-axis alignment and
wrapped line stacking, and reverse/wrap-reverse combinations remain on the
same owner. Source/semantic order, non-flex text bidi, logical properties,
vertical writing modes, and browser-wide directionality remain outside
`docs/plan/tasks/native-engine-099.md`. The design is `2f18abb4`, the
implementation is `3bf658e8`, and local gate evidence is recorded in the task
file. Remote CI remains pending because the branch is local-only.
The completed 100 boundary adds inherited `text-align:start|end` to the
existing fixed-cell inline-flow owner. Logical start/end resolve through
inherited `direction:ltr|rtl`; physical `left|right`, center, source order,
wrapped-line placement, and the fixed-cell no-bidi/shaping boundary remain
unchanged. The contract is `docs/plan/tasks/native-engine-100.md`; its design
is `2dae80fc`, implementation is `3380978c`, and local gate evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.
The completed 101 boundary adds inherited `text-align:justify` to the same
fixed-cell inline-flow owner. Only emitted collapsed ASCII separators on
soft-wrapped non-final lines receive deterministic source-order integer extra
advance; final and hard-break lines, preformatted flow, bidi/shaping, and
language-specific line breaking remain outside the contract. The contract is
`docs/plan/tasks/native-engine-101.md`; design is `959cbbc9`, implementation is
`8ff29aa1`, and the explicit word-spacing acceptance test is `15cf0c85`. Local
gate evidence is recorded in the task file, with exact isolated-target cleanup
recorded in the final cleanup checkpoint. Remote CI remains pending because
the branch is local-only.
The completed 102 boundary adds inherited `text-align-last` to that same
fixed-cell inline-flow owner. Only the final non-empty line flushed by a
block's normal completion path resolves the bounded `auto|left|center|right|
start|end` value; 101 soft-wrap justification, forced-break paths,
bidi/shaping, and language-specific line breaking remain outside the contract.
The completed contract is `docs/plan/tasks/native-engine-102.md`; design is
`fc396200`, implementation is `1157bf49`, and complete local gate plus
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.
The completed 103 boundary adds explicit inherited `text-align-last:justify`
to that same fixed-cell inline-flow owner. Only the final non-empty line
reaching the normal block completion flush may distribute positive free space
across eligible collapsed ASCII separators through the 101 spacing fields;
ordinary soft-wrap justification, preformatted/break-all paths, bidi/shaping,
and full text conformance remain outside the contract. The completed contract
is `docs/plan/tasks/native-engine-103.md`; design is `f261773f`, implementation
is `be5757ae`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.
The completed 104 boundary adds inherited `text-justify:auto|none|inter-word`
to that same fixed-cell text-spacing owner. `none` suppresses the existing
separator expansion for both ordinary `text-align:justify` soft wraps and
explicit `text-align-last:justify` final lines; `auto` and `inter-word` retain
the bounded ASCII-space algorithm. Inter-character spacing, shaping, bidi,
language-specific line breaking, and full text conformance remain outside the
contract. The completed contract is `docs/plan/tasks/native-engine-104.md`;
design is `6ca523f1`, implementation is `d83b24e4`, and complete local gate
plus exact-target cleanup evidence is recorded in the task file. Remote CI
remains pending because the branch is local-only.
The completed 105 boundary extends the existing inherited fixed-cell
`text-decoration` owner from `none|underline` to the single bounded values
`none|underline|overline|line-through`. It carries mutually exclusive line
state through immutable text commands and fixed-pixel software replay without
changing layout, source order, or semantic projections. Decoration colors,
thickness, style, offsets, combinations, font metrics, shaping, bidi, vertical
writing, and full text conformance remain outside the contract. The completed
contract is `docs/plan/tasks/native-engine-105.md`; design is `9002ae13`,
implementation is `ebfefefa`, and complete local gate plus exact-target
cleanup evidence is recorded in the task file. Remote CI remains pending
because the branch is local-only.
The completed 106 boundary extends that same inherited fixed-cell
`text-decoration` owner to distinct multi-token combinations in the
`text-decoration` shorthand. It carries one immutable three-bit line set
through display commands and fixed-pixel replay while preserving layout,
overflow, hit testing, and semantic/source order. `text-decoration-line`
longhand semantics, decoration colors, thickness, style, offsets, font
metrics, shaping, bidi, vertical writing, and full text conformance remain
outside the contract. The completed contract is
`docs/plan/tasks/native-engine-106.md`; design is `c0525afb`, implementation
is `baf680ee`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.
The completed 107 boundary extends the same fixed-cell decoration owner with a
local `text-decoration-color` value. Existing bounded `NativeColor` parsing
feeds a separate decoration color beside glyph color in one immutable text
command, so line pixels remain coupled to the existing origin, width, clip,
scroll, opacity, capture, and raster consumers while glyph pixels retain their
resolved text color. Explicit `transparent` remains meaningful. The completed
contract is `docs/plan/tasks/native-engine-107.md`; design is recorded in
`c5177215`, implementation is `2474efe6`, and local documentation/gate
closeout is recorded in the task file. The local implementation passed 144/144
native integration tests and the full current workspace gates; an ancillary
bounded diagnostics-wait fix is `64247bd4`. Explicit decoration-origin
propagation, `currentColor` syntax, decoration style/thickness/offset, and
full color/text conformance remain outside the contract. Remote CI remains
pending because the branch is local-only.
The completed 108 boundary exposes the same fixed-cell line bitset through the
bounded `text-decoration-line` longhand. `none`, `underline`, `overline`, and
`line-through` combinations reuse the existing declaration-order-aware
shorthand slot and one immutable text command, preserving the 107 color path
and all shared geometry, clipping, scrolling, opacity, capture, hit-test, and
semantic consumers. Full CSS longhand inheritance/decoration propagation,
style, thickness, offsets, and text conformance remain outside the contract.
The completed contract is `docs/plan/tasks/native-engine-108.md`; design is
`ea1bf881`, implementation is `4981ff82`, and complete local gate evidence is
recorded in the task file. The local native integration suite passed 145/145;
the full browser and glass-dev suites plus the strict documentation, package,
fuzz, and static gates also passed. Remote CI remains pending because the
branch is local-only.
The completed 109 boundary exposes deterministic `solid`, `dashed`, and
`dotted` presentation through the existing fixed-cell decoration owner. The
longhand reuses `NativeBorderStyle` and the current integer pattern helper,
carrying one style value in the immutable text command while preserving line
origins, geometry, clipping, scrolling, opacity, capture, hit-test, semantic,
and source-order consumers. Wavy/double styles, thickness, offsets,
decoration-origin propagation, and full CSS conformance remain outside the
contract. Design `93034cbf`, implementation `81069084`, inherited-style
coverage `b8ae87dc`, and local gate/cleanup evidence are recorded in
`docs/plan/tasks/native-engine-109.md`. Remote CI remains pending because the
branch is local-only.
The completed 110 boundary adds bounded inherited `text-decoration-thickness`
in the fixed-pixel range `1px` through `4px`. Each selected decoration keeps
the existing line y origin and paints a positive-y band through the same
immutable `TextRun`; style patterns reuse the existing integer helper with
periods scaled by thickness, and replay clamps externally constructed values to
the same 4px ceiling. Arbitrary lengths, font-derived values, centering,
offsets, baseline metrics, fragment continuity, and full CSS conformance remain
outside the contract. The contract is recorded in
`docs/plan/tasks/native-engine-110.md`; design is `1623c5f1`, implementation is
`157da4ad`, and complete local gate and cleanup evidence are recorded in the
task file. Remote CI remains pending because the branch is local-only.
The completed 111 boundary adds bounded inherited signed fixed-pixel
`text-underline-offset` in the range `-4px` through `4px` to the existing
underline raster owner. Negative values move only the underline toward
decreasing y and positive values toward increasing y; overline and
line-through origins remain unchanged, and the 110 thickness/style helper is
reused. `auto`, percentages, fractional/font-derived metrics,
decoration-origin propagation, fragment continuity, and full CSS conformance
remain outside the contract. The contract is recorded in
`docs/plan/tasks/native-engine-111.md`; design is `e2651dfd`, implementation is
`215b02a8`, current-claim docs are `e76a732a`, and complete local gate and
cleanup evidence are recorded in the task file. Remote CI remains pending
because the branch is local-only.
The completed dependency-ordered 112 boundary adds inherited
`text-decoration-style:double` through a dedicated text-decoration style type,
painting two solid bands with the existing resolved thickness and one
transparent separator pixel. The 111 underline offset, existing line origins,
x anchoring, clipping, scrolling, opacity, capture, and immutable text command
remain shared; border styling, layout, fragment continuity, and browser-wide
conformance remain outside the boundary. Implementation is `3bdd3b54`; final
local gate and cleanup evidence are recorded in
`docs/plan/tasks/native-engine-112.md`. Remote CI remains pending because the
branch is local-only.
The completed dependency-ordered 113 boundary adds inherited
`text-decoration-style:wavy` through the dedicated text-decoration style type,
painting a continuous eight-pixel fixed-cell wave with phase
`[0,1,2,1,0,-1,-2,-1]` and resolved thickness at each x column. Phase resets
at each immutable text run; the 112 double behavior, 111 underline offset,
line origins, clipping, scrolling, opacity, capture, and semantic consumers
remain shared. CSS metric centering, fragment continuity, antialiasing, and
browser-wide conformance remain outside the boundary. Implementation is
`0c6a9ddc`; complete local gate and cleanup evidence are recorded in
`docs/plan/tasks/native-engine-113.md`. Remote CI remains pending because the
branch is local-only.

The dependency-ordered 114 implementation is complete and recorded in
`docs/plan/tasks/native-engine-114.md`. It adds inherited
`text-decoration-skip-ink:auto|none` through a dedicated value carried by the
existing immutable text command. `auto` suppresses underline and overline
pixels only where the same fixed-cell text run has emitted glyph ink; `none`
preserves the existing replay, and line-through remains unchanged. Wavy,
thickness, offset, clipping, scroll, opacity, capture, hit testing, semantics,
source order, and layout remain shared. Font metrics, shaping, fragment
continuity, and browser-wide conformance remain outside the bounded design.
Implementation is checkpointed at `ceedf1d8` and synchronized documentation at
`d276d7b1`; focused, full-native, strict, package, fuzz, security, and static
local gates pass. Exact target reclamation and the remaining local-only
boundary are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The dependency-ordered 115 implementation is complete and recorded in
`docs/plan/tasks/native-engine-115.md`. It adds inherited
`text-decoration-skip-spaces:none|all` through the existing immutable text
command. `all` skips decoration pixels over same-run ASCII-space intervals,
including word, letter, and final-line justification spacing, for underline,
overline, and line-through; `none` preserves replay. The standard's `start` and
`end` modes, Unicode whitespace, line-boundary semantics, and full browser text
conformance remain outside this bounded design. Implementation is checkpointed
at `1c0bd484`; local native/dev, strict, package, fuzz, security, formatting,
and static certification is complete and the exact cleanup is recorded in
`docs/plan/tasks/native-engine-115.md`. The browser registry-backed publish
dry-run passed; the dev registry-backed verification is blocked by the
immutable public `glass-browser 0.3.14` API surface, while the local dev
no-verify packaging dry-run passed. No upload was attempted. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 116 implementation is complete and recorded in
`docs/plan/tasks/native-engine-116.md`. It extends inherited
`text-decoration-skip-spaces` with explicit `start`, `end`, and unordered
`start end` line-edge modes. The authoritative block-owned flow flush marks
the first and last text items and carries immutable provenance beside the
display-list text commands, so raster replay skips only leading/trailing
fixed-cell ASCII-space intervals without changing the 115 `none|all` behavior.
Nested inline temporary flows cannot claim a line edge. Implementation is
checkpointed at `a671a559` and `db7585f7`; focused, full-native, dev, strict,
package, fuzz, security, formatting, and static local gates are recorded in
the task file. Remote CI remains pending because the checkout is local-only.
The dependency-ordered 117 implementation is complete at `5e65aadf` and
recorded in `docs/plan/tasks/native-engine-117.md`. The existing decoration
replay owner now classifies literal/preformatted tabs and non-breaking spaces
with Rust's bounded `char::is_whitespace()` property for `all` and the
selected 116 line edges, while normal collapsing, fixed-cell geometry, line
provenance, and the explicit omission fallback remain unchanged. Focused,
full-native, two-crate, package, fuzz, security, and static local
certification passed; exact evidence and cleanup are recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered 118 implementation is complete at `6f8e89fc` and
recorded in `docs/plan/tasks/native-engine-118.md`. The parser accepts only the
explicit, case-insensitive `text-decoration-skip-spaces: initial` keyword and
resolves it through the existing `StartAndEnd` value. The established
omitted-declaration fallback remains `none`; no general CSS-wide keyword
machinery, layout owner, display-list field, dependency, default-feature, or
crate-boundary change was made. Focused, full-native, two-crate, strict,
package, fuzz, security, formatting, and static local certification passed;
exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 119 implementation is complete at `1118bf2b` and
recorded in `docs/plan/tasks/native-engine-119.md`. It accepts explicit,
case-insensitive `text-decoration-skip-spaces: inherit` through a private
declaration-only value and resolves it at the existing parent-style boundary;
the public finite paint enum, display-list schema, rasterizer, and omitted
fallback remain unchanged. Focused, full-native, two-crate, strict, package,
fuzz, security, formatting, and static local certification passed; exact
evidence and cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered 120 implementation is complete at `897bd648` and
recorded in `docs/plan/tasks/native-engine-120.md`. It accepts explicit,
case-insensitive `text-decoration-skip-spaces: unset` through the same private
declaration-only boundary and resolves it as inherited parent state; the public
finite paint enum, display-list schema, rasterizer, and omitted fallback remain
unchanged. Focused, full-native, two-crate, strict, package, fuzz, security,
formatting, and static local certification passed; exact evidence and cleanup
are recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 121 implementation is complete at `f361415a` and
recorded in `docs/plan/tasks/native-engine-121.md`. It accepts explicit,
case-insensitive `text-decoration-skip-spaces: revert` through a distinct
private declaration-only state and resolves it at the existing one-author-origin
inherited fallback boundary; the public finite paint enum, display-list,
rasterizer, and omitted fallback remain unchanged. `revert-layer`, cascade
layers, multiple style origins, general CSS-wide keyword machinery, and all
layout/raster changes remain outside this slice. Focused, full-native,
two-crate, strict, package, fuzz, security, formatting, and static local
certification passed; exact evidence and cleanup are recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered 122 implementation is complete at `1291fc2c`, with the
strict-Clippy parser-context follow-up at `efb5bdfc`, and is recorded in
`docs/plan/tasks/native-engine-122.md`. It adds bounded top-level named
cascade layers with private first-appearance ordering, a 15-layer limit, an
unlayered/inline bucket above named layers, and explicit
`text-decoration-skip-spaces: revert-layer` rollback through lower candidates
and the existing inherited/root fallback. Invalid layer forms remain typed
diagnostics; layer metadata never reaches public paint, layout, display-list,
or raster artifacts. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.
The dependency-ordered 123 implementation is complete at `af644112` and is
recorded in `docs/plan/tasks/native-engine-123.md`. It reuses the bounded layer
registry and private rollback state for inherited
`text-decoration-skip-ink: revert-layer`, keeping the finite `Auto|None`
public value and existing glyph-intersection replay unchanged. Parser,
cascade, display-list, and decoded-raster regressions passed; general
CSS-wide keyword machinery, multiple origins, layer statements, and
unsupported values remain typed diagnostics. Exact local gate, issue-sync,
and regenerable-output cleanup evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered 124 implementation is complete at `d6c8bc70` and is
recorded in `docs/plan/tasks/native-engine-124.md`. It reuses the bounded layer
registry and private rollback state for inherited
`text-decoration-style: revert-layer`, keeping the finite
`Solid|Dashed|Dotted|Double|Wavy` public value and existing fixed-cell pattern
replay unchanged. Parser, cascade, display-list, command, and decoded-raster
regressions passed; general CSS-wide keyword machinery, multiple origins,
layer statements, and unsupported values remain typed diagnostics. Exact local
gate, documentation-audit, issue-sync, and regenerable-output cleanup evidence
is recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 125 implementation is complete at `271702ae` and is
recorded in `docs/plan/tasks/native-engine-125.md`. It reuses the bounded layer
registry and private rollback state for inherited
`text-decoration-thickness: revert-layer`, keeping the finite `1px` through
`4px` value and existing one-to-four-cell decoration geometry unchanged.
Parser, cascade, display-list, command, and decoded-raster regressions passed
for underline, overline, and line-through; general CSS-wide keyword machinery,
multiple origins, layer statements, and unsupported values remain typed
diagnostics. Exact local gate, issue-sync, and regenerable-output cleanup
evidence is recorded in the task. Remote CI remains pending because the
checkout is local-only.
The dependency-ordered 126 implementation is complete at `3ffe86f8` and is
recorded in `docs/plan/tasks/native-engine-126.md`. It reuses the bounded layer
registry and private rollback state for inherited
`text-underline-offset: revert-layer`, keeping the finite signed `-4px`
through `4px` value and existing underline-only translation unchanged.
Parser, cascade, display-list, command, and decoded-raster regressions passed;
general CSS-wide keyword machinery, multiple origins, layer statements, and
unsupported values remain typed diagnostics. Exact local gate, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 127 implementation is complete at `50a36545` and is
recorded in `docs/plan/tasks/native-engine-127.md`. It reuses the bounded layer
registry and private rollback state for local `text-decoration-color:
revert-layer`, preserving the existing `Option<NativeColor>` no-candidate
fallback and separate glyph/decoration paint owner. Parser, cascade,
display-list, command, and decoded-raster regressions passed; general
CSS-wide keyword machinery, `currentColor`, multiple origins, layer
statements, and unsupported values remain typed diagnostics. Exact local gate,
issue-sync, and regenerable-output cleanup evidence is recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered 128 implementation is complete at `15fc761c` and is
recorded in `docs/plan/tasks/native-engine-128.md`. It reuses the bounded layer
registry for case-insensitive `text-decoration-line: revert-layer` and
`text-decoration: revert-layer`, preserving their shared inherited three-bit
line-state owner, declaration-order interaction, unlayered/inline bucket,
display-list, command, and fixed-cell raster geometry. Parser, cascade,
display-list, inherited fallback, and decoded-raster regressions passed; other
CSS-wide keywords, multiple origins, layer statements, and unsupported values
remain typed diagnostics. Exact local gate, documentation-audit, issue-sync,
and regenerable-output cleanup evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered 129 implementation is complete in `d26033af`, with the
fixture assertion correction in `c32aeafe` and the diagnostic-classifier fix in
`36a0f68`, and is recorded in
`docs/plan/tasks/native-engine-129.md`. It adds private, case-insensitive
`revert-layer` declarations for inherited `text-align`, `text-align-last`, and
`text-justify`, reusing the existing 15-layer registry and unlayered/inline
precedence while preserving the finite alignment values, direction mapping,
final-line alignment, separator justification, and all existing line/artifact
owners. The public computed values and downstream geometry remain unchanged;
the supported-value diagnostic path recognizes the same declaration forms.
The dependency-ordered 130 implementation is complete in `d7f4d7ca` and is
recorded in `docs/plan/tasks/native-engine-130.md`. It adds private,
case-insensitive `white-space: revert-layer` declarations through the
existing 15-layer and unlayered/inline cascade boundary while preserving the
five finite whitespace modes, inherited/root fallback, and all existing
line-flow/artifact owners. Focused parser/cascade and complete native-engine
integration gates, the full affected library suite, and strict affected-
package Clippy passed locally; the package library gate used an explicit 32 MiB
test-thread stack because one pre-existing CLI test overflows the default
thread stack. Remote CI remains pending because the checkout is local-only.
The dependency-ordered 131 implementation is complete in `e35a13fd` and is
recorded in `docs/plan/tasks/native-engine-131.md`. It adds private,
case-insensitive `line-height: revert-layer` declarations through the
existing 15-layer and unlayered/inline cascade boundary while preserving the
positive-pixel line-height grammar, inherited/root fallback, and all existing
flow/artifact owners. Focused parser/cascade and complete native-engine
integration gates, the full affected library suite, and strict affected-
package Clippy passed locally; the package library gate used an explicit 32 MiB
test-thread stack because one pre-existing CLI test overflows the default
thread stack. Remote CI remains pending because the checkout is local-only.
The dependency-ordered 132 implementation is complete in `4a46862f` and is
recorded in `docs/plan/tasks/native-engine-132.md`. It reuses the same private
bounded layer boundary for inherited `direction: revert-layer`, preserving
the finite `ltr|rtl` value, logical text-edge mapping, flex directionality,
wrapped-line placement, source/semantic order, and shared artifact owners.
Focused, full-native, affected-library, and strict affected-package local
gates passed; exact test and cleanup evidence is recorded in the task. Remote
CI remains pending because the checkout is local-only.
The dependency-ordered 133 implementation is complete in `56944c83` and is
recorded in `docs/plan/tasks/native-engine-133.md`. It extends the bounded
rollback boundary to non-inherited `flex-direction: revert-layer`, retaining
the finite row/row-reverse/column/column-reverse values, local `row` fallback,
finite `flex-flow` expansion, and existing row/column/wrapped flex owners.
Focused, full-native, affected-library, and strict affected-package local
gates passed; exact test and cleanup evidence is recorded in the task. Remote
CI remains pending because the checkout is local-only.
The dependency-ordered 134 implementation is complete in `f2e20121` and is
recorded in `docs/plan/tasks/native-engine-134.md`. It extends the bounded
rollback boundary to non-inherited `flex-wrap`, `justify-content`,
`align-items`, `align-self`, and `align-content`, retaining their native
fallbacks, finite `flex-flow`/`place-content` expansion, and existing flex
line/item artifact owners. Focused, full-native, affected-library, and strict
affected-package local gates passed; exact test and cleanup evidence is
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 135 implementation is complete in `74195032` and is
recorded in `docs/plan/tasks/native-engine-135.md`. It extends the bounded
rollback boundary to non-inherited `order`, `flex-grow`, `flex-shrink`, and
`flex-basis`, retaining finite `flex` expansion, local defaults, stable visual
order, grow/shrink allocation, base-size selection, min/max constraints, and
existing layout/artifact owners. Focused, full-native, affected-library, and
strict affected-package local gates passed; exact test and cleanup evidence is
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 136 implementation is complete in `710ed3bb` and is
recorded in `docs/plan/tasks/native-engine-136.md`. It extends
the same private bounded layer resolver to standalone case-insensitive
`flex:revert-layer` by writing rollback candidates for the existing grow,
shrink, and basis components, while preserving finite shorthand expansion,
same-block longhand precedence, local fallbacks, and all current
layout/display-list/hit-test/raster owners. Focused, full-native,
affected-library, and strict affected-package local gates passed; exact
evidence and cleanup are recorded in the task. Other CSS-wide keywords,
multiple origins, layer statements, intrinsic or percentage sizing, and
browser-wide Flexbox conformance remain outside the boundary. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 137 implementation is complete in `7da4dfd5` and is
recorded in `docs/plan/tasks/native-engine-137.md`. Design was recorded in
`79e2d2fa`. It extends the same private bounded layer resolver to standalone,
case-insensitive `flex-flow:revert-layer` and `place-content:revert-layer`,
writing rollback candidates for their existing direction/wrap and
align-content/justify-content components while preserving finite shorthand
expansion, same-block longhand precedence, local fallbacks, and all current
flex layout/display-list/hit-test/raster owners. Focused parser/cascade,
integration, full-native, affected-library, strict Clippy, formatting, and
static documentation gates passed locally; exact evidence and cleanup are
recorded in the task. Other CSS-wide keywords, multiple origins, layer
statements, intrinsic or percentage sizing, and browser-wide Flexbox
conformance remain outside the boundary. Remote CI remains pending because the
checkout is local-only.
The dependency-ordered 138 implementation is complete in `bded96ae` and is
recorded in `docs/plan/tasks/native-engine-138.md`; design was recorded in
`656dc37d`. It extends the same private bounded layer resolver to standalone,
case-insensitive `gap:revert-layer`, `row-gap:revert-layer`, and
`column-gap:revert-layer`, retaining finite integer-pixel shorthand expansion,
independent row/column component fallback, same-block shorthand/longhand
precedence, and all current row/column flex layout and artifact owners.
Focused parser/cascade, integration, full-native, affected-library, strict
Clippy, formatting, and static documentation gates passed locally; exact test
and cleanup evidence is recorded in the task. Percentages, other CSS-wide
keywords, multiple origins, layer statements, and browser-wide gap conformance
remain outside the boundary. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 139 implementation is complete in `0114659d` (design
`16589ae4`) and is recorded in `docs/plan/tasks/native-engine-139.md`. It
extends the same private bounded layer resolver to standalone, case-insensitive
`revert-layer` for inherited `text-transform`, `font-weight`, `font-style`, and
`word-break`, preserving finite public values, parent/root fallback, fixed-cell
layout, wrapping, display-list, raster, overflow, capture, hit-test, and
semantic/source-order owners. Unicode case mapping, font metrics, other
word-break modes, multiple origins, and browser-wide text conformance remain
outside the boundary. Focused parser/cascade and integration tests, full-native
integration/library tests, strict affected-package Clippy, formatting, and
static documentation gates passed locally; exact evidence and cleanup are
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 140 implementation is complete in `7d40cf87` (design
`34f8ec1a`) and is recorded in `docs/plan/tasks/native-engine-140.md`. It
extends the same private bounded layer resolver to standalone, case-insensitive
`revert-layer` for inherited `word-spacing` and `letter-spacing`, preserving
finite non-negative pixel values, inherited/root fallback, and the existing
spacing, wrapping, alignment, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners. Negative/relative/percentage/
fractional spacing, cross-fragment semantics, font metrics, multiple origins,
and browser-wide text conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered 141 implementation is complete in `271bfaf2` (design
`0d1f7281`) and is recorded in `docs/plan/tasks/native-engine-141.md`. It
extends the same private bounded layer resolver to standalone,
case-insensitive `revert-layer` for inherited `vertical-align`, preserving
finite `baseline|top|middle|bottom` values, inherited/root fallback, and the
existing inline line-item, text-fragment, display-list, raster, overflow,
capture, hit-test, and semantic/source-order owners. Baseline metrics, lengths,
percentages, bidi, writing modes, multiple origins, and browser-wide text
conformance remain outside the boundary. Focused parser/cascade and
integration tests, full-native integration/library tests, strict affected-
package Clippy, formatting, and static documentation gates passed locally;
exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 142 implementation is complete in `be860447` (design
`2794365f`) and is recorded in `docs/plan/tasks/native-engine-142.md`. It
extends the same private bounded layer resolver to standalone,
case-insensitive `revert-layer` for the local `text-indent` and
`text-overflow` owners, preserving finite non-negative fixed-pixel indentation,
`clip|ellipsis`, local `0px`/`clip` fallbacks, and the existing first-line flow,
eligible clipped-nowrap truncation, text-fragment, display-list, raster,
overflow, capture, hit-test, and semantic/source-order owners. Negative or
hanging indentation, percentages, font-relative units, inherited
text-overflow, marker customization, multiple origins, and browser-wide text
conformance remain outside the boundary. Focused parser/cascade and
integration tests, full-native integration/library tests, strict affected-
package Clippy, formatting, and static documentation gates passed locally;
exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 143 implementation is complete in `b55751da` (design
`edc29d7c`) and is recorded in `docs/plan/tasks/native-engine-143.md`. It
extends the same private bounded layer resolver to standalone, case-insensitive
`revert-layer` for local `width`, `height`, `min-width`, `max-width`,
`min-height`, and `max-height`, preserving finite non-negative pixel dimensions,
absent local fallbacks, and the existing box-model, normal-flow, flex, overflow,
capture, hit-test, display-list, raster, and semantic/source-order owners.
Percentages, negative dimensions, intrinsic sizing, aspect ratio, multiple
origins, and browser-wide CSS sizing conformance remain outside the boundary.
Focused parser/cascade and integration tests, full-native integration/library
tests, strict affected-package Clippy, formatting, and static documentation
gates passed locally; exact evidence and cleanup are recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered 144 implementation is complete in `7ce9c52c` (design
`255a1ac8`) and is recorded in `docs/plan/tasks/native-engine-144.md`. It
extends the same private bounded layer resolver to standalone, case-insensitive
`revert-layer` for local `box-sizing`, physical padding and margin edges,
including shorthand/longhand rollback and bounded `margin:auto`, preserving
independent edge ownership, content-box/zero local fallbacks, and the existing
box-model, normal-flow, flex, overflow, capture, hit-test, display-list, raster,
and semantic/source-order owners. Percentages, negative/logical edges, margin
collapsing, multiple origins, and browser-wide box-model conformance remain
outside the boundary. Focused parser/cascade and integration tests, full-native
integration/library tests, strict affected-package Clippy, formatting, and
static documentation gates passed locally; exact evidence and cleanup are
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 145 implementation is complete in `997d4aa7` (design
`553c5f89`) and is recorded in `docs/plan/tasks/native-engine-145.md`. It
extends the same private bounded layer resolver to standalone,
case-insensitive `revert-layer` on local `background-color` and inherited
`color`, preserving independent paint-color candidate ownership,
`None`/inherited fallbacks, fill/text display-list and raster consumers, and
the existing text-decoration-color owner. Border-color, `currentColor`,
gradients, system colors, multiple origins, and browser-wide CSS color
conformance remain outside the boundary. Focused parser/cascade and
integration tests, full-native integration/library tests, strict affected-
package Clippy, formatting, and static documentation gates passed locally;
exact evidence and cleanup are recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered 146 implementation is complete in `462d2a70` (design
`d1cd1eab`) and is recorded in `docs/plan/tasks/native-engine-146.md`. It
extends standalone, case-insensitive `revert-layer` to local `overflow`,
`overflow-x`, and `overflow-y` through independent x/y candidates, preserving
the existing visible fallback and shared paint, viewport projection, point-hit,
root-overflow, capture, and semantic/source-order owners. Nested scrolling,
scrollbars, `visible`/`auto`/`scroll` used-value parity, multiple origins, and
browser-wide CSS overflow conformance remain outside the boundary. Focused
parser/cascade and integration tests, full-native integration/library tests,
strict affected-package Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered 147 implementation is complete in `74cc1cf9` (design
`f76720f7`) and is recorded in `docs/plan/tasks/native-engine-147.md`. It
extends standalone, case-insensitive `revert-layer` to the local bounded
one-to-four-value integer `border-radius` shorthand, preserving the zero-corner
fallback and the existing rounded fill, border, point-hit, capture, raster,
overflow, and semantic/source-order owners. Elliptical, percentage,
corner-longhand, nested-clip, anti-aliasing, multiple-origin, and browser-wide
border-radius conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered 148 implementation is complete in `d3f89a6c` (design
`d882d846`) and is recorded in `docs/plan/tasks/native-engine-148.md`. It
extends the same private bounded layer resolver to standalone,
case-insensitive `revert-layer` on the existing local 8-bit `opacity` owner,
preserving the full-opacity fallback, reduced-opacity group markers and
software compositing, and the existing layout, point-hit, capture, raster,
overflow, and semantic/source-order owners. Inherited opacity,
stacking-context/blending parity, filters, animation, multiple origins, and
browser-wide opacity conformance remain outside the boundary. Focused parser/
cascade and integration tests, full-native integration/library tests, strict
affected-package Clippy, and formatting passed locally. Static documentation
gates passed with 562 Markdown documents, 83 current documents, 57
previous-version hits, 656 semantic-audit hits, and 0 current-claim failures.
Documentation coverage passed with 562 Markdown files, 345 full-product MCP
tools (100 browser-only), 17 examples, and 22 public modules; depth passed
with 93 guides and 19 substantive contracts; parity passed for 14 capabilities
across 4 targets; TUI passed at 15 implementation help keys/63 documentation
markers; adapters passed at 5; reliability passed at 6 scenarios across 4
targets; and Web IR passed at 8 fixtures/8 scenarios/11 categories. Remote CI
remains pending because the checkout is local-only.
The dependency-ordered 149 implementation is complete in `6a7dc305`, with the
diagnostic compatibility fix in `3072f6e5` (design `bd3b87b3`), and is recorded
in `docs/plan/tasks/native-engine-149.md`. It reuses the same private bounded
layer resolver for standalone, case-insensitive `revert-layer` on the existing
local `display` and `visibility` owners, preserving the normal-flow
`display:auto`/visible fallbacks and the existing hidden-subtree, normal-flow,
point-hit, display-list, capture, raster, and semantic/source-order owners.
Inherited visibility, display decomposition, formatting-context parity,
table/ruby/flow-root details, animation, multiple origins, and browser-wide
CSS display/visibility conformance remain outside the boundary. Focused parser/
cascade and integration tests, full-native integration/library tests, strict
affected-package Clippy, feature rustdoc, formatting, and diff checks passed
locally; final static documentation evidence is recorded in the task. Remote
CI remains pending because the checkout is local-only.
The dependency-ordered 150 implementation is complete in `1fdbe75d` (design
`27cf6c1a`) and is recorded in `docs/plan/tasks/native-engine-150.md`. It
reuses the same private bounded layer resolver for standalone, case-insensitive
`revert-layer` on the existing physical `border`, `border-top`,
`border-right`, `border-bottom`, and `border-left` owners, preserving the
zero-width/no-paint fallback and the existing box-model inset, border
display-list, capture, raster, point-hit, and semantic/source-order owners.
Logical sides, border-image, gradients, other border styles, animation,
multiple origins, and browser-wide CSS border conformance remain outside the
boundary. Focused parser/cascade and integration tests, full-native
integration/library tests, strict affected-package Clippy, feature rustdoc,
formatting, and diff checks passed locally; final static documentation evidence
is recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered 151 implementation is complete in `c26482b7` (design
`4f23d85a`) and is recorded in `docs/plan/tasks/native-engine-151.md`. It adds
standalone, case-insensitive `border-color` and physical
`border-top-color|border-right-color|border-bottom-color|border-left-color`
with one-to-four-value expansion, independent private per-side color
candidates, same-block declaration order, and `revert-layer` rollback, while
preserving the existing border width/style, zero-width, box-model,
display-list, capture, raster, point-hit, and semantic/source-order owners.
Logical sides, standalone border-width/style, `currentColor`, gradients,
border-image, animation, multiple origins, `!important` inversion, and
browser-wide border conformance remain outside the boundary. Focused parser/
cascade and integration tests, full-native integration/library tests, strict
Clippy, feature rustdoc, two-crate check/build, formatting, and static
documentation gates are recorded in the task; remote CI remains pending
because the checkout is local-only.
The dependency-ordered 152 implementation is complete in `7dfcc7f5` (design
`ff4d7803`) and is recorded in `docs/plan/tasks/native-engine-152.md`. It adds
standalone, case-insensitive `border-width` and physical
`border-top-width|border-right-width|border-bottom-width|border-left-width`
with one-to-four-value expansion, an independent private per-side width
stream, and `revert-layer` rollback, while preserving the existing border
style/color, zero-width, box-model, display-list, capture, raster, point-hit,
and semantic/source-order owners. Width-only declarations do not invent a
style or paint a border. Standalone border-style, logical sides, `currentColor`,
gradients, border-image, fractional/percentage widths, animation, multiple
origins, `!important` inversion, and browser-wide border conformance remain
outside the boundary. Focused parser/cascade and integration tests, full-native
integration/library tests, strict Clippy, feature rustdoc, two-crate
check/build, formatting, and static documentation gates are recorded in the
task; remote CI remains pending because the checkout is local-only.
The dependency-ordered 153 implementation is complete in `6169cabc` (design
`77d81fdc`) and is recorded in `docs/plan/tasks/native-engine-153.md`. It adds
standalone, case-insensitive `border-style` and physical
`border-top-style|border-right-style|border-bottom-style|border-left-style`
with one-to-four-value expansion, an independent private per-side style
stream, same-block declaration order, and `revert-layer` rollback. Resolved
width, style, and color components compose only after their independent
resolution; width-only or style-only declarations cannot invent missing paint
components. Existing zero-width/no-paint, box-model, display-list, capture,
raster, point-hit, and semantic/source-order owners remain unchanged. `none`,
logical sides, `currentColor`, gradients, border-image, unsupported styles,
animation, multiple origins, `!important` inversion, and browser-wide border
conformance remain outside the bounded surface. Focused parser/cascade and
integration tests, full-native integration/library tests, strict Clippy,
feature rustdoc, two-crate check/build, formatting, and static documentation
gates are recorded in the task; remote CI remains pending because the checkout
is local-only.
The dependency-ordered 154 implementation is complete in `875cdad8` (design
`e7c9ad40`) and is recorded in `docs/plan/tasks/native-engine-154.md`. It adds
explicit physical `border-style:none` to the one-to-four-value shorthand and
four physical style longhands through a private no-paint sentinel, so a
winning `none` blocks lower styles and converts to the existing no-side/
zero-width behavior before layout and artifacts. The public paint enum and
display-list schema remain unchanged; `hidden`, other border styles, logical
sides, `currentColor`, gradients, border-image, and browser-wide border
conformance remain outside the boundary. Focused parser/cascade and artifact
integration tests, full-native integration/library tests, strict Clippy,
feature rustdoc, two-crate check/build, formatting, and static documentation
gates passed locally; exact evidence and bounded target cleanup are recorded
in the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered 155 implementation is complete in `2b07f109` (design
`37126fa1`) and is recorded in `docs/plan/tasks/native-engine-155.md`. It adds
explicit physical `border-style:hidden` to the bounded one-to-four-value
shorthand and four physical style longhands through a distinct private
no-paint sentinel. In the current non-table engine, a winning `hidden` blocks
lower styles and uses the same no-side/zero-width result as `none`, while
retaining a private distinction for future collapsed-table conflict
resolution. Public enums and display-list schemas remain unchanged; table
conflict resolution and other border styles remain outside the boundary.
Focused parser/cascade and artifact integration tests, full-native integration/
library tests, strict Clippy, feature rustdoc, two-crate check/build,
formatting, and static documentation gates passed locally; exact evidence and
bounded target cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.
The dependency-ordered 156 implementation is complete in `5270d013` (design
`13c8e90f`) and is recorded in `docs/plan/tasks/native-engine-156.md`. It batches the
painted physical styles `double`, `groove`, `ridge`, `inset`, and `outset`
through the bounded style parser, public computed paint enum, and deterministic
software replay. Integer-pixel double stripes and two-tone/edge-directed
shading are explicit native rules, not browser-fidelity claims; the existing
display-list shape, clipping, capture, point-hit, and semantic/source-order
owners remain stable. Logical sides, table conflict resolution, gradients,
border images, and other general CSS border conformance remain outside the
boundary. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
formatting, and static documentation gates are recorded in the task; remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered 157 slice is recorded in
`docs/plan/tasks/native-engine-157.md`; design is `ab9d6628` and implementation
is `fb2c56a2`. It accepts only exact case-insensitive omitted-component `none`
in the complete and physical border shorthands, routes it through a private
declaration wrapper into the existing no-paint style stream, and preserves the
current public/artifact schemas. Width and color do not receive synthetic
candidates, so a winning `none` blocks paint while a later bounded
`revert-layer` can expose an existing lower painted component. Arbitrary
omitted-component defaults, `border:hidden`, CSS-wide resets, table conflict
resolution, and browser-wide border conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting,
and static documentation gates passed locally; exact target cleanup is recorded
in the task. Remote CI remains pending because the branch is local-only.
The completed dependency-ordered 158 slice is recorded in
`docs/plan/tasks/native-engine-158.md`; design is `6041a479` and implementation
is `f04623fc`. It accepts only exact case-insensitive omitted-component
`hidden` in the complete and physical border shorthands, routes it through the
existing private hidden style stream, and preserves the current public/artifact
schemas and future table-conflict distinction. Width and color do not receive
synthetic candidates, so a winning `hidden` blocks paint while a later bounded
`revert-layer` can expose an existing lower painted component. Arbitrary
omitted-component defaults, CSS-wide resets, logical sides, table conflict
resolution, and browser-wide border conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and
static documentation gates passed locally; exact target cleanup is recorded in
the task. Remote CI remains pending because the branch is local-only.
The completed dependency-ordered 159 slice is recorded in
`docs/plan/tasks/native-engine-159.md`; design is `bdbdb208` and implementation
is `329b3cfb`. It accepts bounded complete `Npx hidden color` values for the
complete and physical border shorthands, carries declared width/color through
private component candidates, maps only style to the existing private hidden
sentinel, and keeps public/artifact schemas unchanged. Width and color cannot
resurrect a hidden side in current non-table composition, while a later
bounded `revert-layer` can expose a lower painted style with the retained
complete components. Arbitrary omitted defaults, CSS-wide resets, logical
sides, table conflict resolution, and browser-wide border conformance remain
outside the boundary. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, formatting, and static documentation gates passed locally; exact
target cleanup is recorded in the task. Remote CI remains pending because the
branch is local-only.
The completed dependency-ordered `native-engine-160` slice is recorded in
`docs/plan/tasks/native-engine-160.md`; design is `bf1a7236` and implementation
is `a880c570`. It accepts bounded complete `Npx none color` values on the
complete and physical border shorthands, carries declared width/color through
private component candidates, projects only the existing private `None` style
sentinel, and keeps current non-table composition no-paint/no-side with public
schemas unchanged. Exact omitted-component none/hidden, complete hidden and
painted values, and standalone `revert-layer` remain supported. Arbitrary
omitted defaults, CSS-wide resets, logical sides, table conflict resolution,
and browser-wide border conformance remain outside the boundary. Focused,
full-native/library, strict Clippy, rustdoc, two-crate, formatting, static
documentation, and bounded cleanup gates are recorded in the task; remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-161` slice is recorded in
`docs/plan/tasks/native-engine-161.md`; design is `c5a58f29` and implementation
is `299de40d` with the test-lint follow-up `2042fb3e`. It adds standalone
physical `border-color` and `border-top|right|bottom|left-color` `currentColor`
substitution through private declaration state, resolving against the existing
element/inherited color owner while keeping public/artifact schemas and broader
CSS color semantics unchanged. Focused, full-native/library, strict Clippy,
rustdoc, two-crate, formatting, static documentation, and bounded cleanup gates
passed locally; exact evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The completed dependency-ordered `native-engine-162` slice is recorded in
`docs/plan/tasks/native-engine-162.md`; design is `4272f0fa` and implementation
is `44b887c4`. It extends the private deferred-color path to complete physical
`Npx <style> currentColor` values for painted, `none`, and `hidden` forms,
resolving explicit current color from the local or inherited element color while
preserving concrete public border values and the existing no-paint behavior.
Omitted width/style defaults, logical sides, broader color semantics, and
browser-wide border conformance remain outside the boundary. Focused,
full-native/library, strict Clippy, rustdoc, two-crate, formatting, static
documentation, and bounded cleanup gates are recorded in the task; remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-163` slice is recorded in
`docs/plan/tasks/native-engine-163.md`; design is `764e0c86` and implementation
is `7a406855`. It extends the same private deferred-color boundary to
`background-color: currentColor`, resolving against the element's local or
inherited `color` only at computed-style construction while preserving the
public `Option<NativeColor>` fill surface and all existing
layout/display-list/capture/raster/hit/semantic owners. `color: currentColor`,
gradients, images, system colors, color spaces, percentages, CSS-wide reset
machinery, and browser-wide color conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, package,
formatting, static documentation, workspace all-target/all-feature, and bounded
cleanup gates passed locally; exact evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-164` slice is recorded in
`docs/plan/tasks/native-engine-164.md`; design is `e11821c5`, implementation is
`cceb61bf`, the diagnostics follow-up is `005083c3`, and synchronized product
documentation is `2960ecc5`. It extends the private deferred-color boundary to
local `text-decoration-color: currentColor`, resolving against the element's
local or inherited `color` while preserving the public `Option<NativeColor>`,
separate glyph/decoration paint owners, and all existing text artifact
schemas. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
package, formatting, static documentation, workspace all-target/all-feature,
and bounded cleanup gates passed locally; exact evidence is recorded in the
task. Gradients, images, system colors, color spaces, percentages, animations,
multiple origins, `color: currentColor`, and browser-wide text-color
conformance remain outside the completed boundary. Remote CI remains pending
because the checkout is local-only.
The completed dependency-ordered `native-engine-165` slice is recorded in
`docs/plan/tasks/native-engine-165.md`; design is `75544c39`, implementation is
`f7b5fd4e`, and synchronized product documentation is `01438316`. It extends
the private color owner to local `color: currentColor`, resolving the
self-reference from the already-computed inherited color or bounded initial
black fallback while preserving the optional public color value and all
existing paint consumers. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence and cleanup are recorded in the task. Gradients, images, system
colors, color spaces, percentages, custom-property graphs, CSS-wide reset
machinery beyond the existing `revert-layer`, multiple origins, animation,
and browser-wide color conformance remain outside the completed boundary.
Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-166` slice is recorded in
`docs/plan/tasks/native-engine-166.md`; design is `d148f766`, implementation is
`b57ba2b8`, and synchronized current product documentation is `d26a9059`. It
extends the private inherited color owner with exact case-insensitive
`inherit`, `unset`, `initial`, and one-author-origin `revert`, resolving the
inherited forms through the bounded parent-color/black-root fallback and
resetting `initial` to black while preserving the optional public color value
and all existing paint consumers. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. Multiple origins, `!important` inversion,
gradients, images, system colors, color spaces, percentages, custom-property
graphs, animation, and browser-wide color conformance remain outside the
completed boundary. Remote CI remains pending because the checkout is
local-only.
The completed dependency-ordered `native-engine-167` slice is recorded in
`docs/plan/tasks/native-engine-167.md`; design is
`9e143200`, implementation is `1ae1f103`, and synchronized current product
documentation is `debd6ab4`. It extends the private non-inherited background
owner with exact case-insensitive `inherit`, `unset`, `initial`, and
one-author-origin `revert`, carrying only the parent's optional concrete fill
for explicit `inherit` and preserving no-fill `None` for reset forms and
ordinary omission. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. Multiple origins, `!important` inversion,
gradients, images, system colors, color spaces, percentages, custom-property
graphs, animation, and browser-wide background conformance remain outside the
completed boundary. Remote CI remains pending because the checkout is
local-only.
The completed dependency-ordered `native-engine-168` slice is recorded in
`docs/plan/tasks/native-engine-168.md`; design is `0bb67e8b`, implementation is
`4521f151`, and synchronized current product documentation is `2184d98`. It
extends local `text-decoration-color` with exact case-insensitive `inherit`,
`unset`, `initial`, and one-author-origin `revert`, copying only the parent's
effective concrete decoration color for explicit `inherit` and resolving reset
forms to the current element color. Omission remains the public `None` and
glyph-color fallback; `currentColor`, `revert-layer`, separate glyph/line
paint, and all text artifact owners remain intact. Focused/full-native/library,
strict Clippy, rustdoc, two-crate, package, formatting, static documentation,
workspace all-target/all-feature, and bounded cleanup gates passed locally;
exact evidence is recorded in the task. Multiple origins, `!important`
inversion, gradients, images, system colors, color spaces, percentages,
custom-property graphs, animation, and browser-wide text-decoration
conformance remain outside the completed boundary. Remote CI remains pending
because the checkout is local-only.
The completed dependency-ordered `native-engine-169` slice is recorded in
`docs/plan/tasks/native-engine-169.md`; design is `b88177dc`, implementation is
`4d9e6979`, test-fixture corrections are `a0e77c84` and `801f7b19`, and
synchronized current product documentation is `07ba3dc4`. It extends the
physical `border-color` shorthand and four color longhands with exact
case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert`.
Only explicit `inherit` copies the parent's effective top/right/bottom/left
colors; reset forms resolve to the current element color and ordinary omission
retains the black side fallback. Existing `currentColor`, `revert-layer`,
width/style composition, border geometry, and all artifact owners remain
unchanged. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
package, formatting, static documentation, workspace all-target/all-feature,
and bounded cleanup gates passed locally. Multiple origins, `!important`
inversion, logical sides, table conflict resolution, gradients, images,
animation, and browser-wide border conformance remain outside the completed
boundary. Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-171` slice is recorded in
`docs/plan/tasks/native-engine-171.md`; implementation is `67e04c0d`. It extends
the physical `border-style` shorthand and four style longhands with exact
case-insensitive `inherit`, `unset`, `initial`, and one-author-origin `revert`.
Only explicit `inherit` copies the parent's effective top/right/bottom/left
styles, including private `none`/`hidden` and styles from unpainted or
zero-width parents; reset forms resolve to private `none` and ordinary
omission retains the no-style fallback. Existing `revert-layer`, width/color
composition, border geometry, and all display, capture, raster, point-hit,
semantic, and public/artifact owners remain unchanged. Focused/full-native/
library, strict Clippy, rustdoc, two-crate, package, formatting, static
documentation, workspace all-target/all-feature, and bounded cleanup gates
passed locally. Mixed CSS-wide/style shorthand, logical sides, table conflict
resolution, multiple origins, and browser-wide border conformance remain
outside the completed boundary. Remote CI remains pending because the checkout
is local-only.
The completed dependency-ordered `native-engine-172` slice is recorded in
`docs/plan/tasks/native-engine-172.md`; implementation is `b0bbe45a`. It extends
the physical `border-radius` shorthand with exact case-insensitive `inherit`,
`unset`, `initial`, and one-author-origin `revert`, copying only explicit
effective parent radii while reset and ordinary omission retain the default
zero-corner fallback. Existing bounded one-to-four-value expansion,
`revert-layer`, rounded geometry, display replay, raster, point-hit, capture,
and semantic/source-order owners remain unchanged. Focused/full-native/library
tests pass locally; remaining certification evidence is recorded in the task.
Mixed CSS-wide/concrete or slash-separated radii, corner longhands, elliptical
and percentage radii, multiple origins, and browser-wide border conformance
remain outside the completed boundary. Remote CI remains pending because the
checkout is local-only.
The completed dependency-ordered `native-engine-173` slice is recorded in
`docs/plan/tasks/native-engine-173.md`; implementation is `f5f53cec`. It adds
exact case-insensitive `inherit`, `unset`, `initial`, and one-author-origin
`revert` to the complete physical `border`, `border-top`, `border-right`,
`border-bottom`, and `border-left` shorthands by projecting private values into
the existing width/style/color candidate streams. Explicit `inherit` copies
effective parent side values, while reset forms project zero-width, private
`none`, and `currentColor` so later component declarations can compose;
ordinary omission remains omission and mixed CSS-wide/concrete forms remain
unsupported. Existing concrete/omitted-component forms, `revert-layer`,
box-model geometry, display replay, raster, point-hit, capture, and semantic
owners remain unchanged. Focused/full-native/library, strict Clippy,
warning-denied rustdoc, paired-crate check/build, packaging, static
documentation, and workspace all-target/all-feature gates pass locally; exact
evidence is recorded in the task. Logical sides, table conflict resolution,
multiple origins, `!important` inversion, and browser-wide border conformance
remain outside the completed boundary. Remote CI remains pending because the
checkout is local-only.
The completed dependency-ordered `native-engine-174` slice is recorded in
`docs/plan/tasks/native-engine-174.md`; implementation is `f6953813`, with the
strict-cascade cleanup at `23b09864`. It adds bounded horizontal-tb logical
`border-block`, `border-block-start`, `border-block-end`, `border-inline`,
`border-inline-start`, and `border-inline-end` shorthands plus their
width/style/color component families. Block start/end map to physical
top/bottom; inline start/end map through the resolved inherited `direction` to
left/right for `ltr` and right/left for `rtl`. Logical declarations project
into the existing physical candidate streams, preserving component
composition, layer/source-order precedence, `currentColor`, CSS-wide values,
and `revert-layer` while reusing box-model, display-list, capture, raster,
point-hit, and semantic/source-order owners. Vertical writing modes, logical
radius, border-image, gradients, table conflict resolution, multiple origins,
and browser-wide logical-border conformance remain outside the completed
boundary. Focused/full-native/library, strict Clippy, warning-denied rustdoc,
paired-crate check/build, and formatting gates pass locally; package, static,
workspace, and cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The completed dependency-ordered `native-engine-175` slice is recorded in
`docs/plan/tasks/native-engine-175.md`; implementation is `2b082ddf`, with the
resolver/test-shape correction at `e6f3259d`. It adds the four physical
`border-radius` corner longhands through private per-corner candidate streams,
preserving shorthand/longhand source order, bounded CSS-wide values,
`revert-layer`, and the existing rounded layout/display/capture/raster/
point-hit/semantic owners. Focused/full-native/library, strict Clippy,
warning-denied rustdoc, paired-crate check/build, packaging, static
documentation, workspace all-target/all-feature, security/fuzz, and formatting
gates pass locally; exact evidence and bounded cleanup are recorded in the
task. Logical corner names, writing-mode-dependent mapping, percentages,
elliptical radii, and browser corner fidelity remain outside the completed
boundary. Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-176` slice is recorded in
`docs/plan/tasks/native-engine-176.md`; implementation is `6543b2b6`. It adds
the four bounded logical corner longhands through horizontal-tb ltr/rtl
direction-aware projection into the completed physical per-corner candidate
streams, preserving source order, CSS-wide values, `revert-layer`, and all
existing rounded consumers. Focused/full-native/library, strict Clippy,
warning-denied rustdoc, paired-crate check/build, packaging, static
documentation, workspace all-target/all-feature, security/fuzz, and formatting
gates pass locally; exact evidence and bounded cleanup are recorded in the
task. Vertical writing modes, text orientation, percentages, elliptical radii,
and browser logical-radius fidelity remain outside the completed boundary.
Remote CI remains pending because the checkout is local-only.
The completed dependency-ordered `native-engine-177` slice is recorded in
`docs/plan/tasks/native-engine-177.md` and implemented in `46f6499a`. It gives
the physical and horizontal-tb logical radius family bounded author-origin
`!important` priority through a private reversed named-layer partition:
important radius declarations beat normal radius declarations, earliest named
important layers win, and unlayered important declarations remain the lowest
important bucket. The radius value grammar and all rounded layout, display,
capture, raster, point-hit, and semantic owners remain unchanged. General
`!important` semantics for other properties, multiple origins, transitions,
animations, vertical writing modes, elliptical/percentage radii, and browser
conformance remain outside this slice. Slice-local certification passed;
issue-level final gates, cleanup, and remote CI remain pending.

The completed dependency-ordered `native-engine-178` slice is recorded in
`docs/plan/tasks/native-engine-178.md` and implemented in `1292538c`. It
extends bounded author-origin `!important` priority to `background-color`,
inherited `color`, and `text-decoration-color` through private reversed
named-layer partitions. Existing value grammar, inheritance/defaulting,
fill/glyph/decoration artifacts, public schemas, and the two-crate boundary
remain unchanged. Border color and other properties do not receive priority
semantics in this slice. Slice-local certification and task-specific cleanup
passed; issue-level final gates, final cleanup, and remote CI remain pending.

The completed dependency-ordered `native-engine-179` slice is recorded in
`docs/plan/tasks/native-engine-179.md` and implemented in `ed1cda27`. It
extends bounded author-origin `!important` priority to the standalone physical
`border-color` shorthand and four physical color longhands through private
reversed named-layer partitions. Existing four-side color resolution,
currentColor/CSS-wide/revert-layer behavior, border width/style composition,
artifacts, public schemas, and the two-crate boundary remain unchanged.
Complete/side border shorthands and logical border-color remain outside this
slice. Slice-local certification passed; final documentation audit and
task-specific cleanup are recorded in the task. Issue-level final gates and
remote CI remain pending.

The completed dependency-ordered `native-engine-180` slice is recorded in
`docs/plan/tasks/native-engine-180.md` and implemented in `491f65fe`, with the
strict-lint follow-up at `100d1888`. It extends bounded author-origin
`!important` priority to the six supported horizontal-tb logical border-color
declarations while preserving their existing `ltr`/`rtl` projection into
physical sides. Complete border shorthands, border width/style, vertical
writing modes, and other properties remain outside this slice. Slice-local
certification passed; final documentation audit and task-specific cleanup are
recorded in the task. Issue-level final gates and remote CI remain pending.
The completed dependency-ordered `native-engine-181` slice is recorded in
`docs/plan/tasks/native-engine-181.md` and implemented in `436dd02a`. It
extends bounded author-origin `!important` priority to the standalone physical
`border-width` shorthand and four physical width longhands while preserving
one-to-four-value expansion, independent per-side resolution, and the existing
width/style/color consumers. Complete/side border shorthands, logical border
width, border style/color, vertical writing modes, and other properties remain
outside this slice. Focused, full-native, feature-library, strict Clippy,
warning-denied rustdoc, current documentation audits, and task-specific
cleanup pass locally; issue-level final gates and remote CI remain pending
until issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-182` slice is recorded in
`docs/plan/tasks/native-engine-182.md` and implemented in `008a5766`. It
extends bounded author-origin `!important` priority to the standalone physical
`border-style` shorthand and four physical style longhands while preserving
one-to-four-value expansion, independent per-side no-paint/paint resolution,
and the existing width/style/color consumers. Complete/side border shorthands,
logical border style, border width/color, vertical writing modes, and other
properties remain outside this slice. Focused, full-native, feature-library,
strict Clippy, warning-denied rustdoc, current documentation audits, and
task-specific cleanup pass locally; issue-level final gates and remote CI
remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-183` slice is recorded in
`docs/plan/tasks/native-engine-183.md` and implemented at `6013d0c5`. It extends
the bounded author-origin `!important` partition to the six supported
horizontal-tb logical border-width declarations, preserving their resolved
`ltr`/`rtl` projection into physical width streams. Complete/side border
shorthands, logical border style, vertical writing modes, and other properties
remain outside this slice. Focused, full-native, feature-library, strict
Clippy, warning-denied rustdoc, and current documentation audits pass locally;
issue-level final gates and remote CI remain pending until issue #40 reaches
its final validation boundary.

The completed dependency-ordered `native-engine-184` slice is recorded in
`docs/plan/tasks/native-engine-184.md` and implemented at `26fd347a`. It
extends the bounded author-origin `!important` partition to the six supported
horizontal-tb logical border-style declarations, preserving their private
`none`/`hidden` behavior and resolved `ltr`/`rtl` projection into physical
style streams. Complete/side border shorthands, logical border width/color,
vertical writing modes, and other properties remain outside this slice.
Focused, full-native, feature-library, strict Clippy, warning-denied rustdoc,
and current documentation audits pass locally; issue-level final gates and
remote CI remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-185` slice is recorded in
`docs/plan/tasks/native-engine-185.md` and implemented at `bfcb6dc9`. It extends
the bounded author-origin `!important` partition to the complete physical
`border` shorthand and the four physical side-border shorthands by carrying
one private importance bit per side into the existing doubled width/style/color
component streams. This preserves independent component composition,
CSS-wide/omitted `none`/`hidden`/`revert-layer` behavior, physical layout and
paint consumers, public schemas, and the two-crate boundary. Logical complete
shorthands, vertical writing modes, and browser-wide border conformance remain
outside this slice. Focused, full-native, feature-library, strict Clippy,
warning-denied rustdoc, current documentation audits, and task-specific
cleanup pass locally; issue-level final gates and remote CI remain pending until
issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-186` slice is recorded in
`docs/plan/tasks/native-engine-186.md` and implemented at `274441e1`, with the
strict-lint helper correction at `758b891a`. It extends the bounded
author-origin `!important` partition to the six supported horizontal-tb logical
complete/side border shorthands by carrying one private importance bit per
logical side into the existing doubled width/style/color component streams
before resolved `ltr`/`rtl` projection. This preserves independent component
composition, CSS-wide/omitted `none`/`hidden`/`revert-layer` behavior, logical
direction mapping, physical layout and paint consumers, public schemas, and
the two-crate boundary. Vertical writing modes and browser-wide logical-border
conformance remain outside this slice. Focused, full-native, feature-library,
strict Clippy, warning-denied rustdoc, current documentation audits, and
task-specific cleanup pass locally; issue-level final gates and remote CI
remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-187` slice is recorded in
`docs/plan/tasks/native-engine-187.md` and implemented at `65883117`. It
extends the bounded author-origin `!important` partition to the supported
text-flow and text-decoration declarations through a private doubled text
partition, preserving important-over-normal ordering, reversed named-layer
priority, inline important precedence, invalid-later preservation,
inherited/local fallbacks, and `revert-layer` rollback. Existing layout,
decoration, raster, capture, hit, diagnostics, schema, dependency, default,
and two-crate owners remain unchanged. Display/visibility/opacity, flex/gap,
dimensions/box model, overflow, multiple origins, transitions, animations,
vertical writing modes, and browser-wide CSS conformance remain outside this
bounded slice. Focused, full-native, feature-library, strict Clippy,
warning-denied rustdoc, current documentation audits, and task-specific
cleanup pass locally; issue-level final gates and remote CI remain pending
until issue #40 reaches its final validation boundary.

The completed dependency-ordered `native-engine-188` slice is recorded in
`docs/plan/tasks/native-engine-188.md` and implemented at `ca0b47bd`. It extends
the bounded author-origin `!important` partition to local `display`,
`visibility`, and `opacity` through a private doubled local partition,
preserving important-over-normal ordering, reversed named-layer priority,
inline important precedence, invalid-later preservation, and `revert-layer`
rollback through the existing hidden-subtree, semantic, layout, display-list,
raster, capture, and point-hit owners. Flex/gap, dimensions/box model,
overflow, other properties, dependencies, defaults, and the two-crate boundary
remain unchanged. Scoped check, focused units/integration, full native
integration (226/226), strict Clippy, warning-denied rustdoc, and formatting
gates pass locally; final issue-level gates and remote CI remain pending.

The completed dependency-ordered `native-engine-189` slice is recorded in
`docs/plan/tasks/native-engine-189.md` and implemented at `94724ab0`. It extends
the bounded author-origin `!important` partition to the normal-only flex and
gap declarations through private doubled flex candidate arrays and important-
aware gap partitions. `place-content`, `flex-flow`, and `flex` carry their
priority through bounded shorthand expansion; important-over-normal ordering,
reversed named-layer priority, inline precedence in the unlayered important
bucket, invalid-later preservation, independent gap-axis source order, and
`revert-layer` rollback are preserved through the existing row/column/wrap,
overflow, layout, display-list, raster, capture, and point-hit consumers.
Dimensions/box model, overflow priority, other properties, dependencies,
defaults, multiple origins, and browser-wide CSS conformance remain outside
this slice. Scoped check, focused units/integration, full native integration
(227/227), strict Clippy, warning-denied rustdoc, and formatting pass locally;
final static, paired-crate, package, workspace, security/fuzz, cleanup,
issue-level, and remote-CI gates remain pending.

The completed dependency-ordered `native-engine-190` slice is recorded in
`docs/plan/tasks/native-engine-190.md` and implemented at `d5cc6e6b`. It extends
the bounded author-origin `!important` partition to the six normal-only local
dimension declarations through private doubled candidate arrays and per-
property importance bits. Important-over-normal ordering, reversed named-layer
priority, inline precedence in the unlayered important bucket, invalid-later
preservation, independent dimension streams, and `revert-layer` rollback flow
through the existing min/max, box geometry, clipping, hit-test, display-list,
raster, and PNG capture owners. Box-model declarations, overflow priority,
other properties, dependencies, defaults, multiple origins, and browser-wide
CSS sizing conformance remain outside this slice. Scoped check, focused units/
integration, full native integration (228/228), strict Clippy, warning-denied
rustdoc, and formatting pass locally; final static, paired-crate, package,
workspace, security/fuzz, cleanup, issue-level, and remote-CI gates remain
pending.

The completed dependency-ordered `native-engine-191` slice is recorded in
`docs/plan/tasks/native-engine-191.md` and implemented at `43e5f8c2`. It extends
the bounded author-origin `!important` partition to the normal-only physical
box-model declarations: `box-sizing`, four physical padding edges, and four
physical margin edges plus their shorthand forms. Important-over-normal
ordering, reversed named-layer priority, inline precedence in the unlayered
important bucket, per-edge shorthand/longhand source order, invalid-later
preservation, `auto` margin provenance, and `revert-layer !important` rollback
flow through the existing content-box/border-box, normal-flow/flex, overflow,
layout, display-list, raster, capture, point-hit, and semantic/source-order
owners. Logical edges, overflow priority, and general CSS conformance remain
outside this slice. Focused units, the dedicated integration fixture, full
native integration (229/229), strict Clippy, warning-denied rustdoc, and
formatting pass locally; final static, paired-crate, package, workspace,
security/fuzz, cleanup, issue-level, and remote-CI gates remain pending.

The completed dependency-ordered `native-engine-192` slice is recorded in
`docs/plan/tasks/native-engine-192.md` and implemented at `fc2c461e` (design
`7e693b79`). It extends the bounded author-origin `!important` partition to
the normal-only `overflow`, `overflow-x`, and `overflow-y` declarations through
private doubled x/y candidate streams and shorthand/x/y importance bits.
Important-over-normal ordering, reversed named-layer priority, inline
precedence in the unlayered important bucket, invalid-later preservation,
independent axis projection, and `revert-layer !important` rollback flow through
the existing clip, root-overflow, layout, display-list, raster, capture,
point-hit, and semantic/source-order owners. Nested scrolling, visible/auto/
scroll used-value parity, logical writing modes, multiple origins, and
browser-wide CSS overflow conformance remain outside this slice. Scoped check,
focused units/integration, full native integration (230/230), strict Clippy,
warning-denied rustdoc, and formatting pass locally; final static, paired-crate,
package, workspace, security/fuzz, cleanup, issue-level, and remote-CI gates
remain pending.

The completed dependency-ordered `native-engine-193` slice is recorded in
`docs/plan/tasks/native-engine-193.md` and implemented at `3e78c246` (design
`03bcf403`). It adds bounded horizontal-tb logical `padding-block`,
`padding-inline`, `margin-block`, and `margin-inline` shorthands plus their
block/inline start/end longhands. Resolved `ltr`/`rtl` direction projects the
logical edges into the existing physical per-edge candidate streams, preserving
important-over-normal ordering, reversed named-layer priority, inline-important
precedence, invalid-later behavior, same-rule physical/logical source order,
`auto` margin provenance, and `revert-layer` rollback through the existing
geometry, normal-flow/flex, overflow, display-list, raster, capture, point-hit,
and semantic/source-order owners. Logical `box-sizing`, vertical writing modes,
percentages, negative lengths, margin collapsing, positioning, CSS-wide reset
keywords, and browser-wide conformance remain outside this slice. Scoped check,
focused parser/integration tests, full native integration (231/231), strict
Clippy, warning-denied rustdoc, and formatting pass locally; final static,
paired-crate, package, workspace, security/fuzz, cleanup, issue-level, and
remote-CI gates remain pending.

The completed dependency-ordered `native-engine-194` slice is recorded in
`docs/plan/tasks/native-engine-194.md` and implemented at `5477fb79` (design
`e643a64c`). It adds standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert` to `box-sizing`, physical padding/margin
shorthands and longhands, and the horizontal-tb logical padding/margin family.
These forms normalize to the existing content-box and zero-edge fallbacks,
while `revert-layer` remains a separate rollback candidate and terminal
`!important` behavior remains unchanged. Existing logical direction projection,
source order, geometry, normal-flow/flex, overflow, display-list, raster,
capture, point-hit, and semantic/source-order owners remain unchanged. Explicit
`inherit`, percentages, negative lengths, margin collapsing, positioning,
vertical writing modes, multiple origins, and browser-wide CSS-wide conformance
remain outside this slice. Scoped check, focused parser/integration tests, full
native integration (232/232), strict Clippy, warning-denied rustdoc, and
formatting pass locally; final static, paired-crate, package, workspace,
security/fuzz, cleanup, issue-level, and remote-CI gates remain pending.

The completed dependency-ordered `native-engine-195` slice is recorded in
`docs/plan/tasks/native-engine-195.md` and implemented at `0caad64b` (design
`a61b9c5f`). It adds standalone case-insensitive `inherit` to
`box-sizing`, physical padding/margin shorthands and longhands, and the
supported horizontal-tb logical padding/margin family. Physical inheritance
copies the parent's effective values, including private margin `auto`
provenance; logical inheritance reads the parent side in the parent's resolved
`ltr`/ `rtl` direction before projecting into the child's direction. Root
fallbacks, explicit non-inheritance, important/source-order and
`revert-layer` behavior remain bounded by the existing private cascade.
Percentages, negative lengths, margin collapsing, positioning, vertical writing
modes, additional logical properties, multiple origins, and browser-wide CSS
conformance remain outside this slice. Scoped check, focused parser/integration
tests, full native integration (233/233), strict Clippy, warning-denied
rustdoc, and formatting pass locally; final static, paired-crate, package,
workspace, security/fuzz, cleanup, issue-level, and remote-CI gates remain
pending.

The completed dependency-ordered `native-engine-196` slice is recorded in
`docs/plan/tasks/native-engine-196.md` and implemented at `fd6ee415` (design
`9231d17f`). It adds standalone case-insensitive `inherit` to the six local
dimension declarations, copying the parent's computed optional pixel value
through the existing private DOM style walk while keeping omitted dimensions
local and preserving explicit parent `None`/auto fallbacks. Important/source-
order behavior, invalid-later preservation, `revert-layer`, min/max,
content-box/border-box, normal-flow/flex, display-list, raster, PNG capture,
point-hit, and semantic/source-order owners remain bounded. Percentages,
negative lengths, intrinsic sizing, aspect ratio, margin collapsing, positioning,
vertical writing modes, additional origins, transitions, animations, and
browser-wide sizing conformance remain outside this slice. Scoped check,
focused unit/integration, full native integration (234/234), strict Clippy,
warning-denied rustdoc, formatting, static documentation truth/coverage/depth,
feature parity, TUI shortcut, and version-sync pass locally; paired-crate,
package, workspace, security/fuzz, cleanup, issue-level, and remote-CI gates
remain pending.

The completed dependency-ordered `native-engine-197` slice is recorded in
`docs/plan/tasks/native-engine-197.md` and implemented at `a0e102b5` (design
`7d71a50c`). It adds standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert` to the six local dimension declarations. Winning
reset candidates resolve to the existing optional `None`/auto fallback without
falling through; `revert-layer` remains the separate lower-layer rollback
candidate. Omission, explicit `inherit`, important/source-order behavior,
invalid-later preservation, min/max, content-box/border-box, normal-flow/flex,
display-list, raster, PNG capture, point-hit, and semantic/source-order owners
remain bounded. Percentages, negative lengths, intrinsic sizing, aspect ratio,
margin collapsing, positioning, vertical writing modes, additional origins,
transitions, animations, and browser-wide sizing conformance remain outside
this slice. Scoped check, focused unit/integration, full native integration
(235/235), strict Clippy, warning-denied rustdoc, formatting, and static
documentation gates pass locally: release truth reports 611 Markdown documents
(83 current, 57 previous-version hits, 714 semantic audit hits, 0 current-claim
failures); coverage reports 611 Markdown files, 345 full-product MCP tools (100
browser-only), 17 examples, and 22 public modules; depth reports 93 current
guides and 19 substantive contracts; parity reports 14 capabilities across 4
targets; TUI reports 15 implementation help keys and 63 documentation markers;
version sync reports 0.3.14. Paired-crate, package, workspace, security/fuzz,
cleanup, issue-level, and remote-CI gates remain pending.

The completed dependency-ordered `native-engine-198` slice is recorded in
`docs/plan/tasks/native-engine-198.md` and implemented at `95a988d0` (design
`927cccca`). It adds standalone case-insensitive `inherit`, `initial`, `unset`,
and one-author-origin `revert` to inherited `white-space`, positive-pixel
`line-height`, `text-transform`, `font-weight`, `font-style`, `word-break`,
`vertical-align`, `word-spacing`, and `letter-spacing`. Parent/root fallback,
terminal reset behavior, invalid-later preservation, existing cascade priority,
and `revert-layer` distinction reuse the existing fixed-cell computed-style,
layout, display-list, raster, PNG, point-hit, semantic, and diagnostic owners;
the public schemas and two-crate boundary remain unchanged. Focused and full
native integration (236/236), feature-library tests with an explicit 8 MiB
test-thread stack (1019 passed, 1 ignored), strict Clippy, warning-denied
rustdoc, locked scoped check, and formatting pass locally. Static release truth
passes with 612 Markdown documents (83 current, 57 previous-version hits, 715
semantic audit hits, 0 current-claim failures); coverage, depth, parity, TUI,
and version-sync also pass with 612 Markdown files, 345 full-product MCP tools
(100 browser-only), 17 examples, 22 public modules, 93 current guides, 19
substantive contracts, 14 capabilities across 4 targets, 15 implementation
help keys, 63 documentation markers, and version `0.3.14`. The default-stack
library run reproduces the pre-existing large-Clap parser-test overflow.
Paired-crate, package, workspace, security/fuzz, cleanup, issue-level, and
remote-CI gates remain pending.

The completed dependency-ordered `native-engine-199` slice is recorded in
`docs/plan/tasks/native-engine-199.md` and implemented at `4771f352` (design
`4b02b41b`). It extends the inherited text-alignment and direction owners with
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert`, while retaining `revert-layer` as the named-layer rollback. Parent
and root fallbacks, terminal reset behavior, invalid-later preservation, and
the existing ltr/rtl logical projection remain bounded; the computed values
continue through the existing layout, display-list, raster, PNG, point-hit,
semantic, and diagnostic owners without public-schema or crate-boundary
changes. Focused cascade/parser/integration coverage and full native
integration (237/237) pass locally. The feature library, paired binaries,
strict Clippy, warning-denied rustdoc, workspace all-target/all-feature tests,
doctests, fuzz, package/install, security, static documentation, and
formatting gates also pass locally. The direct registry-backed dev package
verification remains blocked by the immutable public `glass-browser 0.3.14`
API surface; the canonical local patched/no-verify route and clean-install
transition gate pass. Exact temporary-target cleanup and issue synchronization
remain; remote CI is not claimed for this local-only checkout.

The completed dependency-ordered `native-engine-200` slice is recorded in
`docs/plan/tasks/native-engine-200.md` and implemented at `62525ec4` (design
`9eafebc9`). It extends the inherited `text-decoration-style` owner with
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert`, retaining `revert-layer` as the named-layer rollback. Parent/root
fallbacks, `solid` initial behavior, terminal reset semantics, invalid-later
preservation, and the existing finite decoration styles remain bounded; the
resolved value continues through the existing text display-list, fixed-cell
raster, PNG, and diagnostic owners without public-schema or crate-boundary
changes. Full native integration (238/238), the feature library (1,020 passed,
1 ignored), paired binaries, strict Clippy, warning-denied rustdoc, workspace
all-target/all-feature tests and doctests, fuzz, package, security, and
formatting gates pass locally. The direct
registry-backed dev package verification remains blocked by the immutable
public `glass-browser 0.3.14` API surface; the documented local patched/
no-verify route passes. Remote CI, push, release, tag, and registry
publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-201` slice is recorded in
`docs/plan/tasks/native-engine-201.md` and implemented at `ee7dae83` (design
`d9a763db`). It extends the inherited `text-decoration-thickness` owner with
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert`, retaining `revert-layer` as the named-layer rollback. Parent/root
fallbacks, `1px` initial behavior, terminal reset semantics, invalid-later
preservation, and the existing finite `1px|2px|3px|4px` geometry remain
bounded; resolved thickness continues through the existing text command,
display-list, capture, and fixed-cell raster owners without public-schema or
crate-boundary changes. Full native integration (239/239), the feature library
(1,021 passed, 1 ignored), paired binaries, strict Clippy, warning-denied
rustdoc, workspace tests/doctests, fuzz, package, security, and formatting
gates pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-202` slice is recorded in
`docs/plan/tasks/native-engine-202.md` and implemented at `65e3a76d` (design
`d435032c`). It extends the inherited `text-decoration-skip-ink` owner with
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert`, retaining `revert-layer` as the named-layer rollback. Parent/root
fallbacks, finite `auto|none` behavior, terminal reset semantics,
invalid-later preservation, and the same-run glyph intersection raster remain
bounded; the resolved value continues through the existing text command,
display-list, capture, diagnostic, and fixed-cell raster owners without
public-schema or crate-boundary changes. Full native integration (240/240),
the feature library (1,022 passed, 1 ignored), workspace all-target/all-feature
tests (1,023 passed, 1 ignored in the browser library; 240 native integration;
365 dev tests; 4 development-runtime tests; 15 PTY tests), doctests (4 browser,
1 dev), paired binaries, strict Clippy, warning-denied rustdoc, fuzz, package,
security, and formatting gates pass locally. Remote CI, push, release, tag, and
registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-203` slice is recorded in
`docs/plan/tasks/native-engine-203.md` and implemented at `73c00616` (design
`deeedd19`). It extends the inherited three-bit `text-decoration-line` owner,
including the existing bounded `text-decoration` shorthand route, with
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert`, retaining finite line combinations and `revert-layer` rollback.
Parent/root fallback, `none` initial behavior, terminal reset semantics,
invalid-later preservation, shorthand/longhand source order, display-list,
fixed-cell raster, PNG capture, diagnostics, and the two-crate boundary remain
bounded; full shorthand expansion and generic CSS-wide machinery remain
outside the contract. Full native integration (241/241) and the feature
library (1,023 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-204` slice is recorded in
`docs/plan/tasks/native-engine-204.md` and implemented at `c82773e2` (design
`9473832f`). It extends the inherited signed `text-underline-offset` owner
with standalone case-insensitive `inherit`, `initial`, `unset`, and
one-author-origin `revert`, retaining finite `-4px..=4px` values and
`revert-layer` rollback. Parent/root fallback, zero-pixel initial behavior,
terminal-reset semantics, invalid-later preservation, important/source order,
underline-only movement, overline/line-through preservation, display-list,
fixed-cell raster, PNG capture, diagnostics, and the two-crate boundary remain
bounded; auto, percentages, font-derived values, and generic CSS-wide
machinery remain outside the contract. Full native integration (242/242) and
the feature library (1,024 passed, 1 ignored) pass locally. Remote CI, push,
release, tag, and registry publication remain unclaimed for this local-only
checkout.

The completed dependency-ordered `native-engine-205` slice is recorded in
`docs/plan/tasks/native-engine-205.md` and implemented at `46128c2e` (design
`8852adee`). It extends the local `gap`, `row-gap`, and `column-gap` owners
with standalone case-insensitive `initial`, `unset`, and one-author-origin
`revert`, retaining finite non-negative pixel values and `revert-layer`
rollback. Zero-gap reset fallback, shorthand/longhand axis projection,
important/source order, invalid-later preservation, flex placement,
point-hit, fixed-cell raster, PNG capture, diagnostics, and the two-crate
boundary remain bounded; parent-gap propagation, `inherit`, percentages,
fractional/intrinsic values, and generic CSS-wide machinery remain outside the
contract. Full native integration (243/243) and the feature library (1,025
passed, 1 ignored) pass locally. Remote CI, push, release, tag, and registry
publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-206` slice is recorded in
`docs/plan/tasks/native-engine-206.md` and implemented at `b7bd9ace` (design
`82c31c9a`). It extends the local `text-indent` and `text-overflow` owners with
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`,
retaining finite non-negative fixed-pixel indentation, `clip|ellipsis`, and
`revert-layer` rollback. `text-indent` reset forms use the existing `0px`
fallback and `text-overflow` reset forms use `clip`; invalid-later preservation,
important/source order, first-line layout, eligible truncation, point-hit,
display-list, fixed-cell raster, PNG capture, diagnostics, and the two-crate
boundary remain bounded. `inherit`, parent propagation, negative/fractional or
percentage indentation, and browser-wide overflow conformance remain outside
the contract. Full native integration (244/244) and the feature library (1,026
passed, 1 ignored) pass locally. Remote CI, push, release, tag, and registry
publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-207` slice is recorded in
`docs/plan/tasks/native-engine-207.md` and implemented at `1a43c150` (design
`6d98fb3f`). It extends the local `flex`, `flex-grow`, `flex-shrink`, and
`flex-basis` owners with standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert`, retaining finite shorthand expansion,
non-negative fixed-pixel basis values, `auto`, and `revert-layer` rollback.
Reset forms resolve through the existing private component streams to the
finite `0 1 auto` initial tuple; important/source order, invalid-later
preservation, flex placement, display-list, fixed-cell raster, PNG capture,
point-hit, diagnostics, and the two-crate boundary remain bounded. `inherit`,
percentages, negative/fractional/intrinsic basis values, direction/wrap/
alignment/order, and generic CSS-wide machinery remain outside the contract.
Full native integration (245/245) and the feature library (1,027 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-208` slice is recorded in
`docs/plan/tasks/native-engine-208.md` and implemented at `fcadc99c` (design
`e91a358b`). It extends the local `flex-direction`, `flex-wrap`, and
`flex-flow` owners with standalone case-insensitive `initial`, `unset`, and
one-author-origin `revert`, retaining finite row/column direction values,
finite wrap modes, shorthand component projection, and `revert-layer` rollback.
Reset forms resolve through the existing private component streams to the
finite `row`/`nowrap` initial defaults; important/source order, invalid-later
preservation, row/column mapping, wrapping, display-list, fixed-cell raster,
PNG capture, point-hit, diagnostics, and the two-crate boundary remain bounded.
Full native integration (246/246) and the feature library (1,029 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-209` slice is recorded in
`docs/plan/tasks/native-engine-209.md` and implemented at `0bfbc8f9` (design
`89373bb1`). It extends the local flex-item `order` owner with standalone,
case-insensitive `initial`, `unset`, and one-author-origin `revert`, retaining
the signed finite range and named-layer `revert-layer`. Reset forms resolve
through the existing local order resolver to the finite `0` initial fallback;
important/source order, invalid-later preservation, stable visual/source order,
flex sizing, display-list, fixed-cell raster, PNG capture, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(247/247) and the feature library (1,030 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed for
this local-only checkout.

The completed dependency-ordered `native-engine-210` slice is recorded in
`docs/plan/tasks/native-engine-210.md` and implemented at `e9ac0b1c` (design
`e44ce4ec`). It extends the local `justify-content` owner with standalone,
case-insensitive `initial`, `unset`, and one-author-origin `revert`, retaining
finite distribution values and named-layer `revert-layer`. Reset forms reuse
the existing bounded `flex-start` fallback; important/source order,
invalid-later preservation, free-space distribution, row/column mapping, flex
sizing, display-list, fixed-cell raster, PNG capture, point-hit, diagnostics,
and the two-crate boundary remain bounded. Full native integration (248/248)
and the feature library (1,031 passed, 1 ignored) pass locally. Remote CI,
push, release, tag, and registry publication remain unclaimed for this
local-only checkout.

The completed dependency-ordered `native-engine-211` slice is recorded in
`docs/plan/tasks/native-engine-211.md` and implemented at `29e23b4a` (design
`c5c36b40`). It extends the local `align-items` owner with standalone,
case-insensitive `initial`, `unset`, and one-author-origin `revert`, retaining
finite cross-axis values and named-layer `revert-layer`. Reset forms reuse the
existing bounded `flex-start` fallback; important/source order, invalid-later
preservation, cross-axis placement, `align-self` overrides, flex sizing,
display-list, fixed-cell raster, PNG capture, point-hit, diagnostics, and the
two-crate boundary remain bounded. Full native integration (249/249) and the
feature library (1,032 passed, 1 ignored) pass locally. Remote CI, push,
release, tag, and registry publication remain unclaimed for this local-only
checkout.

The completed dependency-ordered `native-engine-212` slice is recorded in
`docs/plan/tasks/native-engine-212.md` and implemented at `0a624642` (design
`68f42ebe`). It extends the local `align-self` owner with standalone,
case-insensitive `initial`, `unset`, and one-author-origin `revert`, retaining
finite item values and named-layer `revert-layer`. Reset forms reuse the
existing bounded `auto` fallback and continue through parent `align-items`;
important/source order, invalid-later preservation, complete-subtree movement,
flex sizing, display-list, fixed-cell raster, PNG capture, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(250/250) and the feature library (1,033 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed for
this local-only checkout.

The completed dependency-ordered `native-engine-213` slice is recorded in
`docs/plan/tasks/native-engine-213.md` and implemented at `337da7c5` (design
`e82eea50`). It extends the local `align-content` owner with standalone,
case-insensitive `initial`, `unset`, and one-author-origin `revert`, retaining
finite wrapped-line distribution values and named-layer `revert-layer` rollback.
Reset forms reuse the existing bounded `flex-start` fallback; terminal reset,
invalid-later preservation, important/source order, wrapped-line distribution,
item alignment, flex sizing, display-list, fixed-cell raster, PNG capture,
point-hit, diagnostics, and the two-crate boundary remain bounded. Full native
integration (251/251) and the feature library (1,034 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-214` slice is recorded in
`docs/plan/tasks/native-engine-214.md` and implemented at `24ae15a7` (design
`9f26239d`). It extends the local `place-content` shorthand projection with
standalone, case-insensitive `initial`, `unset`, and one-author-origin `revert`,
retaining finite one-/two-value expansion and named-layer `revert-layer`
rollback. Reset forms project through the existing bounded `flex-start`
fallbacks for both `align-content` and `justify-content`; terminal reset,
invalid-later preservation, important/source order, wrapped-line distribution,
main-axis placement, item alignment, flex sizing, display-list, fixed-cell
raster, PNG capture, point-hit, diagnostics, and the two-crate boundary remain
bounded. Full native integration (252/252) and the feature library (1,035
passed, 1 ignored) pass locally. Remote CI, push, release, tag, and registry
publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-215` slice is recorded in
`docs/plan/tasks/native-engine-215.md` and implemented at `a66f474b` (design
`0cd2040d`). It adds standalone, case-insensitive `align-content: inherit`
through the existing private ancestor-style chain while keeping omitted
`align-content` non-inherited with the bounded `flex-start` fallback. Mixed
forms, source order, wrapped-line placement, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (253/253) and the feature library (1,036 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry
publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-216` slice is recorded in
`docs/plan/tasks/native-engine-216.md` and implemented at `c3dc1faf` (design
`485c5750`). It adds standalone, case-insensitive `justify-content: inherit`
through the existing private ancestor-style chain while keeping omitted
`justify-content` non-inherited with the bounded `flex-start` fallback. Mixed
forms, source order, main-axis placement, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Full native
integration (254/254) and the feature library (1,037 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-217` slice is recorded in
`docs/plan/tasks/native-engine-217.md` and implemented at `feb6b607` (design
`5ee75fcb`). It adds standalone, case-insensitive `align-items: inherit`
through the existing private ancestor-style chain while keeping omitted
`align-items` non-inherited with the bounded `flex-start` fallback. Mixed
forms, source order, cross-axis placement, `align-self` overrides,
display-list, raster/PNG, point-hit, semantics, diagnostics, and the two-crate
boundary remain bounded. Full native integration (255/255) and the feature
library (1,038 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-218` slice is recorded in
`docs/plan/tasks/native-engine-218.md` and implemented at `52adc7d1` (design
`ca9afd5f`). It adds standalone, case-insensitive `align-self: inherit`
through the existing private ancestor-style chain while keeping omitted
`align-self` local `auto`; explicit `auto` continues to delegate to the
containing flex parent's `align-items`. Mixed forms, source order, cross-axis
placement, complete-subtree movement, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Full native
integration (256/256) and the feature library (1,039 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-219` slice is recorded in
`docs/plan/tasks/native-engine-219.md` and implemented at `23703765` (design
`0b9e784b`). It adds standalone, case-insensitive `place-content: inherit`
through the existing private `align-content` and `justify-content` ancestor-
style owners, copying both computed parent components only when explicitly
authored while keeping omitted `place-content` local. Mixed forms, source
order, important priority, finite shorthand expansion, `revert-layer`,
wrapped-line distribution, main-axis placement, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (257/257) and the feature library (1,040 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-220` slice is recorded in
`docs/plan/tasks/native-engine-220.md` and implemented at `1f64f232` (design
`8ffe1f17`). It adds standalone, case-insensitive `flex-direction: inherit`
through the existing private parent-style chain, copying the computed parent
direction while keeping omitted `flex-direction` local with the bounded `row`
fallback. Mixed forms, source order, important priority, finite
`flex-flow`/longhand component precedence, `revert-layer`, row/column main-axis
mapping, wrapping eligibility, gap and margin mapping, flex sizing,
display-list, raster/PNG, point-hit, semantics, diagnostics, and the two-crate
boundary remain bounded. Full native integration (258/258) and the feature
library (1,041 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-221` slice is recorded in
`docs/plan/tasks/native-engine-221.md` and implemented at `ebeb8443` (design
`2925cc3a`). It adds standalone, case-insensitive `flex-wrap: inherit`
through the existing private parent-style chain, copying the computed parent
wrap mode while keeping omitted `flex-wrap` local with the bounded `nowrap`
fallback. Mixed forms, source order, important priority, finite
`flex-flow`/longhand component precedence, `revert-layer`, row/column wrapping
eligibility, line formation and reverse stacking, line sizing, gap and margin
mapping, flex sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(259/259) and the feature library (1,042 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed for
this local-only checkout.

The completed dependency-ordered `native-engine-222` slice is recorded in
`docs/plan/tasks/native-engine-222.md` and implemented at `87465d23` (design
`cae84efb`). It adds standalone, case-insensitive `flex-flow: inherit` by
projecting to the existing private `flex-direction` and `flex-wrap`
inheritance owners, copying both computed parent components while keeping
omitted `flex-flow` local with the bounded `row`/`nowrap` fallbacks. Mixed
forms, source order, important priority, finite one-/two-value expansion,
longhand/component precedence, `revert-layer`, row/column mapping, wrapping
eligibility, line formation and reverse stacking, gap and margin mapping, flex
sizing, display-list, raster/PNG, point-hit, semantics, diagnostics, and the
two-crate boundary remain bounded. Full native integration (260/260) and the
feature library (1,043 passed, 1 ignored) pass locally. Remote CI, push,
release, tag, and registry publication remain unclaimed for this local-only
checkout.

The completed dependency-ordered `native-engine-223` slice is recorded in
`docs/plan/tasks/native-engine-223.md` and implemented at `c94b6006` (design
`f97ac088`). It adds standalone, case-insensitive `flex: inherit` by
projecting to the existing private `flex-grow`, `flex-shrink`, and `flex-basis`
inheritance owners, copying all three computed parent components while keeping
omitted `flex` local with the bounded `0 1 auto` fallback. Mixed forms, source
order, important priority, finite shorthand expansion, longhand/component
precedence, `revert-layer`, row/column and wrapped sizing, display-list,
raster/PNG, point-hit, semantics, diagnostics, and the two-crate boundary
remain bounded. Full native integration (261/261) and the feature library
(1,044 passed, 1 ignored) pass locally. Remote CI, push, release, tag, and
registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-224` slice is recorded in
`docs/plan/tasks/native-engine-224.md` and implemented at `bc8de568` (design
`4402c331`). It adds standalone, case-insensitive `flex-grow: inherit` through
the existing private grow component and ancestor-style chain, copying the
computed parent grow value while keeping omitted `flex-grow` local with the
bounded `0` fallback. Mixed forms, source order, important priority, finite
shorthand/longhand precedence, reset semantics, `revert-layer`, row/column
and wrapped sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native
integration (262/262) and the feature library (1,044 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-225` slice is recorded in
`docs/plan/tasks/native-engine-225.md` and implemented at `b4f465db` (design
`7064801b`). It adds standalone, case-insensitive `flex-shrink: inherit`
through the existing private shrink component and ancestor-style chain,
copying the computed parent shrink value while keeping omitted `flex-shrink`
local with the bounded `1` fallback. Mixed forms, source order, important
priority, finite shorthand/longhand precedence, reset semantics,
`revert-layer`, row/column and wrapped sizing, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (263/263) and the feature library (1,044 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-226` slice is recorded in
`docs/plan/tasks/native-engine-226.md` and implemented at `4ee2e1ed` (design
`c52398fd`). It adds standalone, case-insensitive `flex-basis: inherit` through
the existing private basis component and ancestor-style chain, copying the
computed parent basis value—including the bounded `auto` fallback—while
keeping omitted `flex-basis` local. Mixed forms, source order, important
priority, finite shorthand/longhand precedence, reset semantics,
`revert-layer`, row/column and wrapped sizing, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (264/264) and the feature library (1,044 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-227` slice is recorded in
`docs/plan/tasks/native-engine-227.md` and implemented at `6dde525a` (design
`330680b6`), with compatibility coverage retained at `1e7663c1`. It adds
standalone, case-insensitive `order: inherit` through the existing private
parent-style chain, copying the computed parent order while keeping omitted
`order` local with the bounded `0` fallback. Visual `(order, source_index)`
sorting continues to leave semantic/source order unchanged; mixed-invalid
forms, source order, important priority, reset semantics, `revert-layer`,
display-list, raster/PNG, point-hit, diagnostics, and the two-crate boundary
remain bounded. Full native integration (265/265) and the feature library
(1,044 passed, 1 ignored) pass locally. Remote CI, push, release, tag, and
registry publication remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-228` slice is recorded in
`docs/plan/tasks/native-engine-228.md` and implemented at `978ae3b5` (design
`8241ef68`), with compatibility coverage retained at `b645585b`. It adds
standalone, case-insensitive `gap: inherit` through the existing private
parent-style chain, copying computed parent row and column gap components while
keeping omitted `gap` local with the bounded `0` fallback. Direct
`row-gap: inherit` and `column-gap: inherit` remain unsupported by design;
mixed-invalid forms, important priority, shorthand/longhand precedence,
reset semantics, `revert-layer`, wrapped/column placement, display-list,
raster/PNG, point-hit, diagnostics, and the two-crate boundary remain bounded.
Full native integration (266/266) and the feature library (1,045 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-229` slice is recorded in
`docs/plan/tasks/native-engine-229.md` and implemented at `6c4116e4` (design
`0e2916c9`). It adds standalone, case-insensitive `row-gap: inherit` and
`column-gap: inherit` through the existing private parent-style chain, copying
the corresponding computed parent gap component only when explicitly authored
while omitted longhands remain local with the bounded `0` fallback. Independent
axis cascade, layer/importance/source-order precedence, shorthand/longhand
interaction, reset and `revert-layer` behavior, invalid mixed forms, wrapped/
column placement, display-list, raster/PNG, point-hit, semantics, diagnostics,
and the two-crate boundary remain bounded. Full native integration (267/267)
and the feature library (1,046 passed, 1 ignored) pass locally. Remote CI,
push, release, tag, and registry publication remain unclaimed for this
local-only checkout.

The completed dependency-ordered `native-engine-230` slice is recorded in
`docs/plan/tasks/native-engine-230.md` and implemented at `5d214fee` (design
`3782eb4b`). It adds standalone, case-insensitive `text-indent: inherit`
through the existing private parent-style chain, copying the computed parent
first-line indent only when explicitly authored while omitted `text-indent`
remains local with the bounded `0` fallback. Finite values, CSS-wide resets,
`revert-layer`, mixed-invalid preservation, source-order/important precedence,
first-line wrapping, text fragments, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Focused
parser/cascade coverage (3 tests), public inheritance/artifact integration (1
test), full-native integration (268/268), the feature library (1,047 passed,
1 ignored), and formatting/diff checks pass locally. Remote CI, push, release,
tag, registry publication, browser-parity, security-boundary certification,
and promotion remain unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-231` slice is recorded in
`docs/plan/tasks/native-engine-231.md` and implemented at `9a9ef2c7` (design
`57e37dfe`). It adds standalone, case-insensitive `text-overflow: inherit`
through the existing private parent-style chain, copying the computed parent
`clip|ellipsis` value only when explicitly authored while omitted
`text-overflow` remains local with the bounded `clip` fallback. Finite values,
CSS-wide resets, `revert-layer`, mixed-invalid preservation, source-order/
important precedence, eligible clipped-nowrap truncation, text fragments,
display-list, raster/PNG, overflow, point-hit, semantics, diagnostics, and the
two-crate boundary remain bounded. Focused parser/cascade coverage (3 tests),
public truncation/artifact integration (1 test), full-native integration
(269/269), the feature library (1,048 passed, 1 ignored), and formatting/diff
checks pass locally. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary certification, and promotion remain
unclaimed for this local-only checkout.

The completed dependency-ordered `native-engine-232` slice is recorded in
`docs/plan/tasks/native-engine-232.md` and implemented at `1a7df31b` (design
`0a617b8c`, contract clarification `2e8bcf9a`). It adds standalone,
case-insensitive `overflow: inherit`, `overflow-x: inherit`, and
`overflow-y: inherit` through the existing private ancestor-style chain,
copying the parent's effective bounded clip/no-clip axis projections while
omitted declarations remain local with the visible/no-clip fallback. It
preserves shorthand/longhand and important precedence, mixed-invalid
preservation, axis-specific paint clips, viewport/root-scroll projection,
display-list, fixed-cell raster/PNG, point-hit, semantics, diagnostics, and the
two-crate boundary. The focused batch passed 9 library-target tests and 19
overflow-matching native integration tests; formatting and diff checks pass.
The follow-on 233 slice adds standalone case-insensitive `initial`, `unset`,
and one-author-origin `revert` reset forms for the same three declarations,
resetting each affected axis to visible/no-clip while preserving explicit
`inherit` and named-layer `revert-layer`. Its focused reset batch passed 1
library-target test and 1 native integration test. Full issue-level gates and
remote CI remain unclaimed for this local-only checkout. The 234 slice accepts
case-insensitive finite `visible`, `auto`, and `scroll` for the same shorthand
and longhands as the existing visible/no-clip projection, without nested scroll
containers or scrollbar artifacts; its no-clip-focused pair and overflow
regression batch pass locally.

The 051 boundary adds local `opacity` values quantized to bounded 8-bit alpha
and brackets reduced-opacity rendered subtrees with immutable display-list
markers. The rasterizer composites each group inside-out through bounded
transparent layers, while layout, semantic visibility, hit testing, and root
scrolling remain unchanged; general stacking contexts and compositor parity
remain outside the boundary.
Semantic local anchor activation reaches that same bounded navigation owner for
fragment-only, fixture-relative, and absolute local hrefs; unsupported href
forms fail closed.
Visible local fragment targets may position the root viewport at the exact
case-sensitive `id` obtained after bounded percent-decoding; the active history
entry retains the bounded scroll offset for restoration.
The engine does not yet own general CSS, nested/smooth/keyboard scrolling,
scrollbars, nested overflow scrolling, or scrolling/stacking layout, complete
screenshot semantics, font/image fidelity, full visual hit-test fidelity,
full external subresource loading and network security policy, full storage
and cookie policy, chooser/programmatic downloads, continuation-aware prompts,
or platform windows. Unsupported
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
  DOM   history   runtime      resource loader layout    display list  raster
                    |
                scheduler
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
feature adds the optional `rquickjs` dependency and enables it only for
native-engine builds. The first public construction path is:

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
network endpoint. Its revision-aware navigation and action methods serialize
the observation guard with the mutation dispatch. The native runtime is not
selected by omission or automatic backend ranking.

The same feature exposes `--browser-runtime native` in the one-shot CLI. The
CLI constructs the default local configuration, so it accepts `about:blank` and
bounded percent-decoded or standard padded-base64 `data:text/html` navigation.
Rust callers can still register bounded `fixture://` documents through
`NativeEngineConfig`; fixture registration is not a CLI file-loading or
network capability. Native CLI commands include revision-guarded navigate,
click, type, clear, check, uncheck, select, key, key-down, key-up, shortcut,
scroll, text, observe, targets, DOM, evaluate, PNG screenshot, and bounded
anchor-download operations, with semantic locators instead of CSS selectors.
Endpoint access, external subresource/lifecycle streams, profiles, full
storage, prompt continuation, and TUI parity remain bounded or unsupported;
native MCP routes supported operations through the same session boundary.

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

The current resource boundary supports:

- `about:blank`, which loads an empty document;
- `data:text/html,...` with UTF-8 percent-decoded HTML;
- `data:text/html;base64,...` with standard padded RFC 4648 base64 decoding to
  UTF-8 HTML; and
- exact `fixture://...` URLs registered in `NativeEngineConfig`; and
- HTTP(S) HTML documents through the asynchronous native navigation path,
  with an eight-redirect limit, a 30-second request timeout, configured
  document-size enforcement, HTML MIME validation, bounded charset decoding,
  process-owned session cookies, and a bounded in-memory document cache.

Filesystem, custom schemes, and resource candidates denied by the shared
credential/mixed-content policy fail closed. The loader has private CSP
directive-family and CORS authorization primitives, but only bounded link
stylesheets currently call them. Service workers, permissions, full HTTP
cache freshness, persistence, and cross-origin request callers remain outside
this boundary. Only bounded link stylesheets are fetched as subresources;
images, media, fonts, scripts, and imports are not fetched by document
discovery; the explicit GET primitive can fetch one bounded connect target.
A raw fragment is
removed for resource lookup and decoding but is retained in the successful navigation URL;
percent-encoded fragment markers remain payload data. Local resources have an
opaque origin; HTTP(S) resources have a normalized tuple origin. This boundary
is not a hostile-content security boundary until the runtime/process and
network-security workstreams are complete.

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
control state, and bounded root-scroll state. If the fragment's bounded
percent-decoded UTF-8 value identifies one visible exact `id`, the root
viewport is positioned at that element's clamped document-space top; missing,
empty, malformed, invalid-UTF-8, hidden, duplicate, and non-layout targets
preserve the current offset. Explicit Rust back/forward traversal moves
the bounded history cursor; different-resource entries are parsed before
commit, while same-resource fragment entries reuse the current document and
restore the target history entry's saved root offset.

Semantic anchor clicks use the existing action path as one bounded default
action. Fragment-only hrefs resolve against the current resource; fixture
relative links use the registered fixture host; and HTTP(S) links in an
HTTP(S) document use the asynchronous content-process navigation owner with
bounded request accounting. An allowed link click remains in the current
content-process event path until cancellation is known. An anchor with a
`download` attribute queues a parent-owned cross-origin HTTP(S) transfer
instead of navigating, and the explicit runtime download operation owns the
authorized file write. Relative links from about:blank/data URLs, credentials,
unsupported schemes, and other invalid destinations fail before mutation.
Empty hrefs remain click-only. Successful non-download link activation commits
the existing same-document or parse-before-commit different-resource path.

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
parse-before-commit invariants. At the 038 checkpoint, relative paths, remote/
network navigation, downloads, target contexts, and event-loop/default-action
behavior remained unsupported; 040 adds only bounded fixture-relative
resolution. Its implementation and validation evidence are recorded in the
task file and issue #40.

The 039 bounded-fragment-target-scroll boundary resolved one exact raw local
`id` through the existing document/layout owner and positioned the root viewport
at its clamped document-space top. The later 041 bounded-percent-decoded-
fragment-target boundary applied bounded UTF-8 percent decoding before that
exact lookup, 042 added the legacy `<a name>` fallback, and 043 added simple
text-fragment matching. Every history entry stores one bounded root scroll
point; scroll actions update the active entry and traversal restores it,
including reparsed different-resource entries. Prefix/suffix text-fragment
syntax was unsupported at this 039 checkpoint; the later 044 checkpoint adds
bounded exact same-run affixes. Duplicate-id recovery beyond decoded equality,
malformed URL policy, smooth/nested/horizontal/keyboard/snap scrolling, sticky
layout, and browser alignment parity remain unsupported. Each implementation and validation
boundary is recorded in its task file and issue #40.

The 040 bounded-relative-local-links boundary resolves non-absolute link
references only from registered `fixture://` documents, retaining the current
fixture host and using the bounded URL parser for deterministic path/query/
fragment normalization. Relative links from opaque `about:blank` or `data:`
documents, host-changing or malformed references, remote schemes, and missing
fixtures fail before mutation; same-/different-resource commit, anchor scroll,
and per-entry history restoration continue through the existing owners. Its
implementation and validation evidence are recorded in the task file and issue
#40.

At the 041 bounded-percent-decoded-fragment-target checkpoint, local fragment
navigation decoded bounded percent-encoded UTF-8 bytes before the existing
exact visible `id` lookup. Literal `+` remained a plus, and malformed or
invalid-UTF-8 fragments remained successful unresolved-target navigations that
preserved scroll. The later 042 and 043 checkpoints add legacy name anchors and
simple text fragments; general URL-decoding parity remains unsupported. Its
implementation and validation evidence are recorded in the task file and issue
#40.

The 042 bounded-legacy-name-fragment-target boundary is complete locally:
after 041 decoding, a unique exact `<a name>` anchor is used only when no
matching `id` exists. IDs retain precedence, duplicate IDs or names fail
closed, and the existing visible-layout-box, root-scroll, history, and
failure-atomicity rules remain unchanged. Arbitrary `name` attributes and
browser URL/scrolling parity remain unsupported; the subsequent 043 checkpoint
adds only the bounded text-fragment form. Its implementation and validation
evidence are recorded in the task file and issue #40.

The 043 bounded-text-fragment-target boundary is complete locally: a simple
`#:~:text=start` or `#:~:text=start,end` request matches the first visible,
non-truncated text run after per-term bounded UTF-8 decoding. Prefix/suffix
syntax, cross-run ranges, highlights, and browser text-fragment parity remain
unsupported. Its implementation and validation evidence are recorded in the
task file and issue #40.

The 044 bounded-text-fragment-affix boundary is complete locally. It adds the
accepted exact same-run `prefix-,start`, `start,-suffix`, and combined affix
forms around the existing `start[,end]` matcher, including the corresponding
forms with an ordered end term. Raw-comma separation, per-term bounded UTF-8
decoding, first-visible-run order, root-scroll/history behavior, and
fail-closed handling for malformed or unsupported forms remain intact.
Cross-run ranges, multiple directives, highlights, Unicode normalization, and
browser text-fragment parity remain unsupported. Its implementation and
validation evidence are recorded in the task file and issue #40.

The 045 bounded-root-horizontal-scroll boundary is complete locally. It
extends the existing root scroll offset with bounded document width,
independent horizontal/vertical clamping, and the already shared viewport,
hit-test, display-list, and raster projections. Nested scroll containers,
scrollbars, smooth scrolling, and axis-specific CSS overflow remain outside
the slice. Its implementation and validation evidence are recorded in the
task file and issue #40.

The completed 046 bounded-nowrap-whitespace boundary adds inherited
`white-space: nowrap` to the existing collapsed fixed-cell text flow. It keeps
the resulting text on one measured line and reaches the 045 root horizontal
scroll path through the existing layout, viewport, hit-test, display-list,
raster, action, and history owners. Preserved whitespace modes beyond the
supported subset, nested scrolling, scrollbars, and browser line-breaking
parity remain outside the slice. Its implementation and validation evidence
are recorded in the task file and issue #40.

The completed 047 bounded-inherited-line-height boundary propagates the
existing positive pixel `line-height` floor through the DOM style walk so
descendants without a valid local declaration use the nearest computed value.
Font-relative metrics, CSS-wide keywords, baselines, `vertical-align`, and
general computed-style inheritance remain outside the slice.

The completed 048 bounded-clip-aware-root-overflow boundary measures text
contribution to root `content_width` through the same document-space overflow
clip already shared by paint, viewport projection, and point hit testing, so
fully clipped text cannot create a false horizontal scroll range.

The completed 049 bounded-axis-specific-overflow boundary accepts only bounded
`overflow-x`/`overflow-y` `hidden` and `clip` values, cascades the two axes
independently, and preserves the existing rectangular clip owner across paint,
viewport projection, hit testing, and root-overflow measurement.

The completed 050 bounded-min-max-dimensions boundary applies bounded physical
minimum and maximum pixel constraints to the existing content-box or border-box
width/height calculations while preserving normal flow and all downstream
geometry owners.

The completed 051 bounded-opacity-groups boundary adds local CSS opacity as
real subtree compositing. Reduced-opacity elements bracket their own box and
rendered descendants with display-list markers; the software rasterizer
replays each group in a transparent layer before source-over compositing it
onto its parent. Fixed-point alpha parsing, nested groups, `display:contents`,
zero-opacity layout/hit behavior, and explicit layer budgets are in scope;
general stacking, transforms, filters, animation, and browser compositor parity
remain outside the boundary. The completed 052 boundary adds bounded inherited
physical text alignment for fixed-cell direct text and supported inline boxes;
logical directions, justification, and browser inline-formatting parity remain
outside the boundary. The completed 053 boundary extends the existing bounded
color grammar with decimal-channel `rgba(R, G, B, A)` values using fixed-point
alpha quantization and the existing source-over raster path; CSS Color 4,
wide-gamut, interpolation, and color-management behavior remain outside the
boundary. The completed 054 boundary adds inherited fixed-cell
`text-decoration:none|underline` as a text-command bit and a clipped,
alpha-aware one-pixel baseline-offset replay. Font metrics, decoration
propagation, and browser text-paint parity remain outside the boundary. Its
implementation and validation evidence are recorded in the 054 task file and
issue #40. The completed 055 boundary adds inherited ASCII
`text-transform:none|uppercase|lowercase` during fixed-cell layout before
whitespace handling, wrapping, text-fragment matching, display-list projection,
and root-overflow measurement. Semantic source text remains unchanged; Unicode
case mapping, locale behavior, and font-specific glyph metrics remain outside
the boundary. Implementation and validation evidence are recorded in the 055
task file and issue #40.
The completed 056 boundary adds non-negative fixed-pixel `text-indent` for the
first line of block containers. It reduces only that line's fixed-cell
capacity, resets later lines to the full content width, and leaves inline and
`display:contents` elements on their containing block's flow. The effective
indent is clamped to retain one fixed cell; negative, percentage, and
font-relative forms remain outside the boundary. Implementation and local
validation evidence are recorded in the 056 task file and issue #40; remote
CI remains pending until this branch is pushed.
The completed 057 boundary adds bounded inherited non-negative fixed-pixel
`word-spacing` to the existing fixed-cell text-flow owner. Collapsed ASCII
separator spaces receive the extra advance in `normal`, `nowrap`, and
`pre-line`; literal ASCII spaces receive it in `pre` and `pre-wrap`. The
measured advance is shared by wrapping, preformatted chunking, text fragments,
alignment, display commands, raster replay, hit testing, and root overflow;
negative, relative, percentage, keyword, Unicode-whitespace, and browser
word-boundary behavior remain outside the boundary. Implementation and local
validation evidence are recorded in the 057 task file and issue #40; remote
CI remains pending until this branch is pushed.
The completed 058 boundary adds bounded inherited non-negative fixed-pixel
`letter-spacing` after every rendered fixed-cell character in each emitted
fragment, including the final character, and composes it with the existing
word-spacing advance on ASCII spaces. The measured result is shared by
wrapping, preformatted chunking, text fragments, alignment, display commands,
raster replay, hit testing, and root overflow. Fragment and line boundaries,
pair-boundary typography, Unicode shaping, font metrics, negative/relative/
percentage values, `normal`, and browser parity remain outside the boundary.
Its implementation is committed locally as `cb191a3`; the task file records
the local validation evidence, and remote CI remains pending until this branch
is pushed.

The completed 059 boundary adds inherited `font-weight: normal|bold|400|700`
to the fixed-cell text path. `normal`/`400` retain the existing glyph replay
and `bold`/`700` apply a one-pixel horizontal dilation of set glyph pixels
through the existing clipped software rasterizer. Fixed-cell advances,
wrapping, origins, overflow, hit testing, semantics, and capture coordinates
do not change. Font selection/loading, metrics, numeric interpolation,
variable fonts, shaping, anti-aliasing, and browser text-rendering parity
remain unsupported. Its implementation is committed locally as `21fcff5`; the
059 task file records the local validation evidence, and remote CI remains
pending until this branch is pushed.

The completed 060 boundary adds inherited `font-style: normal|italic` to the
fixed-cell text path. `normal` retains the existing glyph replay and `italic`
applies a deterministic bounded row-dependent horizontal shear through the
existing clipped software rasterizer. Bold dilation, underline, spacing,
opacity, scrolling, and capture compose through the same immutable text
command and raster owners; fixed-cell advances, wrapping, origins, overflow,
hit testing, semantics, and capture coordinates do not change. Oblique forms,
angles, font selection/loading/metrics, shaping, anti-aliasing, and browser
text-rendering parity remain unsupported. Its implementation is committed
locally as `2992eb8`; the 060 task file records the local validation evidence,
and remote CI remains pending until this branch is pushed.

The completed 061 boundary adds inherited `word-break: normal|break-all` to
the bounded collapsed fixed-cell flow path. `normal` retains word-aware
wrapping; `break-all` permits deterministic character-boundary splitting for
every collapsed word while preserving the existing separator, spacing,
fragment, overflow, and semantic owners. `pre`, `pre-wrap`, and `nowrap`
retain their existing whitespace behavior. Unicode line-breaking, grapheme
policy, hyphenation, `overflow-wrap`, bidi, writing modes, font metrics, and
browser conformance remain outside the boundary. Its design is `14d7fc4`, its
implementation is `479f3a3`, and its documentation closeout is `433d6fd`;
the 061 task file records the local validation evidence, and remote CI remains
pending until this branch is pushed.

The completed 062 boundary adds local `text-overflow: clip|ellipsis` to the
bounded single-line fixed-cell path. `clip` retains the existing full visual
run under the horizontal overflow clip; eligible `ellipsis` blocks replace an
overflowing suffix with a spacing-aware fixed-cell ASCII `...` marker while
retaining the full semantic source text. The implementation is restricted to
one direct text child in a rendered `nowrap` block with a finite horizontal
clip; multi-line truncation, nested inline formatting, Unicode ellipsis
behavior, and browser conformance remain outside the boundary. Its design is
`7e488aa`, its implementation is `e4c5bb1`, and the 062 task file records the
local validation evidence; remote CI remains pending until this branch is
pushed.

The completed 063 boundary adds inherited
`vertical-align: baseline|top|middle|bottom` to the existing fixed-cell
inline-flow line-item owner. `baseline` preserves the current top-origin
behavior; `top`, `middle`, and `bottom` apply bounded integer offsets within
the existing line box and move an inline item's boxes and text artifacts
together. Font metrics, typographic baselines, lengths, percentages, bidi,
writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. Its design is `7721df2`, its implementation is
`facd2f6`, and the task file records the local validation evidence; remote CI
remains pending until this branch is pushed.

The completed 064 boundary adds block-level `display: flex` for eligible
containers with direct element children. Each item is laid out once in source
order at its explicit or intrinsic fixed width, including physical margins,
against the existing content-box origin. Items do not grow, shrink, wrap,
reverse, reorder, distribute free space, or stretch across the cross axis.
Meaningful direct text, `display: contents`, and visible `<br>` children use
the established normal-flow fallback so the bounded mode never drops source
content. The contract is recorded in
`docs/plan/tasks/native-engine-064.md`; implementation `7c38354` and local
validation evidence are recorded in that task file. Remote CI remains pending
until this branch is pushed.

The completed 065 boundary adds one non-negative fixed-pixel `gap` value between
visible direct element items in an eligible bounded flex row. Hidden and
`display:none` items do not consume a gap position; ineligible containers keep
the existing normal-flow fallback. Multi-value and percentage gap grammar,
`row-gap`, `column-gap`, flex distribution, wrapping, and general Flexbox
remain outside the claim. The contract is recorded in
`docs/plan/tasks/native-engine-065.md`; implementation `28735c4` and local
validation evidence are recorded in that task file. Remote CI remains pending
until this branch is pushed.

The completed 066 boundary adds bounded
`justify-content:flex-start|center|flex-end|space-between` to eligible
fixed-width flex rows. Positive free space is placed before the row or
distributed across the existing gaps with deterministic integer rounding;
overflowing rows retain a zero leading offset and the existing root-scroll
path. The contract is recorded in
`docs/plan/tasks/native-engine-066.md`; design is `a53b10f`,
implementation is `3fe5306`, and the current-source documentation closeout
is recorded in this checkpoint. Remote CI remains pending because the branch
is local-only.

The completed 067 boundary adds bounded non-inherited signed `order` values in
the inclusive range `-1024..=1024` for eligible fixed-width flex rows. Visual
items sort by `(order, source_index)` before the existing gap and
`justify-content` distribution, while semantic DOM/source order remains
unchanged. The contract is recorded in
`docs/plan/tasks/native-engine-067.md`; design is `09f3b00`, implementation is
`a713b6e`, and current-source documentation closeout is recorded in this
checkpoint. Remote CI remains pending because the branch is local-only.

The completed 068 boundary adds bounded non-inherited
`align-items:flex-start|center|flex-end` to eligible fixed-width single-row
flex rows. It aligns complete visual item subtrees within an explicit content
height or the auto row's maximum item outer height using deterministic integer
offsets, while preserving horizontal and semantic/source order. The contract
and exclusions are recorded in
`docs/plan/tasks/native-engine-068.md`; design is `a0488ef`, implementation is
`6b55b9c`, and current-source documentation closeout is recorded in this
checkpoint. Remote CI remains pending because the branch is local-only.

The completed 069 boundary adds bounded non-inherited
`flex-direction:row|row-reverse` to the same eligible single-row flex rows.
`row` remains coordinate-equivalent to 068; `row-reverse` walks the
order-sorted visual sequence from the physical right edge while preserving
margins, gap, justification, cross-axis alignment, shared subtree artifacts,
non-negative coordinates, root horizontal scrolling, and semantic/source
order. The contract and exclusions are recorded in
`docs/plan/tasks/native-engine-069.md`; design is `7fea901`, implementation is
`be11f49`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered `native-engine-070` slice adds bounded
non-inherited `flex-wrap:nowrap|wrap` to the same eligible fixed-width flex
rows. `nowrap` remains coordinate-equivalent to 069; `wrap` forms deterministic
physical lines from measured item outer widths plus the existing gap, reuses
per-line row/reverse placement, justification, and cross-axis alignment, and
preserves complete subtree artifacts, root overflow, and semantic/source order.
The contract and exclusions are recorded in
`docs/plan/tasks/native-engine-070.md`; design is `8772a6a`, implementation is
`5c16185`, and local validation evidence is recorded in the task file. Remote
CI remains pending because the branch is local-only.

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
margin collapsing. Percentages, negative/auto values, logical sides,
positioning, flex/grid, and fractional metrics remained
unsupported at the 016 checkpoint; later 045 adds bounded root horizontal
scrolling without nested scrolling.

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

The native backend accepts semantic `Click`, `Type`, focused-text `KeyPress`,
`KeyDown`, `KeyUp`, and `Shortcut` actions plus bounded vertical
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
replaces private state for native `input` and `textarea` textboxes. KeyPress
and unmodified editing shortcuts insert at the owned caret, replace selected
ranges, and remove the adjacent Unicode scalar for Backspace/Delete; Ctrl/Meta+A
and bounded arrow/Home/End defaults update the owned range. A
bounded visibility gate recognizes `hidden`, `aria-hidden="true"`, and computed
`display:none`/`visibility:hidden`; hidden subtrees are omitted from text,
layout, and hit testing. Supported local links perform the bounded default
navigation described above; unsupported links fail closed. No action performs
grapheme/bidi caret geometry, IME, clipboard, and platform keyboard services
remain outside this bounded selection model; form defaults are limited to the
bounded GET submission path above.
Each accepted mutating action advances the document revision exactly once, so
earlier references must be re-observed. The effects operation returns the current
revision and changed bit; bounded native event metadata remains an internal
Rust inspection surface.

## Backend capability contract

The native profile is `experimental` and declares:

| Capability | Level | Current contract |
|---|---|---|
| lifecycle | available | initialize and explicit close |
| navigation | available | local resources plus bounded external HTTP(S) HTML navigation, inline/classic/module-root/static-graph/literal-dynamic-import child scripts with bounded parser-blocking/async/defer ordering, bounded task turns, and bounded GET or POST form navigation with urlencoded, multipart, and text/plain encodings plus validated submitter overrides; no computed module loading or general subresources |
| contexts | available | up to 32 independent page targets with one explicitly selected active target; create, select, list, and close are native-owned |
| evidence | available | bounded URL, title, visible text, revision; native semantic projection is Rust-only |
| action | available | semantic click/type, focused-text printable/Backspace/Delete key input, bounded single- and multi-select option interaction, bounded vertical root scrolling, bounded GET or POST form defaults with supported encodings and submitter overrides, plus native point targets for supported local controls; text selection, IME, and nested scrolling remain open |
| effects | available | current revision and changed signal; bounded event metadata is Rust-only |
| script | available | bounded QuickJS ECMAScript with a refreshed `window`/`document` snapshot, bounded inline/classic/module-root page-script loading and parser/lifecycle ordering, HTTP(S) static module graphs, literal dynamic imports, bounded due-time `setTimeout`/`setInterval` turns, policy-owned bounded GET, same-origin string-body POST, and bounded cross-origin simple/preflighted POST `fetch()` promises with independent text/json/blob/UTF-8 arrayBuffer/bytes response reads and bounded one-chunk `ReadableStream` response bodies with reader lock/release/cancel, a bounded read-only response Headers view with duplicate-name combination and same-origin/CORS-exposed filtering, and raw bounded byte-preserving response payloads for response body variants and binary Blob slicing, bounded Blob/File construction from ArrayBuffer and typed-array parts, bounded raw-byte-backed Blob/File request bodies for Fetch and asynchronous XHR, bounded mutable Fetch Headers records with live owner-backed iterators plus plain-object custom request headers with JavaScript/Rust validation, forbidden/internal-header protection, same-origin transfer, and sorted multi-header CORS preflight authorization with bounded positive-`Access-Control-Max-Age` caching, bounded Fetch `cors`/`no-cors`/`same-origin` mode policy with fail-closed same-origin and no-cors request checks plus opaque cross-origin no-cors response projection, bounded Fetch `follow`/`error`/`manual` redirect policy with `redirected` and filtered `opaqueredirect` response projection, direct text-backed Blob/File request bodies with normalized MIME propagation and bounded observable fetch AbortController/AbortSignal cancellation with static abort/timeout/any composition, bounded text-only `FormData(form)` construction, text-backed and raw-byte-backed Blob/File parts, and multipart bodies with Rust-owned form association plus bounded live owner-backed `entries()`/`keys()`/`values()`/`[Symbol.iterator]()` iterators, and bounded URLSearchParams construction from strings, records, pair arrays, and pair iterables, mutation, sorting, live entries/keys/values iteration, and URL-encoded bodies, plus asynchronous bounded GET/POST `XMLHttpRequest` with string, text-backed, and raw-byte-backed Blob/File request bodies, bounded `arraybuffer`/`blob` response types, bounded non-zero timeout with zero disabling the extra deadline, request-local abort/reset state, bounded `readystatechange`/`abort`/`timeout` callbacks, and stale-continuation suppression, from explicit evaluations and initial page scripts, typed click/form-submit/attribute/focus commands, persistent listener records, bounded Event/CustomEvent capture/target/bubble dispatch, cancelable click preflight, transactional type/input/change event re-entry, bounded common constraint validation for required/email/URL/length/numeric/date/month/time/datetime-local/pattern controls, bounded `validity`/`validationMessage`/`willValidate` snapshots with `checkValidity()`/`reportValidity()` and custom validity, submitter event metadata and successful-control serialization, bounded external form ownership, bounded multipart/text/plain form encodings and validated `formaction`/`formmethod`/`formenctype` overrides, bounded `readyState`/`readystatechange`/DOMContentLoaded/load phase ordering, form `novalidate`/`formnovalidate` bypass, top-level link/form navigation handoff, and relative HTTP(S)/same-document resolution; no live Web IDL identity, child-frame execution, resource-specific lifecycle parity, beforeinput/composition, full JavaScript RegExp `v`-flag/Unicode-set and file constraint validation or picker/UI parity, full live `ValidityState` identity, private-network access, streaming FormData body parity, synchronous XHR, XHR upload/progress/streaming parity, transport-level fetch cancellation, full transport-streaming Response bodies, invalid raw response-header bytes, response trailers, service workers, WebSocket/EventSource, animation/idle callbacks, task-source fairness, background page scheduling, computed imports, bare specifiers/import maps, local external subresources, or general page loading |
| capture | available | bounded PNG of the current logical RGBA surface; JPEG/PDF and screenshot-containing evidence are unavailable |
| storage | partial | process-owned cookies with bounded `document.cookie` synchronization, bounded document cache, origin-keyed page local/session storage with opt-in revisioned localStorage/cookie/IndexedDB profiles, stale-snapshot key-level merge for Web Storage and cookies, profile-journal local/session events across live local and process-backed documents, bounded reader-lease retention, acknowledged-prefix compaction, profile-snapshot recovery, a bounded StorageManager estimate against the fixed 4 MiB profile quota, and a bounded tagged JSON/structured-clone IndexedDB subset with text-backed Blob/File values plus byte-vector ArrayBuffer/typed-array/DataView values, bounded Blob/File `arrayBuffer()`/`bytes()` reads, version upgrades, same-realm version-change/deletion coordination, serialized atomic ordinary transactions, object stores, indexes, key ranges, cursors, and CRUD; no full cookie policy or IndexedDB parity |
| prompts | partial | bounded alert/confirm/prompt metadata, FIFO pending state, `dialogOpen`, and accept/dismiss resolution; suspended modal continuation and response injection remain open |
| downloads | available | bounded HTTP(S) anchor `download` attributes queue a parent-owned transfer; runtime, CLI, and MCP complete the oldest queued download for the selected target into an existing directory with sanitized collision-free file creation, SHA-256 evidence, stable completion IDs, and bounded cancellation/listing; chooser UI, programmatic/object-URL downloads, streaming/progress, service-worker interception, and cross-target/frame parity remain open |

Within the available script profile, native Fetch `Response.clone()` creates a
bounded fresh response/header/body owner; full disturbance and Web IDL
semantics remain explicitly outside the profile.

The profile limitations are surfaced through `BackendProfile`. The dispatcher
returns typed capability denials for omitted operations. `EvidenceLevel::Deep`
can return a bounded incomplete projection; screenshot-containing levels are
explicitly denied. The separate capture operation supports only bounded PNG
bytes. No operation silently falls back to CDP, the proof backend, or another
resource loader.

## Errors and recovery

`NativeEngineError` distinguishes invalid configuration, lifecycle misuse,
unsupported resources, bounded network failures, parser failures, resource
limits, scheduler failure, target resolution/actionability failures, and
effect-query revision errors.
The backend translates those errors to the existing bounded
`BrowserBackendError` variants.

Navigation follows this transaction boundary:

```text
validate URL -> load resource -> parse new DOM -> schedule commit
      |                 |                    |              |
      +-- error: no engine mutation --------+--------------+
                                      commit revision/history/document
```

The engine never logs source HTML, evaluated input, credentials, cookies, form
values, or full documents. The bounded network slice and later scripting work
must preserve the same transaction, origin, cancellation, and redaction
boundaries.

Profile-backed storage has a separate bounded recovery boundary. The parent
engine is the only journal writer and registers a reader lease under the
profile lock. A missing or overrun lease is treated as stale-reader recovery:
the engine reloads the revisioned profile snapshot, replaces the active
storage state (preserving only the live engine's volatile session map), and
uses a typed full-state transfer for a sandboxed content worker. Event
callbacks that cannot be proven unconsumed are not replayed. Compaction drops
only complete records acknowledged by every non-stale reader lease; a live
reader that prevents the 4 MiB bound receives typed backpressure.

## Tests and promotion boundary

Phase 1 and current Phase 2 semantic-DOM/interaction tests cover:

- lifecycle transitions and repeated/invalid close behavior;
- `about:blank`, percent-decoded `data:` HTML, registered fixtures, and
  bounded external HTTP(S) HTML documents;
- raw-fragment URL retention and bounded same-document local navigation;
- exact visible fragment-target scroll and saved history offsets;
- bounded percent-decoded fragment identifiers for exact visible local IDs;
- bounded legacy `<a name>` fragment identifiers as an exact-ID fallback;
- bounded `#:~:text=start[,end]` visible text-fragment targets;
- bounded text-fragment prefix/suffix affixes within one visible text run;
- bounded root horizontal/vertical scroll extents, independent clamping, and
  two-axis projection through layout, hit testing, display, raster, and history;
- bounded independent `overflow-x`/`overflow-y` `hidden`/`clip` rectangles
  through paint, viewport projection, point hit testing, and root-overflow
  measurement, with shorthand/longhand cascade precedence;
- semantic local anchor activation through dispatcher click;
- fixture-relative local link resolution with same-host enforcement;
- title and visible-text projection with hidden `head`, `script`, and `style`;
- bounded history entries, monotonic revisions, and explicit Rust traversal;
- bounded alert/confirm/prompt metadata, content-worker forwarding, pending
  dialog inspection, `dialogOpen` verification, and native accept/dismiss
  routing;
- bounded native request operation accounting, completion sequencing, and
  `network-quiet` waits across navigation, direct fetch, and external script
  operations;
- direct external anchor activation through the content-process event bridge,
  click cancellation, and asynchronous native navigation;
- cross-origin anchor `download` activation, parent-owned queued transfer,
  collision-free existing-directory writes, completion digest/IDs, and
  download-start verification;
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
- local and child-owned QuickJS host projection, persistent globals, opaque and
  tuple origins, bounded top-level `await`, typed JavaScript DOM commands,
  one-revision command batches, form/option state, attributes, effects, and
  fresh host re-projection.
- opt-in revisioned local-storage/cookie profiles, origin/context-filtered
  storage events across local and sandboxed content processes, bounded `P.events`
  journal records, `P.readers` leases with stale-reader recovery, acknowledged
  prefix compaction under `P.lock`, and full storage-state replacement IPC.
- raw-text/RCDATA containment for script, style, title, and textarea content,
  including unterminated-element behavior.
- bounded stylesheet/inline selector matching and display/visibility cascade
  feeding visible text and semantic actionability.
- deterministic integer-pixel normal-flow layout, bounded width/height
  declarations and min/max width/height constraints, physical padding/margin
  shorthand and longhand cascade, explicit box sizing, outer/content
  rectangles, hidden-box exclusion, and Rust-only layout inspection.
- point hit testing with viewport bounds, deepest-hit ordering, actionable
  ancestor resolution, and pre-mutation rejection for empty/out-of-viewport
  points.
- matching-revision display-list generation with bounded clear/fill/text/border
  commands and explicit unsupported-paint boundaries.
- deterministic bounded RGBA surface replay, source-over blending, fixed-glyph
  text drawing, viewport clipping, and explicit surface-allocation limits.
- bounded local opacity parsing and subtree display-list markers, nested
  transparent-layer replay, inside-out source-over group compositing, and
  explicit aggregate layer depth/pixel limits without changing layout or hit
  testing.
- bounded inherited physical `text-align:left|center|right` parsing and
  cascade, deterministic fixed-cell line offsets across direct text and
  supported inline boxes, and shared layout/paint/raster/hit-test coordinates.
- bounded 15-layer/unlayered `revert-layer` rollback for inherited
  `text-align`, `text-align-last`, and `text-justify`, with private declaration
  state and unchanged line/artifact owners.
- bounded standalone case-insensitive `inherit`, `initial`, `unset`, and
  one-author-origin `revert` for inherited `text-align`, `text-align-last`,
  `text-justify`, and `direction`, with parent/root fallbacks, invalid-later
  preservation, logical `ltr`/`rtl` projection, and unchanged layout,
  display-list, raster, point-hit, semantic, and diagnostic owners.
- bounded 15-layer/unlayered `revert-layer` rollback for inherited
  `white-space`, with private declaration state, five finite modes, inherited/
  root fallback, and unchanged line-flow/artifact owners.
- bounded 15-layer/unlayered `revert-layer` rollback for inherited
  `line-height`, with private declaration state, positive-pixel `Option<u32>`
  fallback, and unchanged flow/artifact owners.
- bounded 15-layer/unlayered `revert-layer` rollback for inherited
  `direction`, with private declaration state, finite `ltr|rtl` fallback,
  unchanged logical text/flex/wrapped-line mapping, and preserved
  source/semantic order and shared artifacts.
- bounded 15-layer/unlayered `revert-layer` rollback for non-inherited
  `flex-direction`, with private declaration state, finite row/row-reverse/
  column/column-reverse values, local row fallback, finite `flex-flow`
  expansion, and preserved flex/artifact owners.
- bounded 15-layer/unlayered `revert-layer` rollback for non-inherited
  `flex-wrap`, `justify-content`, `align-items`, `align-self`, and
  `align-content`, with private declaration state, native fallbacks, finite
  `flex-flow`/`place-content` expansion, and preserved flex line/item
  artifacts.
- bounded 15-layer/unlayered `revert-layer` rollback for non-inherited flex
  item sizing and the standalone `flex`, `flex-flow`, and `place-content`
  shorthands, with private component state, finite expansion, independent
  fallback resolution, and preserved flex line/item artifacts.
- bounded `rgba(R, G, B, A)` functional alpha parsing for background, border,
  and text colors, with shared fixed-point quantization, display-list color
  ownership, and integer source-over replay.
- bounded inherited `text-decoration:none|underline|overline|line-through`
  parsing and cascade, including distinct shorthand combinations, compact
  immutable text-command decoration bits, and deterministic clipped
  alpha-aware fixed-cell line replay.
- bounded inherited ASCII `text-transform:none|uppercase|lowercase` parsing and
  cascade, transformed fixed-cell layout fragments, and consistent wrapping,
  text-fragment, display-list, and root-overflow consumers.
- bounded inherited `vertical-align:baseline|top|middle|bottom` parsing and
  cascade, with clamped fixed-cell top/middle/bottom offsets applied to
  complete inline-item box and text artifact ranges during line flush.
- bounded non-inherited `justify-content:normal|flex-start|center|flex-end|
  space-between|space-around|space-evenly|stretch`
  parsing and cascade, fixed-width flex-row free-space placement, deterministic
  gap distribution, overflow preservation, and shared layout/paint/scroll/
  hit-test coordinates.
- bounded non-inherited signed flex-item `order` in `-1024..=1024` parsing and
  cascade, stable visual `(order, source_index)` sorting, source-order semantic
  preservation, and shared layout/paint/scroll/hit-test coordinates.
- bounded non-inherited `align-items:flex-start|center|flex-end` parsing and
  cascade, explicit/auto cross-axis line sizing, complete item artifact
  translation, overflow preservation, and shared layout/paint/scroll/
  hit-test coordinates.
- bounded non-inherited `align-content:flex-start|center|flex-end|space-between`
  for wrapped rows, with explicit-height positive cross-line distribution and
  complete line artifact translation.
- bounded non-inherited `align-content:space-around` for wrapped rows, with
  saturating integer slot-center offsets and complete line artifact translation.
- bounded non-inherited `align-content:space-evenly` for wrapped rows, with
  equal leading, inter-line, and trailing integer slots and complete line
  artifact translation.
- bounded non-inherited `flex-wrap:wrap-reverse` for eligible fixed-width
  rows, preserving source-order line formation while reversing physical line
  stacking with signed complete-artifact translation.
- bounded explicit non-inherited `align-content:stretch` for wrapped
  fixed-width rows, expanding line heights by deterministic integer shares and
  preserving the shared normal/wrap-reverse artifact consumers.
- bounded explicit case-insensitive `align-content:inherit` propagation
  through the private parent-style chain, while omitted `align-content` stays
  non-inherited and uses the existing `flex-start` fallback.
- bounded explicit case-insensitive `justify-content:inherit` propagation
  through the private parent-style chain, while omitted `justify-content` stays
  non-inherited and uses the existing `flex-start` fallback.
- bounded explicit case-insensitive `align-items:inherit` propagation through
  the private parent-style chain, while omitted `align-items` stays
  non-inherited and uses the existing `flex-start` fallback.
- bounded explicit case-insensitive `align-self:inherit` propagation through
  the private parent-style chain, while omitted `align-self` remains local
  `auto` and explicit `auto` delegates to the containing `align-items` owner.
- bounded explicit case-insensitive `flex-direction:inherit` propagation
  through the private parent-style chain, while omitted `flex-direction`
  remains local with the bounded `row` fallback and finite `flex-flow`/
  longhand component precedence remains intact.
- bounded explicit case-insensitive `flex-wrap:inherit` propagation through
  the private parent-style chain, while omitted `flex-wrap` remains local with
  the bounded `nowrap` fallback and finite `flex-flow`/longhand component
  precedence remains intact.
- bounded explicit case-insensitive `flex-flow:inherit` projection through
  the existing private direction and wrap owners, while omitted `flex-flow`
  remains local with the bounded `row`/`nowrap` component fallbacks.
- bounded explicit case-insensitive `flex:inherit` projection through the
  existing private grow, shrink, and basis owners, while omitted `flex` remains
  local with the bounded `0 1 auto` component fallbacks.
- bounded explicit case-insensitive `flex-grow:inherit` propagation through
  the existing private grow owner, while omitted `flex-grow` remains local with
  the bounded `0` fallback.
- bounded explicit case-insensitive `flex-shrink:inherit` propagation through
  the existing private shrink owner, while omitted `flex-shrink` remains local
  with the bounded `1` fallback.
- bounded explicit case-insensitive `flex-basis:inherit` propagation through
  the existing private basis owner, while omitted `flex-basis` remains local
  with the bounded `auto` fallback.
- bounded explicit case-insensitive `row-gap:inherit` and
  `column-gap:inherit` propagation through the existing private row/column gap
  owners, while omitted longhands remain local with the bounded `0` fallback
  and shorthand/longhand precedence remains bounded.
- bounded non-negative fixed-pixel `text-indent` parsing and cascade for the
  first line of block containers, with one-cell clamping and shared
  layout/wrapping/fragment/paint/hit-test/overflow coordinates.
- bounded inherited non-negative fixed-pixel `word-spacing` across collapsed
  and preformatted ASCII spaces, with shared wrapping, fragment, alignment,
  display-list, raster, hit-test, and root-overflow coordinates.
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
- bounded positive-pixel line-height parsing/cascade, bounded inheritance
  through the DOM style walk, flow minimums, inline auto-height behavior, and
  explicit-height precedence.
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
  local and bounded HTTP(S) URL shapes, including rejection of remote browser
  endpoints and unsupported browser-only flags.

The completed native-engine-browser-194 slice closes document-type construction
after parsed projection. `document.implementation.createDocumentType()` now
creates bounded `DocumentType` nodes with metadata, cloning, document-root
insertion, duplicate/hierarchy validation, frame batching, and persistent
identity refresh across local, HTTP(S) content-worker, same-origin frame, and
nested-frame realms. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-194.md`; broader DOMImplementation,
tree-builder, and Web IDL/conformance promotion remain issue #40 work. The
bounded malformed-input recovery subset is recorded in the later
`native-engine-browser-195` checkpoint.

The completed native-engine-browser-197 slice completes bounded `Attr`
participation in the shared `Node` contract. Constructed attributes now expose
detached node accessors, clone/equality behavior, empty child collections, and
root/connectivity semantics while retaining `ownerElement` as their sole
ownership link. Local, content-worker, and same-origin frame realms use the
same host implementation. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-197.md`. Namespace-aware storage, XML
documents, complete Web IDL descriptor parity, and browser-wide conformance
remain issue #40 work.

The completed native-engine-browser-201 slice extends that bounded attribute
surface with namespace-qualified identity. The native owner and content-worker
wire retain a qualified-name namespace map; local, detached, HTTP(S), and
same-origin frame realms expose XLink/XML/XMLNS-aware `getAttributeNS`,
`setAttributeNS`, `removeAttributeNS`, `Attr`, and `NamedNodeMap` behavior; and
cloning plus foreign-content fragment parsing preserve the same identity.
Unsupported namespace URIs and reserved-prefix mismatches fail closed with
`NamespaceError`. XML documents, namespace-aware CSS selectors, complete Web
IDL descriptor parity, and browser-wide conformance remain issue #40 promotion
work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-201.md`.

The completed native-engine-browser-202 slice adds a typed `SvgStroke` display
command for the existing SVG `rect`, `circle`, and `ellipse` layout families.
The software rasterizer paints deterministic inside-bounded rings with shared
scroll translation, clipping, alpha blending, fill-less behavior, bounded
numeric/`px` widths, `stroke="none"`, and `currentColor` resolution. SVG paths,
line caps/joins, transforms, viewBox mapping, gradients, markers, and external
resources remain later issue #40 promotion work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-202.md`.

The completed native-engine-browser-203 slice adds bounded SVG `line`,
`polyline`, and `polygon` geometry. Shared point parsing feeds layout bounds,
typed `SvgPolygonFill` and `SvgPolyline` commands, even-odd polygon fill,
segment-distance stroke coverage, clipping, scroll translation, alpha
composition, and hit-test ownership. Malformed or over-limit point lists fail
closed. SVG paths, explicit cap/join styles, dash arrays, transforms, viewBox
mapping, gradients, markers, and external resources remain later issue #40
promotion work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-203.md`.

The completed native-engine-browser-204 slice adds bounded straight SVG path
support for absolute/relative `M`, `L`, `H`, `V`, and `Z` commands. Parsed
subpaths feed layout bounds, typed `SvgPathFill`/`SvgPathStroke` commands,
even-odd fill, segment-distance stroke coverage, clipping, scroll translation,
alpha composition, and hit-test ownership. Curves, arcs, dash arrays,
explicit cap/join styles, transforms, viewBox mapping, gradients, markers, and
external resources remain later issue #40 promotion work. Exact evidence is
recorded in `docs/plan/tasks/native-engine-browser-204.md`.

The completed native-engine-browser-205 slice adds bounded quadratic and cubic
SVG path curves through absolute/relative `Q` and `C` commands. Fixed-count
curve flattening feeds shared `NativeSvgSubpath` layout bounds, typed path
fill/stroke commands, polygon/segment rasterization, clipping, scroll
translation, alpha composition, capture, and hit-test ownership. Smooth or
reflected commands, elliptical arcs, dash arrays, explicit cap/join styles,
transforms, viewBox mapping, gradients, markers, and external resources remain
later issue #40 promotion work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-205.md`.

The completed native-engine-browser-206 slice adds bounded smooth/reflected
`S`/`T` and elliptical-arc `A` SVG path commands. The evaluator reflects only
the control point allowed by the preceding command, converts arcs through the
SVG endpoint-to-center algorithm, and flattens them into the existing bounded
`NativeSvgSubpath` geometry. Finite-number checks, binary arc-flag validation,
at most 64 samples per arc, and the 2,048-point budget fail closed before
partial display geometry is published. The existing layout, typed path
fill/stroke, clipping, scroll, alpha, capture, and hit-test consumers remain
the single geometry path. Adaptive flattening, viewBox mapping, dash arrays,
explicit cap/join styles, gradients, markers, and external resources remain
later issue #40 promotion work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-206.md`.

The completed native-engine-browser-207 slice adds bounded affine SVG
transforms. `matrix`, `translate`, `scale`, `rotate`, `skewX`, and `skewY`
lists on SVG roots, groups, and supported shapes compose ancestor matrices in
the shared layout owner. Transformed rect/circle/ellipse/line/polyline/polygon
points and path subpaths feed the same typed fill/stroke display commands,
software rasterizer, clipping, scroll projection, capture, and hit-test
owners; identity transforms retain the pre-transform command forms. Malformed,
non-finite, unknown, and over-limit output fails closed. CSS transforms,
dash arrays, explicit cap/join styles,
gradients, markers, and external resources remain later issue #40 promotion
work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-207.md`.

The completed native-engine-browser-208 slice adds bounded SVG `viewBox` and
`preserveAspectRatio` mapping to the same affine transform owner. The default
`xMidYMid meet`, nine bounded alignments, `meet`, `slice`, and nonuniform
`none` modes convert user-space coordinates into the declared viewport before
the shared integer geometry path. Rect/circle/ellipse/line/polyline/polygon
points and path subpaths retain one layout, typed fill/stroke display-list,
software-raster, clipping, scroll, capture, and hit-test interpretation.
Malformed viewBox/viewport data fails closed. CSS sizing/percentages, nested
viewport placement, dash arrays, explicit cap/join styles,
gradients, markers, and external resources remain later issue #40 promotion
work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-208.md`.

The completed native-engine-browser-209 slice adds SVG viewport clipping to
the shared rectangle clip owner. Each supported descendant now receives the
intersection of its laid-out SVG ancestor viewports and any CSS overflow
clips; the combined document-space clip feeds projected bounds, typed display
commands, software replay, root-scroll translation, capture, and hit testing.
Layout bounds remain unchanged, and shapes outside the viewport cannot paint or
win shape hit ownership. Rounded clip paths, nested viewport placement,
clip-path/mask semantics, dash arrays, explicit cap/join styles, gradients,
markers, and external resources remain later issue #40 promotion work. Exact
evidence is recorded in
`docs/plan/tasks/native-engine-browser-209.md`.

The completed native-engine-browser-210 slice adds bounded inline PNG data-URL
image replay to the shared native artifact path. Validated image elements now
use intrinsic/aspect-ratio sizing and carry decoded RGBA pixels through typed
display-list commands, nearest-neighbor software rasterization, source-over
alpha, clipping, scrolling, hit testing, and PNG capture. External image
resources, transfer/cache ownership, one CSS background-image URL layer, SVG image resources,
and animated formats remain active browser-completeness work. Exact evidence is
recorded in
`docs/plan/tasks/native-engine-browser-210.md`.

The completed native-engine-browser-211 slice adds bounded external PNG image
resources through the content-owner boundary. Static external image sources
now use the shared HTTP(S) loader with credential, redirect, referrer, cookie,
mixed-content, and CSP `img-src` policy; successful PNG responses are decoded
to bounded RGBA pixels and transferred with their source and intrinsic
dimensions through the typed document wire. Parent layout, display-list,
software raster, clipping, scrolling, hit testing, capture, and successful
image load events consume the same resource owner; denied, malformed, or
failed images are non-fatal broken-image results. Decoded caching, responsive
sources, additional CSS image layers beyond the single background-image URL,
SVG image resources, animation, and additional formats remain
later issue #40 work. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-211.md`.

The completed native-engine-browser-212 slice extends external PNG resources
through script mutation. The content owner rewalks the current static image
set after a script command batch, keeps resources whose node/source identity
is unchanged, fetches new or changed HTTP(S) sources through the existing
security policy, and publishes successful decoded RGBA pixels with the
mutation snapshot. Image `load` handlers run in the persistent page realm
before the snapshot is returned; broken resources do not abort the document.
Decoded caching, recursive handler loads, responsive sources, additional CSS
image layers beyond the single background-image URL, SVG image resources,
animation, and additional formats remain later issue #40 work.
Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-212.md`.

The completed native-engine-browser-213 slice adds bounded decoded PNG caching
to the shared external-image loader. Cacheable successful responses are keyed
by requested and final redirect URLs, while `no-store`, `no-cache`, stale
zero-age, privacy-sensitive `Vary`, and `Set-Cookie` responses are excluded.
Current document security policy is still evaluated before lookup, and the
same cache serves initial and script-reactive image discovery. The cache is
process-lifetime, bounded by the existing resource-entry limit, and does not
persist pixels to profiles; freshness/revalidation, concurrent coalescing,
responsive sources, additional CSS image layers beyond the single
background-image URL, SVG image resources, animation, and other formats
remain active issue #40 work. Exact evidence is in
`docs/plan/tasks/native-engine-browser-213.md`.

The completed native-engine-browser-214 slice adds shared URL-reflected DOM
properties. Anchors, areas, bases, and links resolve `href`; image, script,
frame, embed, source, track, audio, and video elements resolve `src`; and forms
resolve `action` against the active document or same-origin frame URL. Setters
stringify and persist the author attribute through the existing typed command
bridge, so image `src` writes use the native resource path. Exact evidence is
in `docs/plan/tasks/native-engine-browser-214.md`.

The completed native-engine-browser-215 slice adds one native CSS
`background-image` URL layer. Inline PNG data URLs and external PNG resources
share bounded source identity, HTTP/CSP/mixed-content/referrer/cookie policy,
decoded caching, typed document-wire validation, software paint, clipping,
scrolling, capture, and script-driven style mutation. CSS repeat/position/size,
multiple layers, gradients, masks, filters, responsive selection, SVG image
resources, animation, and additional formats remain separate issue #40 work.
Exact evidence is in `docs/plan/tasks/native-engine-browser-215.md`.

The completed native-engine-browser-196 slice closes the bounded attribute-node
Web IDL surface. `document.createAttribute()` creates persistent `Attr`
objects, element attribute-node methods preserve ownership and replacement
identity, and `element.attributes` exposes a live indexed/iterable
`NamedNodeMap`. The shared host surface is installed for local elements,
content-worker projections, and same-origin frame elements; at that checkpoint
unsupported namespace-qualified attributes failed explicitly. Exact evidence
is recorded in `docs/plan/tasks/native-engine-browser-196.md`. Namespace-aware
storage is now covered by slice 201; complete Web IDL descriptor parity, XML
documents, and browser-wide conformance remain issue #40 work.

The completed native-engine-browser-195 slice advances the document tree
builder's recovery contract. Rust and detached JavaScript parsing now recover
unterminated comments, bogus declarations, and EOF-terminated tags without
publishing partial elements; first duplicate HTML attributes win; duplicate or
late doctypes are ignored; and common paragraph/list/option/ruby/table implied
end tags close in the native owner. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-195.md`. Full WHATWG insertion modes,
foreign content, table foster parenting, and Web IDL/conformance promotion
remain issue #40 work.

The completed native-engine-browser-193 slice closes script-created comment
construction across local, HTTP(S) content-worker, same-origin frame, and
nested-frame realms. `document.createComment()` returns a bounded Comment node
with CharacterData accessors, insertion/removal, mutation, clone, serialization,
and persistent identity; detached `innerHTML` now materializes comments rather
than dropping them. Comment data remains outside visible text, layout, and
paint. Exact evidence is recorded in
`docs/plan/tasks/native-engine-browser-193.md`; document-type construction, full
malformed-comment recovery, and Web IDL/conformance promotion remain issue #40
work.

The completed native-engine-browser-192 slice preserves parsed HTML comments
and basic document-type metadata as real native nodes across local, HTTP(S)
content-worker, same-origin frame, and nested-frame projections. Comments use
the supported CharacterData mutation path but are excluded from visible text,
layout, and paint; doctypes expose `document.doctype`, node metadata, and
parentage. The typed document wire, persistent script snapshot, serializer,
and frame projection now carry these node kinds, while exact local evidence is
recorded in `docs/plan/tasks/native-engine-browser-192.md`. Complete malformed
HTML recovery, raw-text/foreign-content parsing, script-created comment/doctype
construction, and Web IDL conformance remain issue #40 promotion work.

The completed native-engine-browser-191 slice preserves script-created element
and text-node identity across separate evaluations in local, HTTP(S)
content-worker, and same-origin frame realms. The typed document wire and
persistent script snapshot now carry generation-scoped temporary-node
identities, while each JavaScript realm keeps the original wrappers and
rebinds them to the projected native arena nodes. Queries and later mutations
therefore retain `===` identity and native ownership after a host refresh, and
a failed typed transaction does not publish an updated mapping. Exact evidence
is recorded in `docs/plan/tasks/native-engine-browser-191.md`; ordinary
detached-node persistence, complete HTML tree-builder, and Web IDL conformance
remain issue #40 promotion work.

The completed native-engine-browser-190 slice adds generic DOM node identity
and normalization primitives across local, HTTP(S) content-worker, and
same-origin frame realms. `cloneNode()` creates bounded detached copies with
attributes and optional descendants; `isSameNode()` and `isEqualNode()` use
the host's stable object/tree projection; `compareDocumentPosition()` reports
same-tree order, containment, and disconnectedness; and `normalize()` removes
empty text nodes and merges adjacent text through the existing typed mutation
transaction. The Rust transaction recognizes current-batch script-created
nodes when detaching a child of a detached constructed tree, while ordinary
stale references retain the attached-node guard. Exact evidence is recorded
in `docs/plan/tasks/native-engine-browser-190.md`; complete HTML tree-builder
and Web IDL conformance remain issue #40 promotion work.

The completed native-engine-browser-189 slice adds structural DOM primitives
across local, HTTP(S) content-worker, and same-origin frame realms. The shared
tree projection now returns the owning document or detached tree from
`getRootNode()`, while attached elements serialize through live `outerHTML` and
replace themselves with bounded parsed element/text subtrees. The replacement
path reuses existing parent ownership, collection traversal, mutation command,
and host-refresh machinery; the temporary-node allocator is persistent across
bootstrap refreshes so reused wrappers cannot collide. Exact evidence is
recorded in `docs/plan/tasks/native-engine-browser-189.md`; complete HTML
tree-builder and Web IDL conformance remain issue #40 promotion work.

The completed native-engine-browser-188 slice makes document titles live across
local, HTTP(S) content-worker, and same-origin frame realms. The document
projection resolves `document.title` from the current title node, sends title
writes through the existing bounded text-mutation path, materializes a missing
head/title pair when possible, and sends a typed `setDocumentTitle` command to
the Rust owner when no HTML root can host the node. The Rust path validates the
title bound and creates the durable title node without adding a second DOM
model. Exact local evidence is recorded in
`docs/plan/tasks/native-engine-browser-188.md`; full Web IDL descriptors,
complete HTML tree-builder semantics, and the remaining issue #40 promotion
gates remain open.

The completed native-engine-browser-187 slice exposes live document-facing DOM
surfaces across local, HTTP(S) content-worker, and same-origin frame realms.
Documents provide head, forms, links, scripts, images, scrollingElement,
live tag/class collections, and getElementsByName through the shared tree
walker. Full Web IDL descriptors, comprehensive parser semantics, and the
remaining issue #40 promotion gates remain open.

The completed native-engine-browser-186 slice exposes the shared character-data
surface across local, HTTP(S) content-worker, and same-origin frame realms.
Text nodes now receive the `Text` → `CharacterData` → `Node` prototype chain,
keep `nodeValue`, `data`, and `textContent` synchronized, and implement the
bounded CharacterData mutation methods through the existing native command and
observer path. Full Web IDL descriptors, comprehensive parser semantics, and
the remaining issue #40 promotion gates remain open.

The completed native-engine-browser-185 slice hardens the shared bounded
fragment parser with quote-aware tag-end scanning, declaration/comment
skipping, and raw-text/RCDATA handling for `script`, `style`, `textarea`, and
`title`. Local, HTTP(S) content-worker, and same-origin frame construction
now keeps quoted `>` values and script/style source intact while decoding
RCDATA entities; full HTML tree-builder, foreign-content, and issue #40
promotion gates remain open.

The completed native-engine-browser-184 slice extends live DOM mutation to
element `textContent` and `innerText`. Local, HTTP(S) content-worker, and
same-origin frame setters now materialize one same-turn text child, maintain
live child/parent projections, and expose that child in `MutationObserver`
added-node payloads; suppressed preview commands leave Rust with one
authoritative `setTextContent` transaction. Full Web IDL and HTML semantics,
observer coalescing, and the remaining issue #40 promotion gates remain open.

The completed native-engine-browser-183 slice extends live DOM mutation to
element `innerHTML`. The bounded parser builds same-turn nested host children,
so selectors, text, serialization, and `MutationObserver` added-node payloads
observe the mutation immediately; a suppressed preview command scope keeps
the Rust owner on one authoritative `setInnerHtml` transaction. Local,
HTTP(S) content-worker, and same-origin frame paths are covered, while full
HTML parsing and the remaining issue #40 promotion gates remain open.

The completed native-engine-browser-182 slice extends reflected DOM properties
to common form and HTML controls. Boolean properties (`disabled`, `hidden`,
`multiple`, `required`, `readOnly`, and related flags) and common string
properties now use attribute-backed accessors with native command persistence;
`type` has bounded default/normalization behavior. Local, HTTP(S)
content-worker, and same-origin frame tests cover the same contract, while
complete Web IDL reflection and the remaining issue #40 promotion gates are
still open.

The completed native-engine-browser-181 slice installs reflected `id` and
`className` accessors on local and projected frame elements. Assignment and
attribute methods now share one attribute-backed view, and document queries
traverse the live attached tree so same-turn appended nodes are discoverable.
The implementation is shared by local and HTTP(S) content-worker realms and
covered by same-origin frame evidence; complete Web IDL reflection and the
remaining issue #40 promotion gates are still open.

This slice is not browser parity. It cannot be promoted or advertised as safe
for arbitrary remote content until CSS/layout/paint, security policy, process
isolation, cancellation, conformance, and platform evidence exist.

Future phases may split the DOM parser into tokenizer/tree-builder modules and
add general CSS, nested/scrolling/stacking layout, paint, full event-loop, script,
complete storage semantics, and additional process boundaries. Those changes require updates to this
document, the epic, and their dependency-ordered task files before
implementation.
