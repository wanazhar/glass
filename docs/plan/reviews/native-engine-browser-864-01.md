# Native engine Slice 864 review

- **Task:** `native-engine-browser-864`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

The original regression exposed one runtime defect, fixed in this slice: the
parent EventSource loader applied status, MIME, and CORS checks before parsing
the final actual response's `Set-Cookie` headers. A credentialed CORS failure
therefore discarded an eligible cookie even though CORS only denies exposing
the response to the caller. The cookie processing now occurs in the parent
before those response-exposure failures.

## Review notes

- The process-backed two-origin regression observes the actual EventSource
  GET with its parent-selected HttpOnly seed and no preflight. The response
  includes wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, an HttpOnly `Set-Cookie`, and
  event-stream data. Credentialed CORS rejects it: script observes `error`
  with no `open` or message event.
- The parent cookie API retains the seed and actual-response cookies as
  HttpOnly; the later authorized page Fetch sends both and
  `document.cookie` remains empty. The error handler closes EventSource so
  automatic reconnects do not extend the test's contract.
- The fix accepts response cookies only when `withCredentials` is enabled or
  the response URL is same-origin with the EventSource document. It keeps
  cookie matching and storage in the parent and introduces no child cookie
  store, profile, raw-cookie IPC, or event-body buffering. Redirect-response
  cookie behavior remains outside this slice.
- The Fetch Standard says credentialed CORS failures still respect
  `Set-Cookie` response headers: [CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).
- Scoped `cargo check` and the exact process-backed regression passed. This
  does not certify EventSource redirects/reconnects, full Fetch/CORS or WPT
  behavior, other platforms, independent review, or remote CI.

## Documentation gates

- `python3 scripts/check-release-documentation.py
  --require-previous-version` passed: 1,511 Markdown documents, 83 current,
  63 previous-version hits, 1,674 semantic audit hits, zero current-claim
  failures.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides
  routed and audited; 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` passed: 15 implementation help
  keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` passed using the existing
  shared-target `glass` and `glass-browser` binaries: 1,511 Markdown files,
  346 full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules.
- `rustfmt --edition 2024 --check` on the changed Rust files and
  `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
