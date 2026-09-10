---
id: native-engine-browser-101
scope: glass-browser/native-engine/response-headers
status: done
depends-on: [native-engine-browser-100]
---

# BE-22: bounded response-header snapshots

## Objective

Carry a bounded response-header snapshot through the native network bridge so
Fetch and asynchronous XHR can observe multiple response headers without
exposing cookies or bypassing CORS header rules.

## Contract

- The resource loader transfers bounded valid header name/value pairs,
  preserving repeated names for the JavaScript `Headers` view to combine.
- Same-origin responses expose the bounded valid header set except
  `Set-Cookie`/`Set-Cookie2`; cross-origin responses expose only CORS-safelisted
  or `Access-Control-Expose-Headers` names, with wildcard behavior respecting
  credentials.
- The page `Headers` view normalizes names case-insensitively, combines
  duplicate values in order, and provides read-only `get`/`has`/snapshot
  iterator/`forEach` behavior. XHR response-header access uses the same
  snapshot, including `getAllResponseHeaders()`.
- Header count, name, value, and aggregate-byte limits are enforced at the
  resource boundary and revalidated when decoding worker responses. Invalid
  raw header bytes are omitted; trailers, mutation, full Headers identity, and
  complete Web IDL parity remain open.

## Ownership and sequence

```text
HTTP HeaderMap -> origin/CORS exposure filter -> bounded IPC pairs -> Headers/XHR view
```

The resource loader owns origin exposure and bounds. The content process owns
the trusted IPC decode, while JavaScript owns duplicate combination and the
read-only API snapshot. No cookie header is promoted into page-visible data.

## Deliberate boundary and tradeoffs

Transferring UTF-8-valid header values keeps the existing JSON-framed IPC
protocol and gives deterministic API behavior, but it does not preserve
arbitrary invalid header bytes. Repeated values are transferred separately and
combined at the page boundary, which keeps wire semantics visible without
claiming mutable `Headers` identity or trailer support.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, same-origin duplicate-header
behavior, cross-origin CORS exposure, XHR header access, full native-engine
target, strict-Clippy baseline, documentation, and release-truth gates
complete. The checkout is local-only: no push, remote CI, release, tag, or
registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_cors_authorized_cross_origin_get -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline; normalized diagnostics match the 094 baseline exactly (32 pre-existing diagnostics), with no new diagnostic
- `python3 scripts/check-documentation-coverage.py` — 751 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  751 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=924; current-claim failures=0
