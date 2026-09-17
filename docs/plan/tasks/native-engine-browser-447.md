# Native engine browser-complete slice 447: Fetch/Request URL ownership

status: complete
scope: native-engine/fetch-request-url-ownership
issue: 40
depends-on: [native-engine-browser-446]

## Objective

Make page and dedicated/SharedWorker `Request` and `fetch()` inputs enter the
native loader with one canonical absolute URL owned by the current document or
worker realm. Relative strings, native `URL` objects, and existing native
`Request` objects must follow the same bounded URL owner before dispatch.

## Normative reference

The request/response and URL ownership model follows the current [WHATWG Fetch
Standard](https://fetch.spec.whatwg.org/) and [WHATWG URL
Standard](https://url.spec.whatwg.org/). Glass continues to enforce its
explicit bounded method, scheme, policy, and resource limits at the native
loader boundary.

## Contract

- Page `Request` and `fetch()` resolve string, `URL`, and `Request` inputs
  against the owning document URL before the request enters the host command
  or loader. Dedicated/SharedWorker inputs resolve against the worker's own
  absolute script URL.
- The resulting Request URL is canonical and absolute, including path
  normalization and the URL implementation's existing authority/credential
  serialization. Passing an already-native absolute URL remains stable.
- The old invalid-input boundaries remain intact: page and worker `Request`
  constructors reject unsupported input types synchronously, while worker
  `fetch()` turns URL-resolution failures into rejected promises. URL
  resolution does not bypass the existing HTTP(S)/fixture, CORS, CSP, mixed
  content, cookie, redirect, cache, or credential policy owners.
- Request cloning preserves the canonical URL and existing body/header
  ownership. The host receives the resolved URL rather than reinterpreting a
  realm-relative string in a different process or owner.

## Implementation

- Added page and worker realm-local Request URL helpers that recognize native
  URL objects, stringify ordinary supported inputs, resolve against the realm
  owner, and return the bounded canonical URL projection.
- Routed page and worker `Request` construction plus page and worker Fetch
  dispatch through those helpers without changing body, header, abort, stream,
  or loader policy code.
- Extended content-process and inline-worker witnesses with relative Request
  construction and relative Fetch resolution, including the canonical URL
  observed in the returned response and worker Request.
- Restored the inline worker response-stream cached-body branch that was
  accidentally narrowed during the synchronous-XHR slice; fixture streams now
  retain the shared 8 KiB transport contract.
- Updated the Fetch response-header witness to match the existing sorted
  exposed-header contract rather than wire insertion order.

## Tradeoffs

- Canonicalizing at the JavaScript realm boundary keeps page/worker URL
  ownership deterministic across inline and content-process execution, at the
  cost of one bounded URL parse before every Request/Fetch dispatch.
- The helper intentionally reuses the existing native URL parser and loader;
  this slice does not claim complete URL Web IDL or all Fetch method/scheme
  parity.
- The inline fixture stream repair preserves observable demand-driven chunking
  and avoids buffering regressions, while fixture URL policy remains separate
  from the HTTP(S) network security boundary.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_fetch_streams_fixture_response_body --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_exposes_bounded_script_fetch_promises --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine fetch --locked -- --test-threads=1 --nocapture` (37 passed, 0 failed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-447.json` (current-claim failures=0)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
