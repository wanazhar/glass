---
id: native-engine-browser-054
scope: glass-browser/native-engine/fetch-cors-preflight
status: done
depends-on: [native-engine-browser-053]
---

# BE-02z/BE-04aj: bounded fetch CORS preflight

## Objective

Allow the process-backed script fetch path to reach ordinary cross-origin API
endpoints while preserving a fail-closed CORS boundary and the existing single
resource-loader owner.

## Contract

- Cross-origin simple POSTs with a safelisted `Content-Type` are sent directly
  with an `Origin` header and remain subject to response-side CORS checks.
- Cross-origin non-simple POSTs send an `OPTIONS` preflight with
  `Origin`, `Access-Control-Request-Method`, and the supported
  `Access-Control-Request-Headers` declaration before the actual request.
- The preflight must succeed and authorize the origin, method, and requested
  header; credentialed requests also require the existing explicit
  `Access-Control-Allow-Credentials: true` check.
- Redirect hops re-evaluate the same URL/CSP/mixed-content/referrer policy and
  preflight again when the redirected POST target is cross-origin and
  non-simple. The actual response keeps the existing response-size and CORS
  validation.

## Deliberate boundary and tradeoffs

Only `Content-Type` is currently script-configurable. There is no preflight
cache, private-network access protocol, opaque `no-cors` response, custom
header list, credentialed wildcard relaxation, or full Fetch/Web IDL parity.
The implementation pays an OPTIONS round trip for every eligible request to
keep state bounded and avoid a new cache owner; unsupported authorization
stays an explicit network error rather than leaking the response.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --lib cors_preflight_requires_allowed_method_and_requested_header -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch -- --nocapture` — 7 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
