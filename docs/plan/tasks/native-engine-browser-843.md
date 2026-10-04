---
id: native-engine-browser-843
scope: glass-browser/native-engine/content-process-cookie-authority
status: in-progress
depends-on: [native-engine-browser-842]
---

# Glass native-engine browser slice 843: parent-owned content-process cookies

## Objective

Remove the content process's authoritative cookie mirror and cookie-profile
persistence. Route cookie-bearing page/worker network operations through the
browser parent, which owns matching, response-cookie acceptance, and durable
profile updates, while preserving the script-visible `document.cookie`
projection and existing native Fetch behavior.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-842.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [HTML Standard `document.cookie`](https://html.spec.whatwg.org/multipage/dom.html#dom-document-cookie)

## Contract

- The browser parent is the sole authority for complete per-context cookie
  state, including request matching, `Set-Cookie` acceptance/rejection,
  expiration/deletion, profile persistence, and shared-profile journal
  publication. A content process or worker realm is not a cookie authority or
  durable writer.
- Do not send a complete cookie profile, HttpOnly value, raw `Cookie` or
  `Set-Cookie` header, or cookie-bearing profile path into the content process.
  Remove content-process cookie-profile load/save and persistence paths.
- Give each document only a URL-scoped `document.cookie` projection of
  script-visible non-HttpOnly cookies. A setter returns a bounded typed write
  carrying exact context, frame, document generation, and source URL; the
  parent validates and applies it, then refreshes the projection. Synchronous
  same-turn preview does not confer authority on the child.
- Broker cookie-bearing page and worker network requests through the parent
  and its existing network policy. Preserve `include`, `same-origin`, and
  `omit` at every redirect hop, CORS/referrer policy, response streaming,
  cancellation, and response-cookie update/deletion. Broker errors are
  explicit; no direct child-loader fallback is allowed.
- Commit response-cookie mutations once in the parent before later requests
  can observe them. `Set-Cookie` remains unreadable to script. Cookie import,
  clear, restart, same-profile live-session synchronization, and separate
  profile isolation continue to work through the parent authority.
- Validate every child request/write against its captured owner identity.
  Stale owners cannot redirect to the selected target. IPC and queues remain
  bounded; overflow, process exit, cancellation, and protocol errors surface
  typed failures.
- Preserve the currently supported native cookie attributes and matching
  behavior. Do not claim full Cookie/Web IDL or WPT conformance beyond selected
  passing cases.

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-843.md`
- `CHANGELOG.md`

## Verification

- Process-protocol tests prove no full profile, HttpOnly value, raw cookie
  header, or cookie-bearing profile path crosses into the child; setter writes
  are bounded and owner-tagged.
- Process-backed tests verify the `document.cookie` projection and setter,
  HttpOnly invisibility, worker/page credentials modes, redirect hops,
  response-cookie update/deletion, import/clear, restart, same-profile
  synchronization, and separate-profile isolation.
- Initial-load inline-script-created HTTP(S) classic/module scripts must use
  the parent broker; verify parent-selected HttpOnly request cookies,
  response-cookie updates, and the script-visible projection.
- HTTP(S) stylesheets, CSS imports/fonts, images, and media created by user
  action mutations must use the parent broker and preserve response-cookie
  ordering without a child-loader retry.
- Verify a child shutdown/restart cannot mutate or lose the parent jar and
  that parent-broker failure cannot trigger a direct child request.
- Run selected Fetch/Cookie/SharedWorker WPT cases and record exact selections
  and deviations; mocked-loader checks do not replace network regressions.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
- Build `glass-native-content-worker` explicitly after the check and before
  process-backed regressions; the tests spawn this companion executable, which
  `cargo check` does not rebuild:
  `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked --quiet`
- Run focused process-backed cookie tests after the check; reserve workspace
  and all-feature validation for the final issue #40 gate.
- `cargo fmt --all -- --check`
- `git diff --check`
- Run relevant documentation inventory/coverage checks.

## Current Evidence

- `cargo check -p glass-browser --features native-engine --lib --test native_engine --locked --quiet` passed with existing dead-code warnings only.
- `NativeEngine::cookies_async` now reads the parent loader after synchronizing
  external profile events, overlaid with queued child cookie changes that the
  parent has already collected. The content-process cookie-list IPC command
  and its full-profile response decoder are removed. The process-backed HTTP
  regression passes against the parent import/clear path (1 passed;
  58.03-second test runtime). Initial runs exposed and fixed the missing parent
  overlay for response `Set-Cookie` and stale queued changes after clear.
  The parent process now applies child-reported cookie mutations to the
  durable profile, and child content snapshots no longer apply their cookie
  changes there. Cookie persistence is now split into a parent-managed
  `<profile>.cookies` sidecar; before child startup the parent migrates legacy
  cookies out of the shared Web Storage file. The content loader starts with
  an empty jar instead of loading from that path, and child profile writes keep
  its cookie field empty. At this checkpoint, `set_cookies` and related IPC
  still sent the full cookie profile into the child mirror, and direct child
  network paths remained. A later protocol-27 checkpoint removed full-cookie
  IPC; broader request brokering and child-journal removal remain outstanding.
  The restart persistence regression passed
  (1 passed; 93.27-second runtime); its test name now describes the observed
  process-restart behavior rather than attributing durable writes to the child.
  Cookie import and clear now commit through the parent profile writer; the
  content process only refreshes its mirror and discards local change records.
  Clear reconciles pending child changes before emitting parent deletion
  records, so the next API read cannot restore cookies from the old queue.
- The unhandled-network fallback for the public host
  `NativeEngine::fetch_async` command now sends requests through a parent broker,
  persists response-cookie updates in the parent, and then refreshes the active
  child's runtime projection. Content-side Service Worker interception remains
  in place. Its process-backed HTTP test
  verifies initial and updated HttpOnly cookies are sent only on the requests
  selected by the parent, while `document.cookie` sees the updated non-HttpOnly
  projection (1 passed; 27.17-second test runtime). The parent-brokered
  cross-origin CORS case passed (1 passed; 24.30 seconds), and the separate
  SharedWorker live-cookie case passed (1 passed; 43.44 seconds). The adjacent
  child-network credentials/redirect test had passed previously (1 passed;
  24.64 seconds), but two no-build reruns against the current binary timed out
  while awaiting a script turn at `tests/native_engine.rs:33317` (51.04 and
  29.74 seconds), before assertions. Worker Fetch outside the
  dedicated-worker message path described below and other child-originated
  network operations remain outstanding for the parent broker.
- An eligible HTTP(S) Fetch created during an explicit page script evaluation
  now uses the parent broker after content-side Service Worker interception
  declines it. The parent validates the committed owner and applies the
  request method, body, headers, credentials mode, CORS/redirect/cache modes,
  timeout, and referrer policy. Its owner identity includes the active context,
  frame, content-document generation, and document URL. Bounded cookie setter
  records carry that same owner; the parent validates and applies them before
  selecting request cookies, then commits response-cookie changes before
  returning. It sends back only the matching URL's script-visible
  `document.cookie` projection; response `Set-Cookie` and HttpOnly values do
  not cross to the child on this path. Each explicit page-script turn now
  starts with the parent-computed URL-scoped projection; bounded owner-tagged
  setter writes are returned even when that turn makes no Fetch, then applied
  by the parent. `cookies_async` reads that parent jar, and a process-backed
  regression verifies a standalone setter is included on the next brokered
  request (1 passed; 30.12-second test runtime). The parent projection is
  retained across content turns rather than overwritten from the stale child
  mirror. The
  process-backed regression passed (1 passed; 23.46 seconds) and checks a
  same-turn setter before Fetch, initial and updated HttpOnly request cookies,
  the visible projection, and the next request.
  This does not cover Fetch during initial document loading,
  explicit-turn page/dedicated-worker upload streams (covered by later
  checkpoints below), Service Worker-originated upload streams,
  module/font destinations, Dedicated/SharedWorker module-graph process coverage, autonomous worker events,
  independently delivered SharedWorker messages, or Service Worker internal
  network requests, which still use the child loader. A later checkpoint below
  now routes worker entry and classic imported scripts through the parent;
  module-graph coverage remains outstanding.
  The broker currently buffers the bounded response before resolving Fetch;
  incremental network response streaming/backpressure remains outstanding.
- After protocol version 23 added the parent media-resource path, the scoped
  `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed with existing dead-code warnings only.
  The extended process-backed
  `native_content_process_http_navigation_uses_parent_cookie_authority` test
  passed (1 passed, 897 filtered; 25.76 seconds), verifying media response
  cookies remain parent-owned and the visible projection excludes HttpOnly.
- `cargo fmt --all -- --check`, `git diff --check`, release-documentation
  truth, documentation depth, and TUI shortcut inventory checks pass. The
  latest static doc reports cover 1,471 Markdown files with zero current-claim
  failures, 93 current guides and 19 contracts, and 15 implementation shortcut
  keys/63 documentation markers. After rebuilding the stale `glass` debug
  binary, the MCP schema scoreboard measured 346 tools and 173,741 UTF-8 bytes
  (43,436 estimated tokens); `docs/mcp-schema-budget.md` now records that live
  measurement, and the documentation coverage check passes.
- Standard buffered Fetch emitted during dedicated-worker initialization and
  while handling an explicit page `Worker.postMessage` now uses the parent
  broker. The protocol validates the captured page owner separately from the
  worker request URL, applies pending page cookie writes before selecting
  request cookies, commits response-cookie changes in the parent, and returns
  the page owner's visible cookie projection. The existing worker
  response-stream interface is preserved over the buffered parent response.
  At that checkpoint locally hosted SharedWorker creation/connect evaluations
  also received the broker but lacked a focused process-backed regression;
  the later SharedWorker regression is recorded below. The process-backed
  `native_content_process_worker_message_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 24.15-second test runtime), checking a same-turn
  page setter on the startup Fetch, the parent Set-Cookie update on the later
  message Fetch, response delivery, and distinct page/worker URLs. At this
  checkpoint, worker script/resource/module-graph loading, autonomous turns,
  independently delivered SharedWorker events, module/font destinations,
  initial/resource loading, and Service Worker internal requests remained
  outside the verified broker path. Dedicated-worker upload streams were not
  covered at that checkpoint; see the subsequent upload-stream checkpoint
  below.
- Page-to-dedicated-worker events delivered through a transferred `MessagePort`
  now retain the parent Fetch broker through worker callback evaluation. The
  expanded `native_content_process_worker_message_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 35.17-second test runtime): its MessagePort
  callback Fetch receives the prior parent `Set-Cookie` update, the HttpOnly
  cookie, and a same-turn page cookie write. Other independently delivered
  worker events, autonomous turns, and SharedWorker MessagePort callbacks are
  not thereby verified as parent-brokered.
