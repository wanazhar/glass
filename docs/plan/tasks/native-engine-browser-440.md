# Native engine browser-complete slice 440: XHR response metadata

status: complete
scope: native-engine/xhr-response-metadata
issue: 40
depends-on: [native-engine-browser-439]

## Objective

Close the next XHR response-side fidelity gap without creating a second
network or header-policy owner. Native HTTP reason phrases and response-header
reads must survive the local, content-process, worker, Service Worker, Fetch,
and synchronous-XHR boundaries with browser-shaped filtering and deterministic
serialization.

## Contract

- HTTP responses preserve their canonical reason phrase as `statusText`; the
  bounded synthetic fixture response uses the deterministic `OK` phrase.
- Page and worker Fetch/XHR payloads, synchronous XHR payloads, Service Worker
  responses, and the content-process fetch wire carry that status text without
  manufacturing a numeric fallback.
- Response header names are validated as response names rather than request
  names. Valid names are case-insensitive; malformed names raise `SyntaxError`.
- `Set-Cookie` remains unreadable to page and worker script, while permitted
  duplicate response headers combine in order and `getAllResponseHeaders()`
  returns a normalized, lexicographically ordered list.
- Existing loader-owned CORS exposure, credentials, cookies, redirect, cache,
  response-size, and content-process ownership rules remain unchanged.

## Tradeoffs

- Canonical HTTP reason phrases are stable and safe to transport, but they do
  not preserve a server's non-canonical reason phrase; the native loader does
  not expose raw status-line bytes.
- Sorting the script-visible response-header list makes cross-transport output
  deterministic and matches the browser header-list contract, but it changes
  older insertion-order witnesses.
- Response-header sanitization remains bounded and fail-closed at the script
  projection. Raw invalid wire-header byte preservation and complete XHR/Web
  IDL descriptor parity remain separate issue #40 gates.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_exposes_status_text_and_safe_response_headers --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (21 passed, 0 failed)
- `git diff --check`

The HTTP witness covers page and dedicated-worker XHR status text for 201 and
202 responses, duplicate response-header combination, sorted response-header
serialization, `Set-Cookie` filtering, and invalid response-header names.
