# Native engine Slice 855 review

- **Revision reviewed:** Slice 855 based on `843e42b`
- **Task:** `native-engine-browser-855`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking CORS denial, cookie-owner, or IPC-boundary mismatch was
found.

## Review notes

- The process fixture uses separate loopback ports for the page and API
  origins. It checks the page and DedicatedWorker OPTIONS requests for the
  expected Origin, POST method, normalized requested headers, and absence of
  Cookie.
- Both preflight responses include wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, and otherwise allow the method and
  requested headers. Fetch evaluates the CORS check using the original
  request's `include` credentials mode, so this response must be rejected.
  Both XHRs report a network error, and the API listener fails if an actual
  request follows either preflight.
- The preflight response's HttpOnly `Set-Cookie` is not accepted. The seeded
  parent-owned HttpOnly cookie remains available to the cookie API, while
  `document.cookie` stays empty. No raw Cookie/Set-Cookie header or jar was
  added to IPC, and no child-side HTTP(S) retry was enabled.
- Scoped check and the exact process-backed regression passed locally. This
  does not certify full CORS/XHR behavior, WPT, other platforms, independent
  review, or remote CI.

## Conclusion

**Pass (direct self-review; not an independent review).**