- An awaited ordinary Fetch issued by a controlled Service Worker while
  handling an explicit page Fetch now uses the same parent broker. The existing
  `native_content_process_page_fetch_credentials_survive_service_worker_handoff`
  process-backed regression passed (1 passed; 33.30-second runtime), covering
  cross-origin credentials/CORS behavior and response-cookie visibility on a
  later request. At that checkpoint, Service Worker lifetime/background
  Fetches, Service Worker-originated upload streams, and out-of-band Service
  Worker events remained outside this broker path; a later checkpoint below
  adds parent brokering for non-awaited Fetches created by a controlled
  FetchEvent.
- Ordinary explicit page Fetches with a `ReadableStream` request body now
  collect the bounded body (up to `MAX_NATIVE_FORM_BODY_BYTES`) before using
  the parent broker. `native_content_process_commits_page_work_from_stream_upload_pull`
  passed (1 passed; 22.37-second runtime), verifying the body, pull-callback
  effects, and an HttpOnly cookie set by the upload response on a same-turn
  follow-up Fetch. This preserves bounded memory and parent cookie authority,
  but gives up socket-level upload streaming/backpressure for this path; the
  network request begins after the source closes. Service Worker-originated
  upload streams and dedicated-worker uploads from autonomous event turns are
  not covered by this change.
