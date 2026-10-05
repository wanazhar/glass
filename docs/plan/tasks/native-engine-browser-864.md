---
id: native-engine-browser-864
scope: glass-browser/native-engine/eventsource-cors-error-cookies
status: done
depends-on: [native-engine-browser-863]
---

# Glass native-engine browser slice 864: EventSource CORS-error cookies

## Objective

Verify that a credentialed EventSource actual response rejected by CORS still
updates the parent-owned cookie jar, while the page receives only the
EventSource error behavior and a later authorized page request reuses the
accepted HttpOnly cookie.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-843.md` — content-process parent
  broker, EventSource transport, and cookie-ownership audit.
- `docs/plan/tasks/native-engine-browser-863.md` — ServiceWorker Fetch CORS
  failures across ordinary and captured-load parent-broker routes.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and evidence | Slice 864 treatment |
|---|---|---|
| Page and DedicatedWorker Fetch | Owner-checked parent broker; Slice 860 verifies actual-response CORS-error cookies. | No change. |
| SharedWorker Fetch | Browser-owned coordinator uses the parent loader; Slice 861 verifies actual-response CORS-error cookies. | No change. |
| ServiceWorker FetchEvent | Captured-load and ordinary parent-broker routes are covered by Slices 862-863. | No change. |
| Page EventSource | The content process opens and reads an opaque parent-owned event stream; existing tests verify successful same-origin response-cookie acceptance. No process-backed evidence currently covers an actual response rejected by credentialed CORS while carrying `Set-Cookie`. | Verify the error path and subsequent parent-cookie reuse. |

EventSource has an error-event/reconnect lifecycle rather than Fetch's rejected
Promise. This slice proves the response-cookie ownership boundary without
claiming complete EventSource, redirect, reconnection, or WPT behavior.
The Fetch Standard specifies that credentialed CORS failures still respect
`Set-Cookie` response headers; the CORS error controls response access, not the
parent's network-owned cookie processing ([CORS and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)).

## Contract

- Use distinct loopback page and API origins. The page opens
  `EventSource(api_url, { withCredentials: true })` through the parent broker.
- The API receives one simple GET with the parent-selected HttpOnly seed cookie
  and the page `Origin`; it does not receive a preflight. Its actual response
  uses `Content-Type: text/event-stream`, wildcard
  `Access-Control-Allow-Origin`, `Access-Control-Allow-Credentials: true`,
  and a distinct HttpOnly `Set-Cookie`.
- Credentialed CORS rejects the stream: page script receives an `error`, but
  receives neither `open` nor event data. The test closes the EventSource in
  that error handler so a reconnect cannot add another request.
- A later authorized page Fetch carries both the seed and the actual-response
  cookie. The parent's cookie API retains both as HttpOnly and
  `document.cookie` remains empty.
- Accept the actual response's cookie in the parent before applying EventSource
  CORS exposure checks, and only when the EventSource credentials mode permits
  cookies for that response URL.
- Preserve the parent as the sole request-cookie matcher, `Set-Cookie`
  acceptor, persistence owner, and EventSource transport owner. Do not expose
  raw cookie headers, cookie profiles, or the complete jar to the content
  process.
- If the regression reveals a defect, change only the parent-brokered
  EventSource response propagation necessary to meet this contract.

## Tradeoff

This adds one process-backed two-origin negative case to the EventSource
cookie path. It exposed that the parent loader checked CORS before accepting
the final response's cookie, so the bounded runtime fix now processes eligible
response cookies first. Closing on the first error avoids spending time on
automatic reconnect semantics, which remain a separate contract. The parent
remains the only cookie authority; no response cookie data is sent to the
content process.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-864.md`
- `docs/plan/reviews/native-engine-browser-864-01.md`

## Verification

- Add one bounded process-backed two-origin EventSource CORS-failure test.
  Assert the actual GET, parent-selected seed, origin, absence of preflight,
  error-without-open/data, response-cookie acceptance, and later authorized
  reuse.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; do not create
  a worktree-local target or run workspace-wide tests.
- Run only the exact EventSource regression. Fix any fixture or implementation
  issue in one batch and rerun that exact regression.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final documentation edits. Reuse the existing shared-target
  inventory binaries by explicit path.
- Commit locally with the configured user Git identity. Do not push code or
  claim remote CI, platform certification, complete EventSource or WPT
  conformance. Keep issue #40 open.

## Results

- The first process-backed run reproduced the gap: the credentialed
  cross-origin EventSource received a parent-selected seed cookie and returned
  a real response with wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, and HttpOnly `Set-Cookie`; script
  received only an error, but the later page request carried only the seed.
- `open_event_source_async` now applies final-response `Set-Cookie` values in
  the parent before status, MIME, and CORS exposure failures are returned, and
  only when the EventSource's credentials mode permits cookies for that URL.
  The page still receives no response headers, cookies, or stream data. No
  content-process cookie state or IPC cookie-header transfer was added.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --lib --test native_engine --locked
  --quiet` passed; successful compiler output was suppressed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_content_process_page_event_source_cors_errors_keep_parent_cookies
  -- --exact --nocapture` passed (1 passed; 927 filtered; 29.08 seconds).
- The regression verifies the actual GET, origin, seed cookie, no preflight,
  error without `open`/message data, acceptance of both HttpOnly cookies by
  the parent, reuse on a later authorized page Fetch, and an empty
  `document.cookie`. Closing EventSource on its first error keeps automatic
  reconnect behavior outside this result.
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed; detailed outputs are recorded in the Slice 864 review.
- This is focused local evidence only. EventSource redirect/reconnect
  behavior, complete Fetch/CORS or WPT conformance, other platforms,
  independent review, and remote CI remain unverified. Issue #40 remains open.
