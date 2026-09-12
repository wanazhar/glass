# Glass native engine browser slice 249: common HTTP methods

Status: completed locally.

## Objective

Extend the native network owner so ordinary web applications can use the
common HTTP request methods beyond GET and POST. The JavaScript surface,
content-worker wire, CORS policy, redirect policy, and reqwest transport must
agree on one method contract.

## Contract

- Native `fetch`, `Request`, and asynchronous XHR accept `GET`, `HEAD`,
  `POST`, `PUT`, `PATCH`, `DELETE`, and `OPTIONS`.
- GET and HEAD reject request bodies. The shared loader also rejects a content
  type attached to either bodyless method, so the transport cannot silently
  turn invalid input into a different request.
- Cross-origin CORS requests preflight non-simple methods. Cross-origin
  no-cors requests reject non-simple methods, while simple GET/HEAD/POST
  requests retain the existing direct path and response filtering.
- 301/302/303 redirects rewrite a non-GET/HEAD request to GET and discard its
  body/content type. 307/308 redirects preserve the method and body.
- The content worker serializes the method from the shared typed enum; the
  document navigation path remains restricted to GET and POST.

## Implementation

`NativeNavigationMethod` now owns the supported method names, reqwest method
conversion, bodyless classification, and document-navigation classification.
The fetch parser and content-process load wire use that owner rather than
duplicating string matches. The shared loader builds all requests through one
method-aware reqwest builder, applies the method to CORS preflight cache keys,
and performs redirect method rewriting at the existing redirect boundary.

The JavaScript bootstrap uses one bounded method list for `fetch`, `Request`,
and XHR. Typed array XHR bodies use the existing binary request-body path.
The native integration fixture runs each request in its own evaluation
transaction, observes HEAD/PUT/PATCH/DELETE/OPTIONS at the server, checks
the four non-simple preflights, and verifies the body received for each
body-bearing method.

## Tradeoffs and follow-up

This deliberately covers the common application methods without pretending
to implement every HTTP token or every Fetch standard detail. CONNECT and
TRACE remain rejected because they require additional browser security and
proxy semantics. The implementation copies bounded bodies and keeps the
existing one-request content-process resolution boundary, which preserves
deterministic ownership but does not yet provide streaming uploads or
parallel page fetch scheduling. Full preflight cache semantics, service-worker
interception, complete Fetch Web IDL/body-stream behavior, and browser-wide
conformance remain issue #40 promotion work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_common_http_methods_with_cors_preflight --locked` (1 passed)
- `git diff --check`

The focused test is local evidence only. Remote CI, push, release, registry
publication, and final native-engine parity claims remain unclaimed for this
local-only checkpoint.
