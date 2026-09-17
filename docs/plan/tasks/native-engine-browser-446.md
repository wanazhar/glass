# Native engine browser-complete slice 446: XHR open admission

status: complete
scope: native-engine/xhr-open-admission
issue: 40
depends-on: [native-engine-browser-445]

## Objective

Align page and dedicated/SharedWorker `XMLHttpRequest.open()` with the
method, URL, overload, and validation-order portions of the browser contract
while retaining the existing bounded native loader.

## Normative reference

The method, URL, `async`, credential-overload, and synchronous-configuration
rules follow the current [WHATWG XMLHttpRequest Standard](https://xhr.spec.whatwg.org/),
sections 3.5.1 and 3.5.3.

## Contract

- `open()` converts the method to text and upper case, rejects an invalid HTTP
  method token with `SyntaxError`, and rejects `CONNECT`, `TRACE`, and
  `TRACK` with `SecurityError`. The product's existing bounded method set
  remains explicit: other valid but unsupported methods still raise the
  bounded product `TypeError` rather than being silently downgraded.
- Page and worker XHR resolve relative URL strings and native URL objects
  against the owning document or worker URL. The internal request URL is
  absolute before it enters the native Fetch or synchronous-XHR payload.
- The optional username and password arguments replace credentials only for
  authority-bearing URLs, using the bounded URL credential encoding already
  used by the native URL implementation. Omitted or null arguments preserve
  parsed credentials, and credential arguments do not bypass loader-owned
  cookie, CORS, or credential policy.
- `async` uses Boolean conversion, with an omitted argument defaulting to
  `true`. Page synchronous XHR retains its `timeout` and typed-response
  configuration checks; worker synchronous XHR retains its supported typed
  settings.
- Method, URL, and synchronous-configuration validation completes before an
  active response controller or reader is cancelled. A rejected reopen leaves
  the prior request ownership intact.

## Implementation

- Added realm-local method and URL admission helpers to the page and worker
  bootstrap paths, including DOMException names and bounded credential URL
  reconstruction.
- Extended both XHR `open()` methods to accept the username/password overload,
  use absolute resolved URLs, apply correct Boolean conversion, and cancel
  the previous response only after all new-request validation succeeds.
- Extended the existing page/worker response-type witness with forbidden and
  malformed method probes, numeric `async` conversion, relative URL requests,
  URL-object input, and credential-overload encoding assertions.

## Tradeoffs

- This slice keeps the deliberately bounded native HTTP method and URL
  surface. Full method admission, complete URL parser/error parity, and
  complete XHR/Web IDL descriptor parity remain issue #40 gates.
- URL credential reconstruction is observable only through the resulting
  request URL; the native loader remains authoritative for whether credentials
  are sent and for all origin, cookie, CORS, redirect, cache, and policy
  decisions.
- Validation-before-cancellation improves reuse safety without introducing a
  second transport owner or changing response streaming, upload, or
  synchronous-loader boundaries.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_response_type_is_canonical_and_state_aware --locked -- --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (22 passed, 0 failed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-446.json` (current-claim failures=0)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
