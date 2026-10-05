# Native engine Slice 852 review

- **Revision reviewed:** Slice 852 based on `c88911c`
- **Task:** `native-engine-browser-852`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking credential-mode, CORS, cookie-owner, or IPC-boundary mismatch
was found.

## Review notes

- The loopback servers use different ports to create distinct origins while
  retaining host/path cookie matching. The regression exercises asynchronous
  page XHR, synchronous page XHR via the parent broker, and asynchronous
  DedicatedWorker XHR.
- With `withCredentials == false`, the server observes no cross-origin Cookie
  header, and its response `Set-Cookie` value is not installed. With
  `withCredentials == true`, the server observes cookies selected by the
  parent, and exact-origin `Access-Control-Allow-Origin` plus
  `Access-Control-Allow-Credentials: true` permits the response cookie. Later
  include-mode requests observe the accepted cookies; default-mode requests
  still omit them.
- All cookies are HttpOnly. The test confirms accepted cookies remain visible
  through the parent's cookie API but `document.cookie` is empty. No raw
  Cookie/Set-Cookie header or full jar was added to IPC. The parent remains the
  only cookie matcher, response-cookie authority, and jar owner.
- The test-only code did not alter production request or ownership code. The
  focused process regression and scoped `glass-browser` check passed locally;
  the test reused the unchanged Slice 851 content-worker binary.
- This is not evidence for XHR redirects, full XHR/Web IDL or WPT conformance,
  other platforms, independent review, or remote CI.

## Conclusion

**Pass (direct self-review; not an independent review).**
