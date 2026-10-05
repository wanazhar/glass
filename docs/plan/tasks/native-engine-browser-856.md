---
id: native-engine-browser-856
scope: glass-browser/native-engine/xhr-denied-preflight-method-and-header
status: in-progress
depends-on: [native-engine-browser-855]
---

# Glass native-engine browser slice 856: deny unauthorized XHR preflights

## Objective

Add process-backed negative coverage for credentialed XHR preflights whose
response authorizes the exact origin and credentials but denies either the
requested method or a requested header. Verify both failures stop before the
actual request while preserving parent-owned cookie authority.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-854.md` — successful preflight.
- `docs/plan/tasks/native-engine-browser-855.md` — wildcard-origin denial.
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  fail-closed contract.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS-preflight fetch](https://fetch.spec.whatwg.org/#cors-preflight-fetch)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A page credentialed POST is denied when the preflight grants the exact page
  origin, credentials, and requested headers but omits POST from
  `Access-Control-Allow-Methods`.
- A DedicatedWorker credentialed POST is denied when the preflight grants the
  exact page origin, credentials, and POST but omits the requested custom
  header from `Access-Control-Allow-Headers`.
- Both OPTIONS requests carry Origin, requested method/header names, and no
  Cookie. The page and Worker report XHR network errors; no actual POST reaches
  the API fixture. The fixture fails on an unexpected request.
- `Set-Cookie` values on both rejected preflight responses are not accepted.
  The seeded HttpOnly parent cookie remains, and HttpOnly values stay absent
  from `document.cookie`.
- Cookie matching, `Set-Cookie` acceptance, and jar ownership remain
  exclusively parent-owned. No raw cookie headers or jar cross IPC, and no
  direct child HTTP(S) retry is permitted.
- This is focused process coverage, not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, or remote CI.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-856.md`
- `docs/plan/reviews/native-engine-browser-856-01.md`

## Verification

- Implement one bounded two-origin process regression covering separate
  method-denied and header-denied requests. The API fixture accepts only the
  expected OPTIONS requests and detects any actual request.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression once; do not repeat adjacent tests
  unless production request or owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Directly review preflight method/header rejection and parent cookie
  ownership; no independent agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.
