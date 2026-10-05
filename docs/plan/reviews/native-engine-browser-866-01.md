# Native engine Slice 866 review

- **Task:** `native-engine-browser-866`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. The parent loader's credentials gate correctly omits cross-origin
cookies for EventSource's default same-origin credentials mode and ignores
the response cookie while allowing the CORS-readable stream.

## Review notes

- The two-origin process-backed test uses distinct ports. The page sets an
  HttpOnly seed cookie, then creates EventSource without an init dictionary;
  script confirms `withCredentials` is false.
- The API sees the page Origin but no Cookie or preflight. Its response uses
  `Access-Control-Allow-Origin: *`, an SSE body, and a distinct HttpOnly
  `Set-Cookie`. CORS permits `open` and the message event, so event delivery
  is independent of cookie inclusion.
- A later page Fetch with `credentials: include` carries the parent-selected
  seed but not the EventSource response cookie. The parent cookie API retains
  only the HttpOnly seed; `document.cookie` remains empty.
- No runtime code change, child cookie state, or raw-cookie IPC was added.
  This focused case does not certify redirects/reconnects, full EventSource,
  Fetch/CORS or WPT behavior, other platforms, or remote CI.
- Standards reference: [Fetch Standard, CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).

## Verification

- Scoped `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed with successful compiler output
  suppressed.
- Exact `native_content_process_page_event_source_omits_cross_origin_cookies_by_default`
  passed (1 passed; 929 filtered; 23.24 seconds).
- The release-documentation, documentation-depth, TUI-shortcut, and
  documentation-coverage gates passed after final documentation edits.
  Coverage used the existing shared-target `glass` and `glass-browser`
  binaries by explicit path. `rustfmt --check` and `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
