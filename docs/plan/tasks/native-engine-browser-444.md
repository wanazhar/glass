# Native engine browser-complete slice 444: XHR MIME override

status: complete
scope: native-engine/xhr-override-mime-type
issue: 40
depends-on: [native-engine-browser-443]

## Objective

Close the XHR response-content-type override boundary for page and
dedicated/SharedWorker realms. `overrideMimeType()` must affect response
projection without rewriting the response headers received from the network.

## Contract

- Page and worker `XMLHttpRequest` expose `overrideMimeType()` with a bounded
  tokenized MIME type/subtype validator. Empty, control-character, oversized,
  or malformed MIME essences select the fallback
  `application/octet-stream` MIME type.
- Calling the method in `UNSENT` or `OPENED` stores the override. Calling it in
  `LOADING` or `DONE` raises `InvalidStateError`.
- `open()` resets the request/response state but preserves the stored override,
  so an override set before `open()` controls that request.
- Page XML/HTML document selection and parsing, plus page/worker Blob MIME
  projection, use the override. `getResponseHeader()` and
  `getAllResponseHeaders()` continue to expose the actual response headers.
- Async and synchronous XHR use the same response-content slot and reset it on
  response replacement, abort, timeout, and error. The worker's bounded Blob
  path remains isolated from the page document parser.

## Implementation

- Added realm-local MIME override validators and `overrideMimeType()` methods
  to the page and worker XHR prototypes.
- Added private override/response-content slots and applied the effective MIME
  to sync and async response projection while retaining wire headers for
  response-header access.
- Added a content-process witness covering invalid and post-DONE state errors,
  pre-`open()` persistence, invalid-MIME fallback, XML document identity/text,
  worker Blob type, and wire `Content-Type` preservation.

## Tradeoffs

- The validator is deliberately bounded and recognizes the MIME essence with
  optional bounded parameter text; a full MIME parameter parser and all XHR
  Web IDL descriptor details remain separate issue #40 work.
- The override changes interpretation only. It does not mutate the native
  loader response, CORS exposure, cache state, or header view.
- Worker XHR does not gain a document parser from this slice; only its Blob
  response MIME projection uses the effective type.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_override_mime_type_controls_response_projection --locked -- --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (22 passed, 0 failed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-444.json` (1094 Markdown documents; current-claim failures=0)
- `python3 scripts/check-documentation-depth.py` (93 current guides, 19 substantive contracts)
- `python3 scripts/check-tui-shortcuts.py` (15 implementation help keys, 63 documentation markers)
- `python3 scripts/check-documentation-coverage.py` (1094 Markdown files, 346 full-product MCP tools, 17 examples, 22 public modules)
- `git diff --check`
