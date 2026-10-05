# Native engine Slice 857 review

- **Revision reviewed:** Slice 857 based on `3eccdc13`
- **Task:** `native-engine-browser-857`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking preflight-cache isolation, cookie-owner, or IPC-boundary
mismatch was found.

## Review notes

- The loopback page/API pair uses distinct ports and one repeated API URL,
  method, and requested-header set. The server requires the default-mode
  OPTIONS and cookie-free POST, then requires a second OPTIONS before the
  credentialed POST. If an uncredentialed cache entry authorizes `include`,
  the fixture sees the wrong method at this point and fails.
- The server then expects another credentialed POST without an extra OPTIONS,
  proving the successful `include` cache entry is reused. The default request
  sends no Cookie; the credentialed requests receive only parent-matched
  cookies. An HttpOnly response cookie from the authorized actual request is
  selected on the later cached request.
- Preflight `Set-Cookie` values and the default-mode actual response cookie
  are absent from the parent jar. HttpOnly remains absent from
  `document.cookie`; no raw Cookie/Set-Cookie header or jar was added to IPC.
- Scoped check and the exact process-backed regression passed locally. This
  does not certify full Fetch cache, CORS/XHR, WPT, other platforms,
  independent review, or remote CI.

## Conclusion

**Pass (direct self-review; not an independent review).**
