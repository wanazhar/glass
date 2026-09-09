---
id: native-engine-browser-015
scope: glass-browser/native-engine/child-fetch-caller
status: done
depends-on: [native-engine-browser-014]
---

# BE-02e: bounded child-owned GET/fetch caller

## Objective

Give the shared network policy a real caller without introducing a partial
JavaScript API that could be mistaken for Fetch/Web IDL support. The native
kernel exposes one bounded GET operation, and the request runs in the existing
sandboxed content worker that already owns the external document and session
state.

## Contract

- `NativeEngine::fetch_async` accepts a relative or absolute HTTP(S) target
  and an explicit credential flag. It is available only while a running native
  engine owns an external HTTP(S) document.
- The parent sends a typed fetch request through the existing framed worker
  protocol. The child resolves the URL, applies `connect-src`/`default-src`,
  rejects credentials in URLs, applies HTTPS mixed-content policy, follows at
  most eight validated redirects, and keeps request/response state inside the
  child.
- Cross-origin requests send the serialized document origin in `Origin` and
  expose response bytes only when ACAO authorizes the origin. A wildcard is
  accepted only for non-credentialed requests; credentialed requests require
  an exact ACAO match and `Access-Control-Allow-Credentials: true`. Same-origin
  responses do not need CORS headers.
- The operation is GET-only, sends no custom headers or request body, returns
  any final HTTP status as a typed response, and caps response bytes at the
  configured native document limit. Content type is metadata only; the body
  remains bounded bytes and is never logged by the worker.
- Cookies are sent only when the explicit credential flag is enabled and are
  committed only after the final CORS/body validation succeeds. The operation
  does not use the document cache, mutate the DOM, or advance the document
  revision.
- Malformed IPC, invalid transfers, timeouts, and broken pipes retain the
  existing typed worker-failure behavior. Policy/network rejection is returned
  as a typed fetch error without silently retrying through CDP.

## Deliberate boundary and tradeoffs

- This is a native-kernel primitive, not a `BrowserBackend` fetch operation and
  not a `window.fetch` implementation. JavaScript realms, Web IDL promises,
  request methods/headers/bodies, response streams, CORS preflights, service
  workers, and script/module execution remain open BE-02/BE-04 work.
- A GET-only surface avoids preflight ambiguity and keeps the first readable
  cross-origin response easy to audit. It will need a versioned request model
  before the JS bridge can support ordinary web applications.
- The response limit reuses the native document quota to keep IPC bounded; a
  later profile may define separate body/stream quotas without weakening the
  worker boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The batch was checked as one coherent unit:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture` — 13/13
- `git diff --check`

The next gate is the JavaScript/Web IDL boundary and the broader Fetch request
model. This operation alone does not claim browser-complete networking.
