# Glass native engine browser slice 290: worker XMLHttpRequest

Status: completed locally.

## Objective

Expose asynchronous XMLHttpRequest in classic dedicated workers so common
worker applications can use the existing native Fetch owner without requiring
CDP or a second network implementation.

## Contract

- Worker XMLHttpRequest has stable constructor identity and the standard
  UNSENT, OPENED, HEADERS_RECEIVED, LOADING, and DONE constants.
- Asynchronous open(), setRequestHeader(), send(), abort(),
  addEventListener(), removeEventListener(), response-header lookup, and
  load/error/abort/timeout/loadend events are available.
- Text, ArrayBuffer, and Blob response types preserve the existing worker byte
  boundary and constructor identity.
- Request methods, headers, bodies, credentials, and bounded timeout values
  use the existing worker Fetch command and shared Rust resource loader.
- Ready-state transitions and late-response suppression remain observable after
  multiple content-process host turns. A default timeout of zero means no
  timeout; positive values are bounded by the native XHR limit.

## Path

- crates/glass-browser/src/browser/native_engine/javascript.rs
- crates/glass-browser/tests/native_engine.rs
- docs/plan/README.md
- docs/plan/analysis/native-engine.md
- docs/architecture/native-engine.md

## Implementation

The worker bootstrap installs a stable XMLHttpRequest constructor with
request-local state and event listeners. Its asynchronous request path delegates
to the worker fetch command, preserves normalized headers and request bytes,
then projects response metadata and body conversions back onto the XHR object.
The shared worker response error payload now identifies bounded request
timeouts, allowing ontimeout to remain distinct from network onerror.
Abort invalidates the request token and suppresses late callbacks while the
host request is completing.

This slice intentionally does not create a new transport or content-process
protocol. It remains limited to asynchronous XHR and the already supported
text/byte/Blob body forms.

## Tradeoffs and follow-up

Reusing Fetch keeps security, cookies, CORS, redirects, timeout, and response
limits in one owner and avoids duplicate network policy. The host request is
still not cancelled when a worker calls abort(); the worker suppresses its late
result, so transport cancellation remains a later ownership slice. Upload
progress, synchronous XHR, XML response parsing, richer progress events, and
complete XHR/Web IDL parity remain Issue #40 work, as do module/shared/service
workers and the final native/CDP replacement gates.

## Verification

- cargo fmt --all -- --check
- cargo check --quiet -p glass-browser --features native-engine --tests --locked
- cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine native_content_process_worker_exposes_xhr_fetch_bridge -- --nocapture (1 passed)
- cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine worker -- --nocapture (14 passed)
- git diff --check

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
