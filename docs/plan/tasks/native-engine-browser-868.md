---
id: native-engine-browser-868
scope: glass-browser/native-engine/eventsource-default-credentials-redirect-cookies
status: done
depends-on: [native-engine-browser-867]
---

# Glass native-engine browser slice 868: default EventSource credentials across redirects

## Objective

Verify that default same-origin EventSource credentials remain omitted at each
cross-origin redirect hop, and that the parent rejects an HttpOnly
`Set-Cookie` from a cross-origin redirect response. Preserve the existing
cookie seed and successful CORS-authorized stream behavior.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/native-engine-browser-profile.md#page-eventsource-default-credentials`
- `docs/plan/native-engine-browser-profile.md#page-eventsource-redirect-response-cookies`
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and existing evidence | Slice 868 treatment |
|---|---|---|
| Default-credential page EventSource | Parent broker; Slice 866 covers a direct cross-origin stream with no request or response cookies. | Extend the credentials-mode proof through a redirect response and next hop. |
| Credentialed page EventSource redirect | Parent ResourceLoader; Slice 867 verifies accepting the redirect cookie before the following hop. | Preserve acceptance when the request credentials mode permits it. |
| Page cookie authority | Parent-only matcher, Set-Cookie acceptor, and jar; HttpOnly state stays out of the child. | Prove redirect response cookie rejection and later parent-jar contents. |

## Contract

- Serve a page and API on distinct loopback origins. The page response sets an
  HttpOnly seed and creates an EventSource without `withCredentials: true`.
- The first cross-origin `/redirect` request carries no `Cookie`, includes the
  page `Origin` and EventSource `Accept`, and has no preflight. Its 302 response
  sets a distinct HttpOnly cookie and redirects within the API origin.
- The next `/events` request also carries no `Cookie`. A CORS-authorized
  `text/event-stream` response delivers `open` and message events without
  requiring credentialed CORS headers; the page closes the stream.
- A later explicitly credentialed page Fetch carries the original seed but
  not the redirect cookie. The parent's cookie API excludes the redirect
  cookie, while `document.cookie` remains empty.
- Keep request-cookie matching and all `Set-Cookie` decisions in the parent.
  No cookie profile, raw cookie header, or complete jar may cross IPC.
- If the regression exposes a defect, enforce the existing same-origin
  credentials mode independently for each response and redirect target; do
  not weaken Slice 867's cookie processing for credentialed requests.

## Tradeoff

This adds one same-API-origin redirect to the default-credentials case. It
proves cookie omission and rejection on each cross-origin request without
claiming cross-origin-to-cross-origin redirect policy, redirect loops,
reconnection, or full EventSource/WPT conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  the credentials gate is defective)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-868.md`
- `docs/plan/reviews/native-engine-browser-868-01.md`

## Verification

- Add one process-backed regression using real loopback HTTP listeners. Assert
  no cookies on the redirect or final stream request, no redirect-cookie
  acceptance, successful CORS-authorized event delivery, and the original-only
  cookie on a later authorized page Fetch.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; suppress
  successful compiler output.
- Run only the exact regression, then formatting, `git diff --check`, and all
  four maintainer documentation gates after final documentation edits. Use the
  existing shared-target binaries by explicit path.
- Commit the design checkpoint before code and evidence. Commit the completed
  slice locally with a focused Conventional Commit; do not push or claim remote
  CI.
- Update Issue #40 only after the local checkpoint; keep the epic open.

## Results

- The parent already applied the EventSource credentials gate per URL, so no
  runtime change was necessary. The process-backed regression verifies that
  both cross-origin EventSource hops omit `Cookie`; the 302's HttpOnly
  `Set-Cookie` is not accepted; the CORS-authorized stream still delivers
  open/message events; and a later explicitly credentialed page Fetch sends
  only the preexisting seed. The parent cookie API excludes the redirect
  cookie and `document.cookie` remains empty. No preflight or reconnect occurs.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --lib --test native_engine --locked
  --quiet` passed; successful compiler output was suppressed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_content_process_page_event_source_omits_redirect_cookies_by_default
  -- --exact --nocapture` passed (1 passed; 931 filtered; 23.26 seconds).
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed after the final documentation edits; exact gate results are in
  the review.
- This is focused local evidence only. Cross-origin-to-cross-origin redirect
  policy, redirect loops, EventSource reconnect behavior, full Fetch/CORS or
  WPT conformance, other platforms, independent review, and remote CI remain
  unverified. Issue #40 remains open.
