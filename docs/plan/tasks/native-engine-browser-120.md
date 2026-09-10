---
id: native-engine-browser-120
scope: glass-browser/native-engine/response-constructors
status: done
depends-on: [native-engine-browser-119]
---

# BE-34: bounded Response identity and constructors

## Objective

Complete the bounded Request/Response object pair with native Response
identity and common constructors without widening the transport boundary.

## Contract

- Fetch responses are `instanceof Response` and expose bounded `statusText`
  alongside their existing metadata, body readers, stream, and `clone()`.
- `new Response(body, init)` accepts bounded strings, URLSearchParams, Blob,
  ArrayBuffer, and typed-array bodies with bounded status/statusText/headers;
  null/omitted bodies expose `body === null`.
- `Response.json(data, init)` serializes bounded JSON and supplies a default
  JSON content type; `Response.error()` exposes the filtered error shell; and
  `Response.redirect(url, status)` supports the bounded standard redirect
  status set and Location header.
- Constructed and fetched responses reuse the existing independent body
  readers, bounded stream owner, response-header projection, and clone owner.
- Stream-body constructor input, bodyUsed/disturbance, full redirect/error
  internals, trailers, shared tee/backpressure, complete Response Web IDL
  identity, and browser-wide Fetch parity remain open.

## Ownership and sequence

```text
Response constructor/static -> bounded response payload -> Response owner
Fetch payload ------------------------------^          -> body/header owners
```

Constructed responses stay in the JavaScript realm. Fetched responses continue
to originate from the Rust/content-worker payload and only gain the shared
Response prototype/metadata projection.

## Deliberate boundary and tradeoffs

This makes common Response factories and identity checks work for scripts and
test harnesses while retaining the deliberately independent body readers from
the preceding slices. Stream input, body disturbance and complete factory
semantics remain explicit future gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_response_objects_expose_bounded_constructors_and_identity -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_opaque_no_cors_response -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 770 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  770 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=944; current-claim failures=0