- Dedicated-worker Fetch `ReadableStream` uploads initiated during an explicit
  page-script turn now collect under `MAX_NATIVE_FORM_BODY_BYTES` and use the
  parent Fetch broker. The parent broker remains attached while upload-pull
  callbacks and the buffered response stream are drained, so worker Fetches
  following response-body consumption stay in the parent-owned cookie order.
  `native_content_process_worker_stream_upload_uses_parent_cookie_authority`
  passed (1 passed; 26.58-second runtime), covering a parent-only page setter,
  the upload request cookie, an HttpOnly response cookie, a same-turn follow-up
  worker Fetch, and the public parent cookie view. As on page uploads, HTTP
  starts only after the source closes, so socket-level upload streaming and
  backpressure are lost. Service Worker-originated upload streams and
  autonomous worker turns remain outside this broker path.
- Page-originated Fetch `ReadableStream` uploads to a controlled Service Worker
  are collected under the existing body limit, offered to the Service Worker,
  and sent through the parent broker only if the handler declines them. A
  missing parent broker fails explicitly; network requests never fall back to
  the child loader. The broker applies the page's owner-tagged cookie write
  before selecting upload cookies and commits the response's HttpOnly
  `Set-Cookie` before a same-turn follow-up request.
  `native_content_process_service_worker_replays_cloned_request_body`
  passed (1 passed; 27.81-second runtime), preserving the handled upload
  response and verifying both cookie directions on the network fallback. The
  upload remains buffered rather than socket-streamed. The declining FetchEvent
  path also exposed that the Service Worker dispatcher returned a plain object
  when `respondWith()` was not called even though its host always awaited a
  promise; it now returns a resolved promise, allowing the existing network
  fallback to run. Service Worker-originated upload streams, background work
  outside intercepted FetchEvents, and autonomous worker turns remain outside
  this broker path.
- Slice 842 is complete: browser-coordinated SharedWorker creation derives
  cookies from the exact parent source owner. Its parent snapshot/override/
  deletion process regression passed (1 passed; 48.31-second test runtime),
  and the independent static review found no lock/order defect.
