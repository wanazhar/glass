# Native engine browser-complete slice 439: XHR request headers and identity

status: complete
scope: native-engine/xhr-request-headers
issue: 40
depends-on: [native-engine-browser-438]

## Objective

Close the next XHR request-side fidelity gap without introducing a second
header-policy owner. Page and worker XMLHttpRequest instances must expose the
same bounded request-header validation, standard interface constants, and
send-state transitions while continuing to route requests through the native
resource loader.

## Contract

- Page and worker XHR accept bounded non-forbidden request headers through
  `setRequestHeader()`, combine duplicate values in insertion order, and carry
  them to the existing Fetch/resource-loader policy.
- Invalid names, forbidden names, control characters, and over-limit values
  fail before a request is admitted; Content-Type remains separately tracked
  when the body chooses its own MIME type.
- `XMLHttpRequest.UNSENT`, `OPENED`, `HEADERS_RECEIVED`, `LOADING`, and `DONE`
  are available on the page and worker interfaces and their prototypes.
- `send()` marks the request pending only after synchronous body validation,
  rejects duplicate sends or header mutation while pending, and restores the
  reusable state after success, timeout, error, abort, or synchronous
  validation failure.
- Response/header policy, body limits, CORS, cookies, and redirects remain
  owned by `NativeResourceLoader`; existing XHR event delivery is unchanged.

## Tradeoffs

- Reusing the existing bounded `HeadersNative`/`WorkerHeadersNative` validators
  makes XHR more useful without duplicating forbidden-header or size policy.
- Header values remain normalized and combined rather than preserving raw
  wire spelling; raw invalid header bytes and full XHR Web IDL descriptor
  parity remain separate issue #40 gates.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_page_and_worker_support_sync_xhr --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr_streams_response_progress --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (20 passed, 0 failed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-439.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

The HTTP witness checks custom page/worker request headers, duplicate-value
combination, interface/prototype constants, retry after a rejected stream
body, and the existing response projections. The worker-stream witness was
paced at 50 ms between server writes so the intermediate-chunk assertion does
not depend on nondeterministic TCP coalescing; the final serial XHR group is
20/20 green.
