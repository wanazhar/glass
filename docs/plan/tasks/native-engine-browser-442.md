# Native engine browser-complete slice 442: XHR read-only state

status: complete
scope: native-engine/xhr-read-only-state
issue: 40
depends-on: [native-engine-browser-441]

## Objective

Close the XHR public-state descriptor gap. Script-visible response and
transport state must be exposed through read-only prototype accessors while
native page and worker owners retain private slots for lifecycle transitions.

## Contract

- Page and dedicated/SharedWorker XHR expose prototype getters for
  `readyState`, `status`, `statusText`, `responseURL`, `response`, and
  `upload`; these attributes have no script setter and are not enumerable.
- Internal async, synchronous, streaming, abort, timeout, and response-type
  paths update private slots without weakening the public read-only contract.
- `responseXML` remains state- and response-type-gated, `responseText` remains
  text-only, `responseType` and `timeout` remain writable only through their
  existing validators, and the same upload object remains observable for the
  request lifetime.
- Page and worker projections retain their existing response values,
  lifecycle events, progress records, cancellation behavior, and cross-realm
  isolation.

## Tradeoffs

- Private slots add a small amount of per-object state and make internal
  assignments explicit, but prevent callers from corrupting transport state
  by assigning to response attributes.
- Accessors intentionally remain configurable to match the existing bounded
  Web IDL emulation and to support bootstrap rehydration; the standard
  non-enumerable prototype shape is preserved.
- Worker `responseXML` stays available only for the supported empty/document
  response-type contract; unsupported worker response types continue to fail
  with `InvalidStateError` rather than manufacturing a document.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_exposes_status_text_and_safe_response_headers --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (21 passed, 0 failed)
- `git diff --check`

The page and worker HTTP witness attempts to mutate every newly read-only
attribute, confirms the values and upload identity remain unchanged, checks
the prototype descriptors, and continues through the response-header/status-
text and synthetic EventTarget dispatch assertions.