- The broader content-process mirror, child profile-path/read/write capability,
  and complete parent brokering for every cookie-bearing request class remain
  outstanding. The process-backed
  regression `native_content_process_shared_worker_fetch_credentials_modes_follow_redirects`
  passed against the latest built integration binary (1 passed; 24.64-second
  runtime). It covers credentials/CORS/redirect and invalid-mode behavior, but
  does not verify the parent-brokered dedicated-worker route; this SharedWorker
  Fetch path still performs its request directly in the content process.
- At this earlier checkpoint, a browser-coordinated due DedicatedWorker timer
  no longer executed in the child idle loop. It notified the exact
  context/frame owner, which ran the timer on its next script turn with the
  parent Fetch broker; standalone `NativeEngine` still retained local timer
  dispatch. The latest checkpoint below removes that standalone exception.
  The process-backed unit
  regression `browser_owned_worker_timer_fetch_uses_parent_cookie_authority`
  passed (1 passed; 22.87 seconds), verifying cookies on the worker entry
  request, a classic `importScripts()` dependency request, HttpOnly cookies set
  by both script responses and sent on later requests, a parent-accepted
  HttpOnly response update on the next worker request, the parent cookie API,
  and the script-visible projection. The fixture's initial 20-second accept
  timeout proved too tight; the final
  45-second bound passed. HTTP(S) worker entry scripts, classic
  `importScripts()` dependencies, and static/dynamic worker module dependencies
  now use the parent loader with exact captured page-owner validation and a
  bounded set of parent-observed worker script/referrer URLs. The child receives
  source, final URL, response Referrer-Policy, and the script-visible projection
  only; raw cookie headers and the complete jar are not part of this broker
  response. Non-network worker sources remain local. The new process test
  directly covers the entry script, one classic imported script, and later
  DedicatedWorker timer Fetch, not module-graph variants. Full child-profile
  removal, page-resource broker coverage, ServiceWorker timer/lifetime requests,
  and other unbrokered network classes remain open at this checkpoint.
- HTTP(S) dynamic page-script loads discovered during an explicit,
  parent-brokered page script turn now use the parent loader. This includes
  dynamically attached classic/module script elements, static module graph
  dependencies, and runtime `import()` requests; the parent validates the page
  owner and bounded parent-observed script/referrer URLs, applies setter writes
  before the request, and returns only bounded source, final URL, response
  Referrer-Policy, and the script-visible cookie projection. Blob, file, and
  other non-network sources retain the local loader. The process-backed
  `native_content_process_dynamic_page_module_uses_parent_cookie_authority`
  regression passed (1 passed; 23.24 seconds), proving the page's ordinary and
  HttpOnly cookies reach a dynamically attached module, the module's HttpOnly
  `Set-Cookie` reaches its static dependency, a same-turn setter reaches the
  parent request, and HttpOnly remains absent from `document.cookie`. The
  existing `native_content_process_uses_dynamic_import_map_for_a_later_module_root`
  regression also passed (1 passed; 20.92 seconds). Runtime `import()` does
  not yet have a cookie-specific regression. The existing
  `native_content_process_runs_nested_dynamic_external_scripts` regression
  passed (1 passed; 19.89 seconds), covering the dynamically attached classic
  child script branch. At that checkpoint, parser-discovered scripts during
  initial page loading, page stylesheets/images/fonts/media, and other
  unbrokered requests still used the child loader; the complete content-process
  cookie profile and persistence paths also remained. The later navigation and
  stylesheet checkpoints below supersede the parser-script and initial
  stylesheet portions of that status.
