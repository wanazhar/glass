---
id: native-engine-browser-106
scope: glass-browser/native-engine/cors-preflight-cache
status: done
depends-on: [native-engine-browser-105]
---

# BE-27: bounded CORS preflight cache

## Objective

Cache validated cross-origin CORS preflight decisions for repeated Fetch and
XHR requests without introducing another network or process owner.

## Contract

- A successful preflight is cacheable only when `Access-Control-Max-Age` is a
  valid positive integer. Failed, invalid, missing, and zero-age responses are
  not cached.
- The key includes the document origin, target URL, request method, credentials
  mode, and sorted requested-header set, so authorization cannot cross a
  policy-relevant request boundary.
- Cache retention is bounded to 64 entries and at most 600 seconds. When full,
  the deterministic oldest-key eviction keeps resource use bounded.
- A cache hit skips only the repeated OPTIONS request; the actual request still
  passes through the existing origin, CSP, redirect, credentials, CORS, size,
  and response-exposure owners.
- Private-network access, opaque `no-cors` responses, preflight response
  sharing across broader network partitions, and complete Fetch/CORS Web IDL
  parity remain open.

## Ownership and sequence

```text
request policy -> preflight key -> validated OPTIONS + max-age -> bounded cache
                                                       \-> actual request
```

The resource loader owns the cache beside cookies, document cache, and CSP
policy. No JavaScript API or content-worker protocol is expanded; repeated
requests reuse the loader's validated policy decision only.

## Deliberate boundary and tradeoffs

The cache is intentionally in-memory and process-local. It improves repeated
application requests during one native browsing session but is not persisted,
shared across browsing contexts, or presented as a complete browser network
partition implementation. A small TTL and entry cap favor predictable resource
use over long-lived cache hit rate.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine target and existing
preflight policy coverage. The issue-level full native-engine suite,
strict-Clippy baseline, documentation, release-truth, remote-CI,
publication, and browser-parity gates remain final issue gates; this task
makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_reuses_successful_cors_preflight_cache -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine preflight -- --nocapture` — 3 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 756 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  756 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=930; current-claim failures=0
