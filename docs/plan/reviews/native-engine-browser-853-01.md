# Native engine Slice 853 review

- **Revision reviewed:** Slice 853 based on `0acc2b3c`
- **Task:** `native-engine-browser-853`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking redirect credential-mode, CORS, cookie-owner, or IPC-boundary
mismatch was found.

## Review notes

- The loopback fixtures use two ports on `127.0.0.1`: same-origin starting
  requests go to the page origin, and redirect targets use a distinct origin
  with the same cookie host/path scope.
- The regression observes asynchronous page XHR with default credentials,
  synchronous page XHR through the parent broker with `include`, and both
  default and `include` asynchronous DedicatedWorker XHR. At the cross-origin
  hop, default mode sends no Cookie header and its response cookie is absent
  from the parent's later cookie state. Include mode sends parent-selected
  cookies only with exact-origin credentialed CORS; accepted response cookies
  are visible on later requests.
- HttpOnly response cookies are visible through the parent cookie API but
  absent from `document.cookie`. The test changes no production network or
  owner code, adds no cookie IPC fields, and does not enable a child-side
  network retry. The parent remains the sole cookie matcher, response-cookie
  authority, and jar owner.
- Scoped `glass-browser` check and the focused process-backed regression passed
  locally. This is not evidence for all redirect combinations, WPT/full XHR
  conformance, other platforms, independent review, or remote CI.

## Conclusion

**Pass (direct self-review; not an independent review).**
