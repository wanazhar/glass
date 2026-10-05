---
id: native-engine-browser-858
scope: glass-browser/native-engine/xhr-credentialed-preflight-wildcards
status: in-progress
depends-on: [native-engine-browser-857]
---

# Glass native-engine browser slice 858: reject credentialed preflight wildcards

## Objective

Add process-backed coverage proving wildcard allowed-method and allowed-header
values do not authorize a credentialed XHR preflight. Preserve parent-only
cookie handling and ensure both requests stop before POST.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-855.md` — wildcard-origin denial.
- `docs/plan/tasks/native-engine-browser-856.md` — method/header denial.
- `docs/plan/tasks/native-engine-browser-857.md` — preflight cache isolation.
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  fail-closed contract.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS-preflight fetch](https://fetch.spec.whatwg.org/#cors-preflight-fetch)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- The page's credentialed POST preflight returns exact-origin CORS and
  `Access-Control-Allow-Credentials: true`, but uses `*` for
  `Access-Control-Allow-Methods`. The method wildcard must not authorize POST.
- The DedicatedWorker's credentialed POST preflight returns exact-origin CORS,
  credentials support, and an explicit allowed POST method, but uses `*` for
  `Access-Control-Allow-Headers`. The header wildcard must not authorize the
  requested custom header.
- Both OPTIONS requests carry Origin, requested method/header names, and no
  Cookie. Both XHRs report network errors; no actual POST reaches the API
  fixture, which fails on an unexpected request.
- `Set-Cookie` on either rejected preflight is not accepted. The seeded
  HttpOnly parent cookie remains, and HttpOnly stays absent from
  `document.cookie`.
- Cookie matching, `Set-Cookie` acceptance, and jar ownership remain
  exclusively parent-owned. No raw cookie headers or jar cross IPC, and no
  direct child HTTP(S) retry is permitted.
- This is focused wildcard-denial coverage, not complete CORS, XHR/Web IDL or
  WPT conformance, cross-platform certification, or remote CI.

## Tradeoff

Credentialed CORS requires explicit allowed methods and headers rather than
wildcards. Servers must enumerate the exact requested values; accepting a
wildcard for convenience would expand the authority granted by the preflight.
This slice verifies the denial boundary without changing production behavior.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-858.md`
- `docs/plan/reviews/native-engine-browser-858-01.md`

## Verification

- Implement one bounded two-origin process regression with separate page
  method-wildcard and Worker header-wildcard cases. The API fixture accepts
  only expected OPTIONS requests and detects any actual request.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression once; do not repeat adjacent tests
  unless production request or owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Directly review wildcard semantics and parent cookie ownership; no
  independent agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.
