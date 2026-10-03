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
- Verify a child shutdown/restart cannot mutate or lose the parent jar and
  that parent-broker failure cannot trigger a direct child request.
- Run selected Fetch/Cookie/SharedWorker WPT cases and record exact selections
  and deviations; mocked-loader checks do not replace network regressions.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
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
  changes there. The child still receives the shared path and full profile and
  can rewrite the combined snapshot, so child write capability is not removed
  yet. Child profile loading/mirroring, mutation authority, and general network
  brokering remain outstanding. The restart persistence regression passed
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
  module/font destinations, worker module-graph process coverage, autonomous worker events,
  independently delivered SharedWorker messages, or Service Worker internal
  network requests, which still use the child loader. A later checkpoint below
  now routes worker entry and classic imported scripts through the parent;
  module-graph coverage remains outstanding.
  The broker currently buffers the bounded response before resolving Fetch;
  incremental network response streaming/backpressure remains outstanding.
- The latest scoped `cargo check -p glass-browser --features native-engine
  --lib --test native_engine --locked --quiet` passed with existing dead-code
  warnings only. The extended process-backed
  `native_content_process_http_navigation_uses_parent_cookie_authority` test
  also passed (1 passed; 21.17 seconds); it is recorded below.
- `cargo fmt --all -- --check`, `git diff --check`, release-documentation
  truth, documentation depth, and TUI shortcut inventory checks pass. The
  latest static doc reports cover 1,471 Markdown files with zero current-claim
  failures, 93 current guides and 19 contracts, and 15 implementation shortcut
  keys/63 documentation markers. The link/inventory coverage check was not
  rerun because its prior attempt reported the live MCP schema size as 173,741
  bytes while `docs/mcp-schema-budget.md` records 173,237; the `glass` debug
  binary has not been rebuilt in this checkout.
- Standard buffered Fetch emitted during dedicated-worker initialization and
  while handling an explicit page `Worker.postMessage` now uses the parent
  broker. The protocol validates the captured page owner separately from the
  worker request URL, applies pending page cookie writes before selecting
  request cookies, commits response-cookie changes in the parent, and returns
  the page owner's visible cookie projection. The existing worker
  response-stream interface is preserved over the buffered parent response.
  Locally hosted SharedWorker creation/connect evaluations also receive the
  broker, but lack a focused process-backed regression. The process-backed
  `native_content_process_worker_message_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 24.15-second test runtime), checking a same-turn
  page setter on the startup Fetch, the parent Set-Cookie update on the later
  message Fetch, response delivery, and distinct page/worker URLs. Worker
  script/resource/module-graph loading, autonomous turns, independently
  delivered SharedWorker events, module/font destinations, initial/resource
  loading, and Service Worker internal requests remain outside the verified
  broker path. Dedicated-worker upload streams were not covered at that
  checkpoint; see the subsequent upload-stream checkpoint below.
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
  later request. Service Worker lifetime/background Fetches,
  Service Worker-originated upload streams, and out-of-band Service Worker
  events remain outside this broker path.
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
  fallback to run. Service Worker-originated upload streams and lifetime or
  autonomous worker turns remain outside this broker path.
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
- In browser-coordinated mode, a due DedicatedWorker timer no longer executes
  in the child idle loop. It notifies the exact context/frame owner, which runs
  the timer on its next script turn with the parent Fetch broker; standalone
  `NativeEngine` retains local timer dispatch. The process-backed unit
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
  timer Fetch, not module-graph variants. Full child-profile removal,
  page-resource broker coverage, ServiceWorker timer/lifetime requests, and
  other unbrokered network
  classes remain open.
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
  child script branch. Parser-discovered scripts during initial page loading,
  page stylesheets/images/fonts/media, and other unbrokered requests still use
  the child loader; the
  complete content-process cookie profile and persistence paths also remain.
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
  invisibility during parser execution. The regression directly covers a
  classic parser script, not the initial module graph. Stylesheets, images,
  fonts, media, Service Worker-handled navigation responses, and the full child
  profile/read/write path remain outstanding.
- The Slice 842 HTTP regression successfully bound a local listener. The
  earlier blanket claim that local TCP binding is denied is stale for this
  checkout; that result does not prove parent-brokered network transport.
- Implementation is in progress on `task/native-engine-browser-843`.
- Issue #40 remains open.
