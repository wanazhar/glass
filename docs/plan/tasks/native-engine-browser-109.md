---
id: native-engine-browser-109
scope: glass-browser/native-engine/fetch-modes
status: done
depends-on: [native-engine-browser-108]
---

# BE-29: bounded Fetch mode policy and opaque responses

## Objective

Carry the Fetch `mode` option through the existing script/content-process
request owner and expose the bounded cross-origin `no-cors` response contract
without creating a second network or response implementation.

## Contract

- Fetch defaults to `cors` and accepts `cors`, `no-cors`, and `same-origin`.
  Unsupported mode values fail as a JavaScript `TypeError` before dispatch.
- `same-origin` rejects a cross-origin target before network I/O. `cors`
  retains the existing Origin, preflight, response-authorization, and exposed
  response-header checks.
- A cross-origin `no-cors` request rejects non-safelisted request headers and
  content types before network I/O. Safelisted GET/POST requests omit the
  `Origin` header and may reach the target without CORS response headers.
- A successful cross-origin `no-cors` response is an opaque response: its
  JavaScript type is `opaque`, status is `0`, `ok` is `false`, URL is empty,
  headers are empty, and body-reading methods reject with `TypeError`. The raw
  response body and headers are not transferred into the script realm.
- Same-origin `no-cors` responses retain the ordinary bounded response shell.
  Service-worker routing, private-network access, streaming bodies, redirect
  parity, and complete Fetch/Response Web IDL parity remain open.

## Ownership and sequence

```text
Fetch options -> mode validation -> typed content-process request
  -> bounded loader policy -> opaque/basic response projection
```

The existing resource loader remains the only network owner. The content
process transfers only the typed mode and the bounded response projection;
JavaScript owns the final response shell and hides opaque data.

## Deliberate boundary and tradeoffs

This slice makes the common mode distinction observable while preserving a
small deterministic policy surface. Opaque responses permit the request but
do not expose data to scripts, and no-cors request validation is intentionally
fail-closed. That leaves full Fetch mode, redirect, service-worker,
private-network, streaming, and Web IDL parity outside the claim.

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

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_opaque_no_cors_response -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 759 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  759 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=933; current-claim failures=0
