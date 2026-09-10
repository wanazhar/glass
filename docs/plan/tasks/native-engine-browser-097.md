---
id: native-engine-browser-097
scope: glass-browser/native-engine/fetch-request-headers
status: done
depends-on: [native-engine-browser-096]
---

# BE-18: bounded Fetch request-header dictionaries

## Objective

Allow ordinary native `fetch()` callers to send a bounded request-header
dictionary while preserving browser-owned security headers and CORS policy.

## Contract

- Fetch accepts a bounded plain-object request-header dictionary and normalizes
  names case-insensitively; duplicate case variants combine with `, `.
- Header names and values are validated in both the JavaScript realm and the
  Rust network owner, with bounded count, name, value, and aggregate limits.
- Forbidden/internal names such as `Cookie`, `Origin`, `Referer`, `Host`,
  `Content-Length`, `Sec-*`, and `Proxy-*` fail closed; Glass retains ownership
  of cookies, origin, referrer, and transport framing.
- Same-origin requests transfer supported custom headers to the HTTP owner.
  Cross-origin requests include every non-safelisted header in the sorted
  `Access-Control-Request-Headers` preflight list and require matching server
  authorization.
- Authorization is removed when a manual redirect crosses origins. Existing
  method, body, content type, credentials, CORS, CSP, redirect, and response
  limits remain authoritative.
- Full `Headers` constructor/identity and mutation parity, duplicate-value
  list semantics beyond bounded combination, response-header exposure, raw
  headers, trailers, and complete Fetch/Web IDL parity remain open.

## Ownership and sequence

```text
page dictionary -> normalized command map -> Rust validation -> CORS preflight -> HTTP request
```

JavaScript performs early normalization and fail-fast validation. The
content-process command decoder and Rust resource loader revalidate the map,
apply internal-header policy, compute CORS-safelisted names, and own the
preflight and wire transfer.

## Deliberate boundary and tradeoffs

Using a normalized map keeps IPC bounded and avoids introducing a third crate
or a second header representation. The tradeoff is that this is not the full
Fetch `Headers` object: duplicate values are combined into one bounded string,
the constructor is not exposed, and response headers remain on the earlier
single-content-type surface.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, same-origin and cross-origin
header behavior, full native-engine target, documentation, and release-truth
gates complete. One unrelated scheduler test had a transient timing failure
on an earlier full run; its isolated rerun passed, followed by a clean full
382-test run. The checkout is local-only: no push, remote CI, release, tag, or
registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_cross_origin_post_after_cors_preflight -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; normalized diagnostics match the 094 baseline and no new diagnostic was introduced
- `python3 scripts/check-documentation-coverage.py` — 747 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  747 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=920; current-claim failures=0