- Unhandled HTTP(S) top-level navigation now requests its document from the
  parent loader after content-side Service Worker interception declines it.
  The parent validates the load ID, document generation, context, frame, and
  requested URL before selecting navigation cookies and accepting response
  cookies. The child receives only a bounded decoded document, the selected
  response policy headers, and that final URL's script-visible
  `document.cookie` projection; neither `Cookie`/`Set-Cookie` headers nor
  HttpOnly values are part of the response. The worker protocol is version 20.
  The added process-backed `native_content_process_http_navigation_uses_parent_cookie_authority`
  regression passed (1 passed; 20.40 seconds), verifying that the initial
  page script and `document.cookie` see only the non-HttpOnly cookie while the
  parent cookie API retains both response cookies. The existing
  `native_runtime_service_worker_fetch_open_window_resumes_navigation`
  regression also passed (1 passed; 34.25 seconds) on an explicit 4 MiB test
  thread; the harness's default 2 MiB stack overflows on this nested navigation
  path. Parser-discovered HTTP(S) classic and module scripts, including static
  module dependencies, now use the parent page-script broker during initial
  loading as well as explicit script turns. The parent binds these requests to
  the active owner or captured load generation/context/frame and, for a
  parent-loaded navigation, its observed final document URL. After each script
  response it returns only the URL-scoped visible cookie projection; the
  process-backed regression was extended and passed (1 passed; 21.17 seconds),
  checking request-cookie selection, response-cookie acceptance, and HttpOnly
  invisibility during parser execution. Protocol version 21 added the parent
  path for initial HTTP(S) stylesheet links and static CSS imports, retaining
  the parent's stylesheet loader and a 512 KiB response bound. Protocol
  version 22 adds parent-brokered page images and transfers the child's applied
  meta-CSP source list to the parent before page-owned resource loads and
  Fetches. The process regression passed (1 test; 24.90 seconds), covering
  ordered cookie updates across navigation, stylesheet, CSS import, image, and
  classic parser-script requests, and proving `img-src 'self'` blocks the
  cross-origin image before network access. The initial module graph still
  lacks a dedicated cookie regression. Protocol version 23 adds parent-owned
  HTTP(S) page-media loads; only bounded media metadata returns to the child,
  while request and response cookies stay in the parent. The same process test
  passed after extension (1 passed; 25.76 seconds), verifying the media
  response's visible and HttpOnly cookies reach the later parser request while
  only the visible value enters the script projection. Protocol version 24
  routes initial and dynamically recascaded CSS `@font-face` loads and explicit
  page `FontFace` destination requests through the parent font loader. The
  scoped check passed with the existing 69 dead-code warnings. The test target
  rebuilt in 3m54; the extended process regression passed (1 passed; 897
  filtered; 32.16 seconds): the CSS font response's
  cookies reach the following image request, the `FontFace.load()` path exposes
  only the visible cookie to script, and a subsequent Fetch carries both
  cookies set by that font response. Font requests outside an active
  parent-brokered turn, dynamic stylesheet loads outside an active
  parent-brokered page-script turn, Service-Worker-handled navigation
  responses, and the full child profile/read/write path remain outstanding.
  Protocol version 25 extends captured-load owner validation to waiting-worker
  navigation activation, top-level navigation preload, and navigation
  FetchEvent requests. The process-backed
  `native_content_process_waiting_service_worker_navigation_uses_parent_cookie_authority`
  regression passed (1 passed; 28.78 seconds), verifying ordered parent cookie
  updates through activation, preload, navigation Fetch, and the next page
  Fetch, with HttpOnly values filtered from `document.cookie`.
  HTTP(S) dynamic stylesheet links and their CSS imports created during an
  explicit parent-brokered page-script turn now reuse the existing parent
  stylesheet protocol. `native_content_process_applies_stylesheet_link_referrer_policy`
  passed (1 passed; 897 filtered; 29.43 seconds), retaining cache/referrer and
  cascade assertions while proving the parent
  selects an initial HttpOnly page cookie for the dynamic stylesheet, accepts
  its HttpOnly response cookie for the imported stylesheet and later Fetch,
  and keeps both secrets out of `document.cookie`.
  The initial test-target rebuild took 3m02. Its first run reached the final
  computed-style assertion after the new cookie checks passed, and differed
  only because this engine serializes the `bold` keyword as `"bold"`, not
  `"700"`; the test-only correction rebuilt in 1m08 and the focused rerun
  passed.
- HTTP(S) Service Worker registration and `update()` entry scripts now use
  the parent worker-script broker during parser-time registration and explicit
  page-script turns. Their classic `importScripts()` dependencies and static
  module graph dependencies use that same parent loader; cookie writes are
  applied before each request and response cookies remain in the parent. The
  process-backed
  `native_content_process_service_worker_module_registration_uses_parent_cookie_authority`
  regression passed (1 passed; 24.85 seconds), verifying HttpOnly cookies from
  the page, Service Worker module entry, and imported module reach registration
  and update requests, while the page's `document.cookie` projection stays
  empty. The existing
  `native_content_process_service_worker_registration_uses_live_document_referrer_policy`
  regression passed (1 passed; 26.09 seconds) after `history_sync` began
  advancing the parent's active owner URL only after successful synchronization;
  this preserves live `history.pushState` referrers and prevents a subsequent
  script turn from being rejected against a stale parent URL. Service Worker
  restoration now uses that broker for persisted entry and static dependency
  loads during matching-document initialization. The new process-backed
  `native_content_process_service_worker_restoration_uses_parent_cookie_authority`
  regression passed (1 passed; 33.09 seconds), verifying that restoration
  entry and dependency response cookies are available to the following
  parent-owned navigation while all remain HttpOnly to the page. The scoped
  `cargo check -p glass-browser --features native-engine --tests --locked --quiet`
  passed. Browser-coordinated Service Worker timer callbacks now defer to the
  exact page owner's script turn and execute with its parent Fetch broker. The
  process-backed `browser_owned_service_worker_timer_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 22.89 seconds), proving the first Fetch receives
  the parent script cookie and pre-rotation HttpOnly cookie, then the next
  Fetch receives the parent's accepted HttpOnly rotation; both secrets remain
  hidden from `document.cookie`. The combined
  `cargo test -p glass-browser --features native-engine --lib --locked --quiet parent_cookie_authority`
  run passed both DedicatedWorker and Service Worker timer regressions (2
  passed; 35.36 seconds). These process-backed unit tests launch the separate
  `glass-native-content-worker` executable, so rebuild it first with
  `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked`;
  otherwise an existing stale executable can make the test exercise old child
  code. Non-awaited ordinary Fetches created by a controlled FetchEvent now
  queue to the parent and run on the next exact-owner turn, leaving the
  independent `respondWith()` response unblocked. The process-backed
  `browser_owned_service_worker_lifetime_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 28.49 seconds), verifying parent cookie
  selection, HttpOnly rotation, and the next page Fetch. Service
  Worker-originated ReadableStream Fetch uploads during a parent-brokered
  FetchEvent now collect through the existing demand protocol under the
  `MAX_NATIVE_FORM_BODY_BYTES` and chunk-count limits before being sent through
  the parent broker. The new process-backed
  `native_content_process_service_worker_stream_upload_uses_parent_cookie_authority`
  regression verifies the HttpOnly seed and same-turn page cookie on the upload,
  the parent-accepted HttpOnly response rotation on the following request, and
  that the page projection excludes both HttpOnly values. This path buffers the
  upload before network dispatch, so it does not retain socket-level upload
  backpressure; standalone no-parent Service Worker uploads keep their existing
  direct streaming path. Background work outside intercepted FetchEvents and
  other internal network paths remain outside this checkpoint. The scoped
  `cargo check -p glass-browser --features native-engine --tests --locked
  --quiet` and companion worker binary build passed with existing dead-code
  warnings; the focused process regression passed (1 passed; 24.52 seconds).
  Formatting and diff checks pass. Documentation gates pass: 1,471 Markdown
  files with zero current-claim failures, 93 current guides and 19 contracts,
  15 implementation shortcut keys/63 markers, and coverage of 346 MCP tools,
  17 examples, and 22 public modules.
