---
id: native-engine-browser-866
scope: glass-browser/native-engine/eventsource-same-origin-credentials-cookie-omission
status: in-progress
depends-on: [native-engine-browser-865]
---

# Glass native-engine browser slice 866: default EventSource credential omission

## Objective

Verify that a cross-origin EventSource with default `withCredentials: false`
can receive a CORS-authorized stream while neither sending cookies nor
accepting its response `Set-Cookie` into the parent-owned jar.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/native-engine-browser-profile.md#page-eventsource-cors-error-response-cookies`
- `docs/plan/tasks/native-engine-browser-864.md` — credentialed page EventSource
  CORS-error response-cookie handling.
- `docs/plan/tasks/native-engine-browser-865.md` — browser-owned SharedWorker
  EventSource CORS-error owner path.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and evidence | Slice 866 treatment |
|---|---|---|
| Page EventSource with `withCredentials: true` | Content-process parent broker; Slice 864 verifies actual-response cookie acceptance after CORS failure. | Contrast with default credentials. |
| Browser-owned SharedWorker EventSource | Parent coordinator; Slice 865 verifies the same CORS-error cookie contract for its distinct owner path. | No change. |
| Page EventSource with default `withCredentials: false` | Parent broker and shared ResourceLoader credential gate; no process-backed cross-origin response-cookie omission evidence found. | Verify request-cookie omission and response-cookie rejection while CORS permits stream access. |

EventSource with `withCredentials: false` uses same-origin credentials. A
cross-origin response may pass CORS without credential authorization, but its
cookies are not credentials for this request. The parent must not attach
cookies or honor response `Set-Cookie` for that cross-origin request.

## Contract

- Serve the page and API on distinct loopback ports. The page response sets an
  HttpOnly seed cookie; the page creates `new EventSource(api_url)` without an
  init dictionary and confirms `withCredentials` is false.
- The API receives one GET with the page `Origin`, no `Cookie`, and no
  preflight. It returns a valid `Access-Control-Allow-Origin: *` response,
  `text/event-stream` data, and a distinct HttpOnly `Set-Cookie`.
- CORS permits the stream: page script sees `open` and the event data, then
  closes EventSource. This ensures rejection is not being misreported as
  omitted credential handling.
- A later page Fetch explicitly using `credentials: include` sends the
  parent-selected seed but not the EventSource response cookie. The parent
  cookie API retains the seed and excludes the response cookie; the seed stays
  out of `document.cookie` because it is HttpOnly.
- Preserve parent-only request matching, response-cookie processing, and
  persistence. Do not transfer a cookie profile, raw headers, or jar into the
  content process.
- If the test reveals a defect, fix only EventSource credentials-mode cookie
  selection/acceptance in the parent loader.

## Tradeoff

This adds one successful cross-origin stream case to distinguish response
visibility from cookie credentials. It verifies a security-sensitive
negative path without expanding redirect, reconnect, or general EventSource
conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  the parent credentials gate is defective)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-866.md`
- `docs/plan/reviews/native-engine-browser-866-01.md`

## Verification

- Add one process-backed two-origin regression. Assert the actual GET, Origin,
  absent request Cookie/preflight, successful stream events, absent response
  cookie on a later credentialed request, and parent cookie API result.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; do not create
  a worktree-local target or run workspace-wide tests.
- Run only the exact regression, then formatting, `git diff --check`, and all
  four maintainer documentation gates after final docs edits. Use the existing
  shared-target binaries by explicit path.
- Commit locally with the configured Git identity. Do not push or claim remote
  CI, platform certification, complete EventSource, or WPT conformance. Keep
  Issue #40 open.

## Results

Implementation and focused verification are pending.
