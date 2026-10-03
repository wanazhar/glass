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
  This does not cover Fetch during initial document loading, streamed uploads,
  module/font destinations, worker-script/resource/module-graph loading,
  autonomous worker events, independently delivered SharedWorker messages, or
  Service Worker internal network requests, which still use the child loader.
  The broker currently buffers the bounded response before resolving Fetch;
  incremental network response streaming/backpressure remains outstanding.
- The latest scoped `cargo check -p glass-browser --features native-engine
  --lib --test native_engine --locked --quiet` passed with existing dead-code
  warnings only; the focused worker regression is recorded below.
- `cargo fmt --all -- --check`, `git diff --check`, release-documentation
  truth, documentation depth, and TUI shortcut inventory checks pass.
  Documentation coverage remains blocked only by the existing missing live
  MCP schema measurement ``| Serialized `tools` array | 173,741 UTF-8 bytes |``
  in `docs/mcp-schema-budget.md`.
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
  delivered SharedWorker events, streamed uploads, module/font destinations,
  initial/resource loading, and Service Worker internal requests remain
  outside the verified broker path.
- Page-to-dedicated-worker events delivered through a transferred `MessagePort`
  now retain the parent Fetch broker through worker callback evaluation. The
  expanded `native_content_process_worker_message_fetch_uses_parent_cookie_authority`
  regression passed (1 passed; 35.17-second test runtime): its MessagePort
  callback Fetch receives the prior parent `Set-Cookie` update, the HttpOnly
  cookie, and a same-turn page cookie write. Other independently delivered
  worker events, autonomous turns, and SharedWorker MessagePort callbacks are
  not thereby verified as parent-brokered.
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
- The Slice 842 HTTP regression successfully bound a local listener. The
  earlier blanket claim that local TCP binding is denied is stale for this
  checkout; that result does not prove parent-brokered network transport.
- Implementation is in progress on `task/native-engine-browser-843`.
- Issue #40 remains open.