- Parent-brokered Service Worker registration/update now keeps the broker
  through worker source evaluation and `install`/`activate` event settlement.
  The first process regression exposed that the parent accepts worker-script
  loads during an in-flight navigation but rejects ordinary lifecycle Fetches
  without a resource-load marker. Such Fetches now carry an explicit
  captured-load marker, and the parent still validates the exact current or
  in-flight document owner before admitting them; ordinary FetchEvent paths
  do not set the marker. The process-backed
  `native_content_process_service_worker_lifecycle_fetches_use_parent_cookie_authority`
  regression passes (1 passed; 24.42 seconds), verifying the same-turn page
  cookie on the worker script and lifecycle Fetches, the install response's
  HttpOnly cookie on the activate Fetch, parent retention of both lifecycle
  cookies for the following page request, and their invisibility in
  `document.cookie`. The scoped check and rebuilt companion worker pass with
  existing dead-code warnings. Navigation-time activation of an already-waiting
  worker and background work outside intercepted FetchEvents remain direct.
- Navigation-time activation of an already-waiting worker now retains the
  parent Fetch broker through its `activate` settlement. The same captured-load
  broker covers Service Worker navigation preload and the navigation FetchEvent;
  preload requests preserve top-level navigation cookie policy and are bounded
  by the configured document/body limit. The parent accepts these requests
  only after checking the captured-load marker and exact active/in-flight
  context, frame, generation, and URL. Ordinary page Fetch paths continue to
  omit the captured-load marker. The process-backed
  `native_content_process_waiting_service_worker_navigation_uses_parent_cookie_authority`
  regression passed (1 passed; 28.78 seconds), exercising persisted active and
  waiting worker restoration, page-cookie flush, waiting-worker activation,
  navigation preload Set-Cookie, navigation FetchEvent Set-Cookie, and the next
  page Fetch. All HttpOnly values remained unavailable through
  `document.cookie`. The scoped test-target check passed with existing DOM
  dead-code warnings. Background work outside intercepted FetchEvents and
  other internal network paths remain direct child requests.
- The navigation-preload API process regression exposed two coupled failures:
  repeated page-bootstrap injection recreated the manager brand WeakMaps, and a
  fast `respondWith()` could drop a preload after the parent had started its
  brokered request, leaving the late `fetched` reply to be decoded as a new
  content command. The maps now persist across injection. Protocol 26 adds a
  bounded, captured-operation/Fetch-ID cancellation handshake: the parent
  cancels unfinished network work; if its response already won, the child
  drains that exact reply and the cancellation acknowledgement. The process
  regression
  `native_service_worker_navigation_preload_sends_header_and_reuses_response`
  passes (1 passed; 44.94 seconds), including preload header/referrer behavior,
  response reuse, the fast-response race, and a subsequent navigation. The
  `native_content_process_waiting_service_worker_navigation_uses_parent_cookie_authority`
  regression also passes on the rebuilt worker (1 passed; 28.42 seconds),
  retaining parent-owned HttpOnly cookie flow through waiting-worker
  activation, preload, navigation FetchEvent, and the following page request.
  The scoped native-engine test-target check and formatting pass with the
  repository's existing dead-code warnings.
