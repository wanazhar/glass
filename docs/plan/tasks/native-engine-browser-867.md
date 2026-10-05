---
id: native-engine-browser-867
scope: glass-browser/native-engine/eventsource-redirect-response-cookie-order
status: done
depends-on: [native-engine-browser-866]
---

# Glass native-engine browser slice 867: EventSource redirect response cookies

## Objective

Verify that a credentialed EventSource redirect response updates the parent
cookie jar before the next redirect hop, and that the final stream and later
authorized page request use the accepted HttpOnly cookie.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/native-engine-browser-profile.md#page-eventsource-cors-error-response-cookies`
- `docs/plan/tasks/native-engine-browser-864.md` — final actual-response cookie
  acceptance before CORS exposure failure.
- `docs/plan/tasks/native-engine-browser-866.md` — credentials-mode exclusion
  for cross-origin EventSource cookies.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and evidence | Slice 867 treatment |
|---|---|---|
| Final page EventSource response | Content-process owner-bound parent broker; Slice 864 verifies eligible final `Set-Cookie` processing before a CORS error. | Preserve as baseline. |
| Default-credential cross-origin EventSource | Parent broker; Slice 866 verifies no request/response cookies while CORS allows stream delivery. | Preserve credentials-mode gate. |
| Page EventSource redirect response | Same parent ResourceLoader manually follows redirects, but its current response-cookie block runs only after the redirect loop. No process-backed redirect-response cookie ordering evidence was found. | Verify redirect cookie acceptance before the next hop. |

## Contract

- Serve the page and API on distinct loopback origins. The page response sets
  a parent-owned HttpOnly seed cookie and opens a credentialed EventSource to
  the API's `/redirect` endpoint.
- The parent-selected seed reaches `/redirect`. This actual 302 response sets a
  distinct HttpOnly cookie and redirects within the API origin to `/events`.
  The parent accepts that cookie before issuing the next request.
- `/events` receives both parent-selected cookies, the page `Origin`, and the
  EventSource Accept header; no preflight occurs. Its exact-origin credentialed
  CORS response delivers `open` and stream data, after which script closes the
  connection.
- A later authorized page Fetch also sends both cookies. The parent cookie API
  retains the seed and redirect cookie as HttpOnly; `document.cookie` remains
  empty.
- Preserve the parent's sole request-cookie matcher, redirect policy owner,
  `Set-Cookie` acceptor, and persistence role. No raw cookie header, profile,
  or complete jar may cross into the content process.
- If the regression exposes a defect, process each eligible redirect response
  cookie in the parent before following its Location; preserve the credential
  gate for every hop and final response.

## Tradeoff

This adds one bounded redirect within a single cross-origin EventSource API
origin. It verifies cookie ordering without expanding cross-origin redirect
policy, redirect loops, reconnects, or full EventSource/WPT conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  redirect cookie ordering is defective)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-867.md`
- `docs/plan/reviews/native-engine-browser-867-01.md`

## Verification

- Add one process-backed two-origin regression for a credentialed EventSource
  redirect. Assert the seed on the first hop, response cookie on the next hop
  and later authorized request, Origin, no preflight, successful stream
  delivery, and parent cookie API visibility.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; do not create
  a worktree-local target or run workspace-wide tests.
- Run only the exact regression, then formatting, `git diff --check`, and all
  four maintainer documentation gates after final documentation edits. Use the
  existing shared-target binaries by explicit path.
- Commit locally with the configured Git identity. Do not push or claim remote
  CI, platform certification, complete EventSource, or WPT conformance. Keep
  Issue #40 open.

## Results

- The first source audit found that `open_event_source_async` accepted
  `Set-Cookie` only after its manual redirect loop. The parent now processes
  each response's eligible cookies immediately after receiving it and before
  following a redirect, preserving the credentials-mode gate at every URL.
  No cookie headers or jar data cross into the content process.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --lib --test native_engine --locked
  --quiet` passed; successful compiler output was suppressed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_content_process_page_event_source_redirect_cookies_use_parent_authority
  -- --exact --nocapture` passed (1 passed; 930 filtered; 21.04 seconds).
- The regression verifies the initial parent seed on `/redirect`, the
  redirect response's HttpOnly cookie on `/events` before stream delivery,
  successful CORS/open/message behavior, reuse of both cookies by a later
  authorized page Fetch, parent API visibility, and an empty
  `document.cookie`. It also checks the origin, EventSource Accept header, no
  preflight, and absence of reconnect requests.
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed; detailed commands/results are recorded in the Slice 867 review.
- This is focused local evidence only. Cross-origin redirect combinations,
  redirect loops, EventSource reconnect behavior, full Fetch/CORS or WPT
  conformance, other platforms, independent review, and remote CI remain
  unverified. Issue #40 remains open.
