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