- The Slice 842 HTTP regression successfully bound a local listener. The
  earlier blanket claim that local TCP binding is denied is stale for this
  checkout; that result does not prove parent-brokered network transport.
- Native profile persistence now stores cookies in a dedicated sidecar and
  keeps cookie values out of the shared Web Storage profile. The scoped
  cookie-profile unit tests passed (3 passed), and the process-backed
  `native_cookie_profile_survives_native_process_restart` regression passed
  (1 passed; 32.51 seconds) with a freshly rebuilt content worker. This is a
  disk/profile boundary only: complete cookie profiles and changes are still
  sent to the child over IPC, and remaining direct child network paths still
  need to be brought under parent ownership.
- Content-worker protocol 27 removes the parent-to-child full-profile set,
  change, and clear commands. The parent now sends only an owner-checked,
  URL-scoped script-visible cookie projection; page setter lines return to the
  parent without mutating the child loader. Child-direct internal requests can
  still produce cookie-change journals, so this narrows the authority boundary
  but does not finish it. The process-backed
  `native_content_process_synchronizes_document_cookie_with_http_session`
  regression passed (1 passed; 36.21 seconds), covering initial and refreshed
  projection, parent-applied script setters, hidden HttpOnly import, and clear.
  The scoped native-engine check and rebuilt companion worker also passed with
  existing DOM dead-code warnings.
- At the earlier DedicatedWorker timer checkpoint, browser-coordinated timers
  deferred every due callback,
  including nonzero deadlines, to the exact page-owner turn with its parent
  Fetch broker. The process regression now uses a 25 ms delay and passed
  (1 passed; 25.12 seconds), retaining HttpOnly filtering and parent-accepted
  response-cookie rotation. Standalone `NativeEngine` timer scheduling still
  used its local content-process timer path then; Slice 843's latest checkpoint
  below removes that standalone exception.
- At the protocol-28 checkpoint, page EventSource open, chunk reads,
  reconnects, and close through the parent network/cookie authority. The parent
  retains each live response stream and applies request-cookie selection and
  response `Set-Cookie`; the child receives only bounded chunks addressed by
  opaque stream IDs and continues to parse/dispatch SSE events. It has no
  direct-loader fallback and no raw `Cookie`/`Set-Cookie` header crosses IPC.
  `native_content_process_drives_event_source_named_multiline_events` passed
  (1 passed; 902 filtered; 63.91 seconds), verifying the request cookie, SSE
  response-cookie persistence, a subsequent request using that cookie, and a
  cookie-free Web Storage profile. The test-target rebuild took 13m06 on this
  checkout; Worker EventSource was still direct at that checkpoint.
- Content-worker protocol 29 now routes dedicated Worker EventSource open,
  bounded reads, reconnect, and close through the parent cookie/network
  authority. IPC binds each stream to the parent-captured page owner and worker
  ID while preserving the worker script URL as the network initiator; no
  Cookie/Set-Cookie headers or child-loader fallback are allowed. The existing
  worker SSE regression now checks the parent-selected HttpOnly request cookie
  and the parent-persisted HttpOnly response cookie. Focused `cargo check`
  passed; `native_content_process_worker_drives_event_source_named_events`
  passed (1 passed; 902 filtered; 24.63 seconds). Its incremental test-target
  build took 4m48.
- Content-worker protocol 30 now routes page and dedicated Worker WebSocket
  handshakes, frame sends/receives, and close through the parent cookie/network
  authority. The parent owns live sockets and handshake cookies; the child gets
  only opaque stream IDs, bounded frames, and lifecycle events. Focused
  `cargo check` passed; the page and Worker text/binary/close regressions passed
  (2 passed; 901 filtered; 40.73 seconds), verifying parent-selected HttpOnly
  handshake cookies and parent-persisted WebSocket response cookies. The page
  ordinary-turn background-delivery regression also passed (1 passed; 902
  filtered; 30.11 seconds).
- Content-worker protocol 31 routes synchronous page and Worker XHR through a
  blocking parent IPC round trip. The parent applies pending script cookie
  writes before the request, selects request cookies, accepts response
  `Set-Cookie`, and returns the response with only the URL-scoped visible
  cookie projection. The child has no local-loader fallback for this path.
  Focused `cargo check` passed. The process-backed
  `native_content_process_synchronous_xhr_uses_parent_cookie_authority`
  regression passed (1 passed; 903 filtered; 21.41 seconds), covering page and
  Worker XHR, a pending script setter, HttpOnly request/response cookies,
  same-name response-cookie rotation, and later requests. Formatting passed.
