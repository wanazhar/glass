---
id: native-engine-browser-005
scope: glass-browser/native-engine/content-process-resource-transfer
status: done
depends-on: [native-engine-browser-004]
---

# BE-01d: child-owned bounded resource transfer

## Objective

Move bounded external HTTP(S) document acquisition across the process
boundary. The `glass-native-content-worker` helper now invokes the same
`NativeResourceLoader` contract for URL validation, redirects, response MIME,
response-size limits, timeout, and UTF-8 decoding. The parent receives only a
bounded transfer and remains responsible for native DOM construction and the
existing failure-atomic commit path.

This is the first resource-ownership slice, not hostile-content completion.
The child does not yet own HTML parsing, DOM mutation, layout, painting,
JavaScript, storage, or the full security policy. Those later slices must move
the relevant state and APIs behind the process boundary rather than treating a
source transfer as isolation.

## Contract

- `load` is a versioned, request-ID-correlated IPC command carrying only a
  validated HTTP(S) URL and the caller's bounded document-byte quota.
- The child reuses `NativeResourceLoader` so parent and child do not develop
  divergent redirect, HTML MIME, body-size, credential, or UTF-8 behavior.
- The child returns the final URL and base64-encoded HTML body in the existing
  1 MiB frame limit. The configured document limit is at most 256 KiB, leaving
  bounded wire overhead for the transfer metadata.
- The parent validates the returned request ID, URL scheme, final URL syntax,
  body encoding, UTF-8, origin, and byte quota before parsing or committing.
- Child-side HTTP rejection is returned as a typed worker error without body
  or credential logging. Broken pipes, malformed responses, invalid transfer
  data, and load deadlines poison and terminate the child; the next external
  navigation creates a fresh process rather than reusing a known-bad one.
- The parent applies a 30-second IPC load deadline in addition to the loader's
  request timeout. Local/data/fixture resources retain the deterministic
  in-process path and do not spawn a content process.
- The browser backend still has no Chromium/CDP fallback. A failed child load
  leaves the current document untouched because parsing and commit happen only
  after the transfer succeeds.

## Tradeoffs and missed behavior

- Reusing the loader keeps policy and limits consistent, but base64 IPC
  temporarily duplicates the document in child bytes, encoded response, and
  parent bytes. A later binary frame or shared-memory transfer can reduce
  copies without weakening the byte quota.
- Sequential request/response IPC is easy to reason about and prevents
  response confusion, but one content process cannot service concurrent loads
  yet. Cancellation currently terminates the child at the deadline rather
  than running a fine-grained cancellable fetch protocol.
- HTML source still crosses back to the parent and the parent still parses it;
  hostile markup is therefore not isolated from the parser or DOM implementation.
  The next process gate must move document construction and its crash boundary
  into the child before promotion.
- The existing Rustls/reqwest behavior remains the network policy. Charset
  sniffing, cookies/cache, CORS/CSP, mixed-content/referrer policy,
  permissions, subresources, service workers, and OS sandboxing remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/bin/glass_native_content_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The resource-transfer slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --locked`
- `cargo check -p glass-browser --features native-engine --test native_engine --locked`
- `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_enforces_child_owned_document_limit -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_uses_explicit_local_constructor -- --nocapture`

The affected library and integration checks passed without warnings. The
real helper-backed HTTP navigation passed 1/1, child-owned limit enforcement
passed 1/1, and the local/no-process session regression passed 1/1. Full
content-process parsing, supervisor recovery, OS sandboxing, cross-platform
packaging, WPT, security, and production promotion gates remain open.
