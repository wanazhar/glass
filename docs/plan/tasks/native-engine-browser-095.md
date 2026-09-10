---
id: native-engine-browser-095
scope: glass-browser/native-engine/fetch-response-headers
status: done
depends-on: [native-engine-browser-094]
---

# BE-16: bounded fetch response headers

## Objective

Replace the response's single ad-hoc content-type lookup with a bounded
read-only header view that ordinary fetch callers can inspect using the common
`Headers` methods.

## Contract

- `Response.headers` supports case-insensitive `get` and `has` for the
  content-type value already transferred by the native network boundary.
- `entries`, `keys`, `values`, `forEach`, and the default iterator return
  bounded snapshots in normalized lower-case header-name order.
- Response headers are read-only; no mutating or custom request-header path is
  introduced by this slice.
- The existing `Response` status, URL, body variants, CORS, origin, and
  response-size policies remain unchanged.
- Multiple header values, `Headers` constructor identity, raw header exposure,
  request header dictionaries, forbidden-header rules, trailers, and complete
  Fetch/Headers Web IDL parity remain open.

## Ownership and sequence

```text
Rust/content-worker content type -> read-only response header snapshot -> page lookup/iteration
```

The Rust/content-worker network owner remains responsible for header parsing
and transfer. JavaScript owns the bounded normalized snapshot and read-only
iteration surface.

## Deliberate boundary and tradeoffs

Using the one validated content type already present in the response payload
avoids widening the IPC schema and prevents accidental exposure of unvalidated
headers. The tradeoff is that applications requiring cache, authentication,
set-cookie, multiple-value, or trailer inspection still need the future full
header contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, worker header behavior, full
native-engine target, documentation, and release-truth gates complete. The
checkout is local-only: no push, remote CI, release, tag, or registry-
publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 745 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 745 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=918; current-claim failures=0
