---
id: native-engine-browser-002
scope: glass-browser/native-engine/http-document-navigation
status: done
depends-on: [native-engine-browser-001]
---

# BE-02a: bounded external HTTP(S) document navigation

## Objective

Open the first executable BE-02 network boundary for the browser-complete
program: native-only navigation to an external HTTP(S) HTML document. The
batch must pass the response through the existing native document owner and
backend dispatcher without starting Chromium, connecting to CDP, or silently
falling back.

This is a useful external-navigation slice, not the BE-02 milestone. It does
not claim a secure general browser network stack, subresources, scripts,
cookies, cache, CORS/CSP, service workers, permissions, or browser parity.
The BE-01 asynchronous worker/content-process boundary remains a promotion
dependency for hostile-content use.

## Contract

- `NativeEngineConfig` accepts HTTP and HTTPS initial URLs in addition to the
  existing `about:blank`, `data:text/html`, and registered `fixture://` URLs.
- Native session initialization and navigation use an async resource path for
  HTTP(S), while local resources retain deterministic in-process behavior.
- The loader uses the existing `reqwest`/Rustls dependency, follows at most
  eight redirects, applies a 30-second request timeout, rejects userinfo
  credentials, and never logs response bodies or request data.
- A response must be successful and `text/html` or `application/xhtml+xml`
  when a content type is supplied. The body is streamed into the configured
  document limit and must currently decode as UTF-8.
- The final redirected URL is retained, including the caller's fragment, and
  HTTP(S) resources receive a normalized tuple origin. Local/data/fixture
  resources remain opaque.
- The native backend exposes the result through the existing navigation and
  compact-evidence contract. Errors map to typed connection, configuration,
  unsupported-content, or limit failures.
- No subresource fetch, JavaScript execution, HTML5 parser replacement, or
  security-policy decision is implied by this batch.

## Tradeoffs and missed behavior

- Building a request client per document keeps the loader cloneable and avoids
  introducing shared mutable transport state, but connection pooling and
  cache reuse are deferred.
- UTF-8-only decoding keeps the parser boundary explicit; HTTP charset
  negotiation and HTML encoding sniffing remain a required BE-02 follow-up.
- Redirects are bounded and HTTP(S)-only, but origin transition policy,
  mixed-content rules, cookies, CORS, CSP, TLS policy configuration, and site
  isolation are not implemented yet.
- The engine still runs in-process. It is not suitable for untrusted remote
  content until the content-process, sandbox, quota, and crash-recovery gates
  pass.

## Paths

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/origin.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The affected native feature was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_ -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine lifecycle_state_is_terminal_after_close -- --nocapture`

The HTTP integration test passed 1/1. The local/native session regression
filter passed 2/2, the lifecycle test passed 1/1, formatting passed, and the
affected library check passed with no warnings. Full workspace, WPT,
cross-platform, process-isolation, security, real-site, and production
promotion gates remain open.

