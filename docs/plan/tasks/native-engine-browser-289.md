# Glass native engine browser slice 289: worker Fetch Web IDL objects

Status: completed locally.

## Objective

Give classic dedicated-worker Fetch the object identity needed by ordinary web
code: mutable `Headers`, body-owning `Request`, and body-owning `Response`
instances. Keep the existing shared host loader and bounded response stream as
the only transport boundary.

## Contract

- Worker `Headers` accepts records, pair sequences, and other worker `Headers`
  instances; header names and values are normalized and bounded by the same
  request policy as `fetch()`.
- Worker `Headers` exposes mutation, lookup, iteration, `size`, and
  `instanceof Headers`; host-created response headers are read-only.
- Worker `Request` accepts URL/string-like input and request options, preserves
  method, headers, mode, redirect, credentials, and body metadata, and supports
  `clone()` with independent body ownership.
- Worker `Response` instances preserve status, status text, URL, type, headers,
  `ok`, body identity, `clone()`, static `json()`/`error()`/`redirect()`, and
  one-shot text/JSON/byte/ArrayBuffer/Blob/stream consumers.
- Request and response bodies are disturbed exactly once per object. A request
  body is serialized as bounded bytes before the owner-tagged Fetch command is
  sent; the shared host loader still owns URL/security/cookie/CORS/redirect,
  transport, and response-limit policy.
- Constructor identity survives host round-trips in a worker realm. Repeated
  bootstrap evaluation is re-entrant and does not terminate the content
  process.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker bootstrap now installs stable `Headers`, `Request`, and `Response`
constructors. Header instances retain normalized entry pairs and enforce
bounded mutation; response headers are frozen after construction. Request and
response clones receive independent byte snapshots and stream/body state, while
the original object becomes unusable after its own body is consumed. Fetch
accepts a worker `Request` or URL/string input and sends the serialized body
bytes through the existing worker-owner command.

The implementation remains worker-realm scoped. It does not add a second
network path, loosen request policy, or claim module/shared/service-worker,
XHR, streaming-transport, or complete Web IDL parity.

## Tradeoffs and follow-up

The object layer improves compatibility for ordinary classic-worker Fetch code
without adding transport round trips. Bodies are still host-buffered snapshots,
so this is not demand-driven network streaming. Header and body behavior is a
bounded production slice rather than every Fetch standard edge case; redirects,
CORS, cookies, limits, and transport failures remain owned by the shared Rust
loader. Issue #40 still requires the remaining browser platform surfaces and
the final native/CDP replacement gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine native_content_process_worker_fetch_preserves_binary_request_and_response_bodies -- --nocapture` (1 passed)
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine worker -- --nocapture` (13 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
