id: native-engine-browser-839
scope: glass-browser/native-engine/service-worker-navigation-preload-fetch
status: in-progress
depends-on: [native-engine-browser-838]
---

# Glass native-engine browser slice 839: Navigation preload Fetch integration

## Objective

Execute enabled ServiceWorker navigation-preload requests through the native
network layer and expose their results through `FetchEvent.preloadResponse`.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-838.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Service Workers: Handle Fetch](https://www.w3.org/TR/service-workers/#handle-fetch)
- [Service Workers: `FetchEvent.preloadResponse`](https://www.w3.org/TR/service-workers/#dom-fetchevent-preloadresponse)
- [Service Workers: `FetchEvent.respondWith()`](https://www.w3.org/TR/service-workers/#dom-fetchevent-respondwith)
- [HTML Standard: event handler IDL attributes](https://html.spec.whatwg.org/multipage/webappapis.html#event-handler-idl-attributes)
- [Fetch Standard](https://fetch.spec.whatwg.org/)

## Contract

- Start preload only for a matching navigation whose request method is `GET`,
  whose registration is enabled, and whose active ServiceWorker has a
  non-empty `fetch` listener set.
- Clone the navigation request, add exactly the registration's
  `Service-Worker-Navigation-Preload` header value, and bypass ServiceWorker
  interception for that clone. Reuse the native loader's URL, origin,
  credentials, redirect, network-policy, cookie, response-size, and
  cancellation behavior; do not create a second browser/backend path. In a
  sandboxed content process, both preload and `fetch(event.request)` must use
  the exact captured-load, owner-checked parent broker. The parent alone
  selects request cookies, accepts `Set-Cookie`, and persists the jar; the
  child receives only the URL-scoped script-visible cookie projection.
- Carry the source Document URL and its effective Referrer-Policy separately
  across the browser/content-process boundary. Apply that policy to the
  initial preload and recompute the `Referer` after redirects, including
  response `Referrer-Policy` updates; never infer policy from an already
  shortened referrer URL. Expose `FetchEvent.request.referrer` in its
  policy-filtered form, including a reduced cross-origin value, while retaining
  the full source URL only in internal request state for network and preload
  calculations. Only the internal navigation token may bypass the public
  Request constructor's same-origin referrer restriction.
- Start the preload concurrently with FetchEvent dispatch. Navigation
  cancellation aborts it. It must not block dispatch while waiting for the
  network response or allow a late response to mutate a committed/cancelled
  navigation. If the handler has already produced its own response while the
  preload is still pending, commit the ServiceWorker response without waiting
  for the unused preload body and abort that pending request.
- The navigation `FetchEvent.request` exposes `mode === "navigate"`. Create
  that internal request without weakening the public `Request` constructor's
  rejection of `new Request(url, { mode: "navigate" })`.
- `FetchEvent.preloadResponse` resolves to an immutable, readable native
  `Response` on success, rejects with `TypeError` on a network error, and
  resolves to `undefined` when the algorithm does not start a preload.
- The `respondWith()` promise alone determines when the FetchEvent response is
  ready. Independent `waitUntil()` promises extend the event lifetime without
  delaying that response or converting their rejection into a navigation
  failure. The `respondWith()` promise itself also extends the event lifetime.
  Lifetime work, including native host commands it emits, must remain owned and
  continue after the response is returned; it must not be dropped to meet the
  response deadline. Calling `respondWith()` stops invocation of later
  listeners for that FetchEvent, as required by the FetchEvent dispatch
  algorithm.
- Dispatch `self.onfetch` in registration order as an event-handler listener
  in the same sequence as `addEventListener("fetch", ...)`: its first
  non-null assignment registers its position, replacing its callback while
  active preserves that position, assigning `null` removes it, and assigning a
  callback again registers it at the then-current end. A `respondWith()` call
  stops only listeners later in that ordered sequence.
- Preserve response status, URL, headers, redirect state, and bounded body
  bytes without consuming the worker-visible body. When the fetch handler does
  not call `respondWith`, follow the Service Workers fetch algorithm without
  issuing a duplicate request when the preload result is reusable.
- Tests assert the actual request method, URL, configured header, number of
  network requests, overlap with FetchEvent dispatch, navigation request mode,
  `Request.referrer`/`referrerPolicy`, source-document `Referer` policy, redirect
  policy updates, FetchEvent response semantics, network failure, and
  cancellation. Also verify a pending unrelated `waitUntil()` does not delay an
  independent response, while its lifetime work remains alive and can settle;
  verify `onfetch` and registered callbacks run in registration order across
  handler replacement and deactivation/reactivation, and verify
  `respondWith()` suppresses only later listeners in that order.
  Include non-GET,
  disabled, absent-listener, and enabled-listener controls.
- Process-backed network assertions are required; a pure command or mocked
  loader test alone does not close this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-839.md`

## Verification

- Focused process-backed integration tests for actual HTTP and JavaScript
  behavior, including cancellation and no duplicate request.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- `cargo fmt --all -- --check`
- Relevant Fetch and Service Worker WPT cases, with selected cases and
  deviations recorded.
- `git diff --check`.

## Current Evidence

- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q` passes on the current revision.
- `cargo test -p glass-browser --lib --features native-engine --locked navigation_preload -- --quiet` passes (six socket-free tests, including the immediate-response runtime regression).
- `cargo test -p glass-browser --lib --features native-engine --locked service_worker_fetch_ -- --quiet` passes (five socket-free runtime tests, including independent `waitUntil()` lifetime, listener suppression, `onfetch` registration order, handler replacement, and deactivation/reactivation).
- `cargo test -p glass-browser --lib --features native-engine --locked independent_response_survives_and_retains_lifetime_fetch_work -- --nocapture` passes. Its fixture-backed native-loader fetch returns the independent response first, remains owned by the Service Worker registry, then resolves the `waitUntil(fetch())` continuation in the same worker realm.
- Process-backed `native_content_process_persists_service_worker_cache_across_restart` passed (1 passed; 906 filtered; 39.70 seconds), closing Slice 838's persistence gate.
- Process-backed `native_service_worker_navigation_preload_sends_header_and_reuses_response` passed on the current broker signature (1 passed; 906 filtered; 36.50 seconds), verifying the configured header, navigation response reuse, referrer behavior, and following navigation.
- Process-backed `native_service_worker_fetch_event_navigation_request_uses_parent_cookie_authority` passed on its final cleanup-adjusted rerun (1 passed; 906 filtered; 25.04 seconds). `fetch(event.request)` on a controlled navigation sends the parent's HttpOnly seed, accepts the parent's HttpOnly response-cookie rotation, and sends both on the next page Fetch; `document.cookie` stays empty while `cookies_async()` reads the parent jar.
- `cargo fmt --all -- --check`, release-documentation truth, documentation depth, TUI shortcut inventory, and `git diff --check` pass.
- The response test proves headers are immutable and both the original and
  cloned response bodies remain readable. It also verifies the policy-reduced
  cross-origin `Request.referrer` and that public `Request` construction still
  rejects navigation mode.
- The fixture-backed registry regression confirms that a bodyless host Fetch
  command emitted by `waitUntil()` does not delay an already-settled response;
  its native network task remains owned, and its Promise continuation later
  executes in the same Service Worker realm. The content-process loop selects
  between incoming IPC and these task completions, persists resulting
  loader/cache state, and queues cookie changes for its next response.
  Removing the worker's routes aborts its outstanding tasks.
- The content-process wait loop now also selects the earliest due timer across
  DedicatedWorker, SharedWorker, active ServiceWorker, and waiting ServiceWorker
  realms while IPC is idle. A socket-free ServiceWorker registry regression
  verifies a `waitUntil()` timer callback runs after its independent FetchEvent
  response settles. Process-backed idle-timer and WPT evidence remain open.
- Content-worker stdin now has one blocking reader that routes normal request
  frames to the asynchronous loop and `dialog_decision` frames to the
  synchronous dialog host. The regression
  `content_ipc_reader_routes_dialog_decisions_without_stealing_requests`
  passes using a blocking in-memory reader; this verifies demultiplexing but
  does not establish process-backed dialog behavior or out-of-band event
  delivery.
- This does not close lifetime scheduling. Streaming upload Fetch commands
  still use the synchronous upload driver, and a Fetch command encountered
  while the `respondWith()` promise is still pending is resolved in the
  response settlement loop. Client messages, `openWindow()`, and MessagePort
  effects emitted by a later lifetime continuation are retained in the child
  queues but are not delivered out-of-band to the parent.
- Two socket-free runtime regressions verify that `onfetch` shares the ordered
  FetchEvent listener sequence, replacement preserves its position,
  deactivation/reactivation appends it at the new position, and `respondWith()`
  suppresses only later listeners.
- Local TCP listener binding works in the current checkout. An exploratory
  process-backed overlap probe used a timer-delayed independent FetchEvent
  response and a gated preload body. With `RUST_MIN_STACK=8388608`, the
  navigation still did not commit before the preload was released; this
  exposes a real response-progress gap while a parent-brokered preload is in
  flight. The default test-thread stack also overflows on that probe. Keep
  asynchronous response/preload overlap and cancellation open until the
  content-process event loop can advance the response independently.
- Process-backed eligibility controls, no-duplicate fallback, source-policy
  redirect updates, worker-visible `Request.referrer`, independent-response
  overlap, and cancellation are not all closed by the passing header/cookie
  tests above. WPT, remote CI, and cross-platform validation remain open.
