# Native engine Slice 863 review

- **Task:** `native-engine-browser-863`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. Both captured-load and ordinary ServiceWorker Fetch requests preserve
parent-owned cookie handling when actual responses fail CORS.

## Review notes

- The process-backed two-origin regression now distinguishes the two broker
  branches. A ServiceWorker Fetch during captured navigation uses the
  captured-load marker; a nested Fetch from the FetchEvent caused by explicit
  page `fetch('/worker-probe')` uses the ordinary parent broker.
- Both API responses are actual credentialed GET responses with wildcard
  `Access-Control-Allow-Origin`, `Access-Control-Allow-Credentials: true`, and
  separate HttpOnly cookies. The fixture rejects preflight, unexpected order,
  and extra requests. The ServiceWorker observes an actual `TypeError` for
  each and receives no response status or body.
- A later authorized worker request and direct page request each carry both
  response cookies. The parent cookie API retains the HttpOnly values, while
  `document.cookie` remains empty. No runtime change, child jar, raw cookie
  headers in IPC, or direct child network fallback was introduced.
- The scoped check and exact test passed. This does not certify background
  ServiceWorker Fetch, complete Fetch/CORS or WPT behavior, other platforms,
  independent review, or remote CI.

## Documentation gates

- `python3 scripts/check-release-documentation.py
  --require-previous-version` passed: 1,509 Markdown documents, 83 current,
  63 previous-version hits, 1,672 semantic audit hits, zero current-claim
  failures.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides
  routed and audited; 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` passed: 15 implementation help
  keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` passed using the existing
  shared-target `glass` and `glass-browser` binaries: 1,509 Markdown files,
  346 full-product MCP tools (101 browser only), 17 examples, and 22 public
  modules.
- `rustfmt --edition 2024 --check
  crates/glass-browser/tests/native_engine.rs` and `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
