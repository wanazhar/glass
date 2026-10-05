---
id: native-engine-browser-858
scope: glass-browser/native-engine/xhr-credentialed-preflight-wildcards
status: complete
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

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --test native_engine --locked --quiet` passed with
  the existing legacy-parser/dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_credentialed_preflight_wildcards_send_no_post
  -- --exact` passed (1 passed; 922 filtered; 23.37 seconds).
- The process fixture observed two credentialed OPTIONS requests. The page
  response used exact-origin CORS and credential support but wildcard
  `Access-Control-Allow-Methods`; the DedicatedWorker response allowed POST
  explicitly but used wildcard `Access-Control-Allow-Headers`. Both OPTIONS
  requests carried no Cookie, both XHRs reported network errors, and no actual
  POST reached the API listener. The rejected preflight `Set-Cookie` values
  were not accepted; the seeded HttpOnly parent cookie remained and
  `document.cookie` stayed empty.
- No production Rust code changed. Cookie matching, response-cookie
  acceptance, and jar ownership remain exclusively parent-owned; no raw cookie
  headers or jar were added to IPC, and no child network retry was enabled.
- `cargo fmt --all -- --check` and `git diff --check` passed. Documentation
  gates passed: release-truth scanned 1,499 Markdown files (83 current, zero
  current-claim failures); depth validated 93 guides and 19 contracts;
  shortcuts validated 15 keys and 63 markers; coverage validated 346 MCP
  tools (101 browser-only), 17 examples, and 22 public modules. Coverage used
  the existing shared-target binaries via explicit `--glass` and
  `--glass-browser` paths.
- This is focused wildcard-denial coverage, not complete CORS, XHR/Web IDL or
  WPT conformance, cross-platform certification, independent review, or remote
  CI. Issue #40 remains open.
