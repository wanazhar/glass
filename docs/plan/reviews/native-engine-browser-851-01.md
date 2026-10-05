# Native engine Slice 851 review

- **Revision reviewed:** Slice 851 worktree based on `330529a`
- **Task:** `native-engine-browser-851`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking credential-mode or cookie-owner mismatch was found.

## Review notes

- Page and Worker asynchronous XHR now pass Fetch credentials mode `same-origin`
  when `withCredentials` is false and `include` when true. The synchronous
  parent broker derives the same typed mode from the existing boolean; the
  local-owner synchronous runner uses that same mapping.
- `decode_parent_fetch_request` still rejects disagreement between the
  boolean credentials flag and the typed mode. The parent validates the
  captured context/frame/generation/document owner, applies only owner-tagged
  cookie writes to its loader, and returns a bounded visible cookie
  projection. No raw Cookie/Set-Cookie header or full jar was added to IPC,
  and the child HTTP(S) fail-closed path is unchanged.
- `xhr_credentials_modes_match_same_origin_and_include_contracts` verifies
  the default mode allows credentials only for a same-origin target and that
  explicit `include` remains credentialed cross-origin.
- `native_content_process_synchronous_xhr_uses_parent_cookie_authority`
  verifies same-origin default synchronous page XHR and asynchronous page and
  Worker XHR. The loopback server observes parent-selected HttpOnly cookies;
  response cookies rotate into later requests, remain available in the
  parent's cookie API, and stay absent from `document.cookie`.
- Scoped cargo check, content-worker build, both focused tests, formatting, and
  documentation audits passed locally. This is not remote-CI, cross-platform,
  cross-origin process/CORS, WPT, or full XHR/Web IDL certification.

## Conclusion

**Pass (direct self-review; not an independent review).**