- HTTP(S) scripts inserted by inline scripts during initial document loading
  now use the owner-checked parent resource broker instead of the child loader.
  The process-backed
  `native_content_process_http_navigation_uses_parent_cookie_authority`
  regression passed (1 passed; 903 filtered; 34.45 seconds), checking that the
  inserted script request carries the parent's HttpOnly cookie, its response
  cookie is committed by the parent for a later request, and script sees only
  the visible projection. The code path also brokers module roots and their
  dependencies; this regression directly covers a classic script.
- User action mutations now carry the parent resource loader through the
  content-process IPC transaction. Newly attached HTTP(S) stylesheets and CSS
  imports/fonts, images, and media settle through an owner-checked parent
  broker; unavailable owner/runtime/loader state fails closed rather than
  falling back to the child loader. The process-backed
  `native_content_process_mutation_stylesheets_use_parent_cookie_authority`
  regression passed (1 passed; 904 filtered; 23.11-second runtime; integration
  target rebuild 2m21s), verifying parent-selected HttpOnly cookies on a
  click-created stylesheet/import chain, parent acceptance for a later Fetch,
  and HttpOnly invisibility in `document.cookie`. The scoped `cargo check`
  passed with existing dead-code warnings. This does not cover other
  child-direct internal network classes or claim full parent-only ownership.
- Initial document-load DedicatedWorker creation and startup Fetch now use an
  owner-checked parent broker. Only brokers created for that captured load set
  the captured-load marker; the parent still validates the exact context,
  frame, generation, and document URL, and later script/mutation brokers remain
  unmarked. The process-backed
  `native_content_process_initial_worker_startup_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 904 filtered; 22.11 seconds), verifying parent-
  selected HttpOnly cookies on Worker entry/startup Fetch, response-cookie
  rotation on a later Worker message Fetch, and HttpOnly invisibility in the
  page projection. Scoped `cargo check` passed with existing dead-code warnings.
- Local SharedWorker startup/connect Fetch and explicit page MessagePort turns
  now have process-backed evidence for the parent broker. The regression
  `native_content_process_shared_worker_fetch_uses_parent_cookie_authority`
  passed (1 passed; 904 filtered; 23.75 seconds), verifying HttpOnly request
  cookies on startup and `include`, no cookies for `omit`, startup and explicit
  response-cookie updates/deletion in later requests, and visible-only page
  cookies. At this checkpoint, standalone content-process autonomous
  WorkerTimer turns still used the local loader. The latest checkpoint below
  replaces that behavior; the scoped check passed with existing dead-code
  warnings.
- Autonomous DedicatedWorker and ServiceWorker timers now defer in every
  content-process mode. The child only announces the due owner; the parent
  advances the timer on the exact context/frame script turn, where timer Fetch
  and Worker Fetch-stream callbacks use the parent broker. The process-backed
  standalone regressions
  `native_content_process_worker_timer_fetch_uses_parent_cookie_authority` and
  `native_content_process_service_worker_timer_fetch_uses_parent_cookie_authority`
  passed together (2 passed; 1,689 filtered; 39.15 seconds), verifying
  parent-selected HttpOnly request cookies, accepted response-cookie rotation,
  and the visible-only `document.cookie` projection. `cargo check` passed;
  explicitly rebuilding `glass-native-content-worker` was required because
  process tests spawn that separate executable. The first run used a stale
  worker binary and failed; the rebuilt-worker run passed.
- ServiceWorker `postMessage` and page MessagePort-delivered ServiceWorker
  message events now receive the active owner-checked parent Fetch broker in
  the content-worker script turn. The process-backed
  `native_content_process_service_worker_message_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 1,691 filtered; 25.87 seconds), verifying the
  message event's HttpOnly response cookie is accepted by the parent and sent
  on a later page request, but never exposed through `document.cookie`. The two
  existing standalone timer regressions passed again (2 passed; 1,690 filtered;
  42.15 seconds). This closes those event-dispatch routes only; remaining
  child-direct internal network classes remain in scope.
- ServiceWorker FetchEvent continuations resumed after `clients.openWindow()`
  now carry the parent's exact context/frame/generation/document URL over IPC.
  The content process validates that captured owner against its committed
  document, and the resumed settlement retains the parent Fetch broker rather
  than falling back to its loader. Content-worker protocol is now version 32.
  The expanded process-backed
  `native_runtime_service_worker_fetch_open_window_resumes_navigation`
  regression passed (1 passed; 904 filtered; 33.95 seconds), verifying the
  resumed Fetch sends initial parent-owned HttpOnly cookies, its HttpOnly
  response cookie is accepted into the parent jar and sent on a later page
  request, and `document.cookie` does not expose it. Scoped `cargo check`
  passed with existing dead-code warnings; the companion content worker was
  explicitly rebuilt before the passing regression. Other child-direct internal
  network paths remain open.
- Implementation is in progress on `task/native-engine-browser-843`.
- Issue #40 remains open.
