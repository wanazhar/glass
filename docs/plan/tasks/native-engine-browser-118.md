---
id: native-engine-browser-118
scope: glass-browser/native-engine/request-objects
status: done
depends-on: [native-engine-browser-117]
---

# BE-32: bounded Fetch Request objects

## Objective

Allow common page code to construct bounded native `Request` objects and pass
them to `fetch()`, while preserving the existing request validation and
transport owners.

## Contract

- `new Request(url, init)` accepts the bounded GET/POST method set, existing
  `cors`/`no-cors`/`same-origin` mode policy, existing redirect policy, bounded
  headers, credentials value, signal, and existing text/Blob/File/FormData/
  URLSearchParams body inputs.
- `Request` exposes bounded `method`, `url`, `headers`, `mode`, `redirect`,
  `credentials`, `signal`, and body metadata, and `clone()` returns a fresh
  request/header owner with the same bounded init state.
- `fetch(request)` and `fetch(request, overrides)` merge the request init with
  explicit overrides and route the resulting URL and fields through the
  existing Fetch validation, CORS, redirect, abort, and body-transfer owners.
- Existing string-input Fetch behavior and request-header security/bounds
  remain unchanged.
- Full Request body streams, bodyUsed/clone disturbance rules, URL resolution
  parity, cache/referrer/integrity/keepalive/priority/window fields, duplex,
  complete Request/Headers Web IDL identity, and browser-wide Fetch parity
  remain open.

## Ownership and sequence

```text
Request owner -> bounded init/header state -> fetch(request, overrides)
                                           -> existing fetch command/loader
```

Explicit overrides are shallowly merged at the Fetch boundary. Native Request
clones copy the bounded settings and allocate a fresh Headers owner; the
existing transport request remains the sole Rust/content-worker owner.

## Deliberate boundary and tradeoffs

This covers the common Request-object handoff without duplicating network
transport or widening the method/security surface. The bounded `body` field is
metadata for the supported input types, not a claim that Request body streams,
disturbance, or full Web IDL descriptors are implemented.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 768 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  768 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=942; current-claim failures=0
