---
id: native-engine-browser-005
scope: glass-browser/native-engine/content-process-resource-transfer
status: done
depends-on: [native-engine-browser-004]
---

# BE-01d: child-owned bounded resource transfer

## Objective

Move bounded external HTTP(S) document acquisition and HTML tree construction
across the process boundary. The `glass-native-content-worker` helper now
invokes the same `NativeResourceLoader` contract for URL validation, redirects,
response MIME, response-size limits, timeout, and UTF-8 decoding, then parses
the document into a bounded typed DOM snapshot. The parent reconstructs that
snapshot and retains the existing failure-atomic commit path.

This is the first resource-ownership slice, not hostile-content completion.
The child does not yet own stylesheet computation, DOM mutation, layout, painting,
JavaScript, storage, or the full security policy. Those later slices must move
the relevant state and APIs behind the process boundary rather than treating a
source transfer as isolation.

## Contract

- `load` is a versioned, request-ID-correlated IPC command carrying only a
  validated HTTP(S) URL and the caller's bounded document-byte quota.
- The child reuses `NativeResourceLoader` so parent and child do not develop
  divergent redirect, HTML MIME, body-size, credential, or UTF-8 behavior.
- The child returns the final URL and a base64-encoded typed DOM snapshot in a
  bounded 4 MiB frame. The configured source document limit is at most 256 KiB
  and the decoded snapshot is capped at 2 MiB, leaving bounded wire overhead.
- The parent validates the returned request ID, URL scheme, final URL syntax,
  snapshot encoding, origin, node links, attributes, and configured quotas
  before reconstructing or committing.
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

- Reusing the loader keeps policy and limits consistent, but JSON/base64 IPC
  temporarily duplicates the parsed document in child memory, encoded
  response, and parent memory. A later binary frame or shared-memory transfer
  can reduce copies without weakening the quotas.
- Sequential request/response IPC is easy to reason about and prevents
  response confusion, but one content process cannot service concurrent loads
  yet. Cancellation currently terminates the child at the deadline rather
  than running a fine-grained cancellable fetch protocol.
- The child owns HTML tokenization/tree construction, but the parent rebuilds
  the bounded arena and still reparses stylesheet sources. Hostile CSS/parser
  behavior is therefore not fully isolated. The next process gate must move
  computed style and document mutation ownership behind the child boundary.
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
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_rejects_malformed_html_before_parent_commit -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_uses_explicit_local_constructor -- --nocapture`

The affected library and integration checks passed without warnings. The
real helper-backed HTTP navigation passed 1/1, child-owned limit enforcement
passed 1/1, child parser rejection/failure-atomicity passed 1/1, and the
local/no-process session regression passed 1/1. Full
computed-style/content execution, supervisor recovery, OS sandboxing,
cross-platform packaging, WPT, security, and production promotion gates
remain open.
