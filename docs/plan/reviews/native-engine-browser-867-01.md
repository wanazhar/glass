# Native engine Slice 867 review

- **Task:** `native-engine-browser-867`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

The regression confirmed one gap, fixed in this slice: EventSource's manual
redirect loop followed a redirect without first processing its eligible
`Set-Cookie` headers. The parent now updates its own jar for each eligible
network response before following a redirect or returning a final response.

## Review notes

- The two-origin process test opens a credentialed EventSource to `/redirect`.
  The first request carries the parent-owned HttpOnly seed; its 302 response
  adds a distinct HttpOnly cookie. The immediately following `/events` request
  must carry both cookies. The final response is CORS-authorized and dispatches
  open/message events.
- A later explicitly credentialed page Fetch must also carry both cookies.
  The parent cookie API retains both as HttpOnly, and `document.cookie` stays
  empty. The test rejects preflight and unexpected reconnect requests.
- Cookie processing remains gated by `withCredentials` or same-origin URL for
  each response hop. The parent remains the sole cookie matcher, acceptor, and
  jar owner; no raw cookie header or jar state crosses IPC.
- This is focused local evidence, not complete redirect policy, cross-origin
  redirect combinations, reconnect behavior, full EventSource/Fetch/CORS or
  WPT conformance, cross-platform certification, or remote CI.
- Standards reference: [Fetch Standard, CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).

## Verification

- Scoped `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed with successful compiler output
  suppressed.
- Exact `native_content_process_page_event_source_redirect_cookies_use_parent_authority`
  passed (1 passed; 930 filtered; 21.04 seconds).
- The release-documentation, documentation-depth, TUI-shortcut, and
  documentation-coverage gates passed after final documentation edits.
  Coverage used the existing shared-target `glass` and `glass-browser`
  binaries by explicit path. `rustfmt --check` and `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
