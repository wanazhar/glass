---
id: native-engine-browser-855
scope: glass-browser/native-engine/xhr-denied-credentialed-preflight
status: complete
depends-on: [native-engine-browser-854]
---

# Glass native-engine browser slice 855: deny credentialed XHR preflight

## Objective

Add process-backed negative CORS-preflight coverage proving that an
unauthorized credentialed XHR fails before its actual request is dispatched.
Keep cookie selection and response-cookie processing exclusively in the
parent.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-854.md` — successful XHR preflight.
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  fail-closed contract.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A cross-origin page or DedicatedWorker XHR with `withCredentials == true`
  and a preflighted request sends an OPTIONS request without cookies.
- If the preflight response uses wildcard `Access-Control-Allow-Origin`, it
  cannot authorize a credentialed request even when
  `Access-Control-Allow-Credentials: true` is present.
- The XHR reports a network error and the parent sends no actual POST. The
  process-backed fixture must fail if any actual request reaches its listener.
- Any `Set-Cookie` on the rejected cross-origin preflight response is not
  accepted. Existing parent jar state remains intact and HttpOnly values stay
  absent from `document.cookie`.
- Exercise both page asynchronous XHR and DedicatedWorker asynchronous XHR.
  The parent remains the sole cookie matcher, `Set-Cookie` acceptor, and jar
  owner; no raw cookie headers or jar cross IPC, and no direct child HTTP(S)
  retry is allowed.
- This negative regression is not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, or remote CI.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-855.md`
- `docs/plan/reviews/native-engine-browser-855-01.md`

## Verification

- Implement one bounded two-origin process regression. The API fixture accepts
  only the expected OPTIONS requests and detects any follow-up actual request.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression once; no adjacent tests unless
  production request/owner code changes.
- Run formatting, `git diff --check`, and the four maintainer documentation
  gates after final docs edits.
- Directly review the CORS denial and parent cookie boundary; no independent
  agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --test native_engine --locked --quiet` passed with
  the existing legacy-parser/dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_denied_credentialed_preflight_sends_no_post
  -- --exact` passed (1 passed; 919 filtered; 19.94 seconds).
- The process fixture observed the page and DedicatedWorker credentialed
  OPTIONS requests with their origin, method, requested-header names, and no
  Cookie. Both responses used wildcard `Access-Control-Allow-Origin` with
  `Access-Control-Allow-Credentials: true`; both XHRs reported a network error
  and no actual POST reached the listener. The rejected preflight
  `Set-Cookie` was not accepted; the seeded HttpOnly parent cookie remained,
  and `document.cookie` stayed empty.
- No production Rust code changed. The parent remains the sole cookie matcher,
  `Set-Cookie` authority, and jar owner; no cookie header or jar was added to
  IPC. The exact-source Fetch CORS check uses the original request's
  credentials mode for the preflight response, so wildcard origin cannot
  authorize `include`.
- `cargo fmt --all -- --check` and `git diff --check` passed. Documentation
  gates passed: release-truth scanned 1,493 Markdown files (83 current, zero
  current-claim failures); depth validated 93 guides and 19 contracts;
  shortcuts validated 15 keys and 63 markers; coverage validated 346 MCP
  tools (101 browser-only), 17 examples, and 22 public modules. Coverage used
  the existing shared-target binaries via explicit `--glass` and
  `--glass-browser` paths.
- This targeted negative case is not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, independent review, or remote CI.
  Issue #40 remains open.
