# Native engine Slice 860 review

- **Task:** `native-engine-browser-860`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. The page and DedicatedWorker Fetch rejection types and cookie-owner
boundaries match the slice contract.

## Review notes

- The page and Worker response resolvers now reject failed Fetch requests with
  actual `TypeError` instances. The Worker resolver previously only changed a
  generic `Error` object's name, which did not satisfy `instanceof TypeError`.
  Existing `TimeoutError` handling is preserved.
- The process-backed two-origin regression proves both CORS-failed Fetch
  promises reject without exposing a response. It verifies that the parent
  sends the seed cookie on each request, accepts the actual-response HttpOnly
  cookies even though CORS rejects them for script, and sends those cookies on
  later authorized requests in order.
- The cookie API retains the seed and both response cookies as HttpOnly, while
  `document.cookie` is empty. No raw cookie or jar crosses IPC, and no
  child-side network retry was introduced.
- The first exact run failed only because the final Fetch Promise was not
  awaited in the test fixture. The fixture now awaits its body; the corrected
  exact regression passes. This is recorded as harness correction, not hidden
  as a product failure.
- The scoped check and exact process regression passed. Documentation-gate
  counts and formatting evidence are recorded below. This does not certify
  full Fetch/CORS or WPT conformance, other platforms, independent review, or
  remote CI.

## Documentation gates

- `scripts/check-release-documentation.py --require-previous-version` passed:
  1,503 Markdown documents, 83 current documents, 63 previous-version hits,
  1,670 semantic audit hits, zero current-claim failures.
- `scripts/check-documentation-depth.py` passed: 93 current guides routed and
  audited; 19 substantive contracts.
- `scripts/check-tui-shortcuts.py` passed: 15 implementation help keys and
  63 documentation markers.
- `scripts/check-documentation-coverage.py` passed: 1,503 Markdown files,
  346 full-product MCP tools (101 browser-only), 17 examples, 22 public
  modules. It reused the existing shared-target binaries explicitly.
- `rustfmt --edition 2024 --check` for the two changed Rust files and
  `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
