---
id: native-engine-browser-856
scope: glass-browser/native-engine/xhr-denied-preflight-method-and-header
status: complete
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

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --test native_engine --locked --quiet` passed with
  the existing legacy-parser/dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_method_and_header_denied_preflights_send_no_post
  -- --exact` passed (1 passed; 920 filtered; 21.84 seconds).
- The process fixture observed two credentialed OPTIONS requests. The page
  preflight granted the exact origin, credentials, and requested headers but
  denied POST; the DedicatedWorker preflight granted the exact origin,
  credentials, and POST but omitted the requested custom header. Both OPTIONS
  requests carried no Cookie; both XHRs reported a network error, and no
  actual POST reached the listener. Neither rejected preflight's HttpOnly
  `Set-Cookie` was accepted; the seeded parent cookie remained and
  `document.cookie` stayed empty.
- No production Rust code changed. Cookie matching, response-cookie
  acceptance, and jar ownership remain exclusively parent-owned; no raw cookie
  headers or jar were added to IPC, and no child network retry was enabled.
- `cargo fmt --all -- --check` and `git diff --check` passed. Documentation
  gates passed: release-truth scanned 1,495 Markdown files (83 current, zero
  current-claim failures); depth validated 93 guides and 19 contracts;
  shortcuts validated 15 keys and 63 markers; coverage validated 346 MCP
  tools (101 browser-only), 17 examples, and 22 public modules. Coverage used
  the existing shared-target binaries via explicit `--glass` and
  `--glass-browser` paths.
- This is focused process coverage, not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, independent review, or remote CI.
  Issue #40 remains open.
