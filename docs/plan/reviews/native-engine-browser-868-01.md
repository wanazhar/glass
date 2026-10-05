# Native engine Slice 868 review

- **Task:** `native-engine-browser-868`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

No implementation defect was found. `open_event_source_async` applies its
existing same-origin credentials gate independently to each redirect target
and response. The process-backed regression confirms the redirect response's
HttpOnly cookie is rejected and is absent from the next request and later
parent-authorized cookie state.

## Review notes

- The page opens an EventSource without `withCredentials: true` to a distinct
  loopback API origin. Both `/redirect` and `/events` must arrive without a
  `Cookie` header, with the page `Origin` and EventSource `Accept`; neither
  request triggers a preflight.
- The redirect response attempts to set an HttpOnly cookie. The final
  CORS-authorized event stream still dispatches open/message events.
- A later explicitly credentialed page Fetch carries the original parent seed
  but not the redirect cookie. The parent cookie API excludes that cookie, and
  `document.cookie` remains empty.
- Parent cookie matching, response-cookie acceptance/rejection, and the jar
  remain parent-owned. No raw cookie header or complete jar crosses IPC.
- This does not certify cross-origin-to-cross-origin redirect policy, redirect
  loops, reconnect behavior, full EventSource/Fetch/CORS or WPT conformance,
  cross-platform behavior, or remote CI.
- Standards reference: [Fetch Standard, CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).

## Verification

- Scoped `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed with successful compiler output
  suppressed.
- Exact `native_content_process_page_event_source_omits_redirect_cookies_by_default`
  passed (1 passed; 931 filtered; 23.26 seconds).
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed after final edits. Coverage used the existing shared-target
  `glass` and `glass-browser` binaries by explicit path.

## Conclusion

**Pass (direct self-review; not an independent review).**
