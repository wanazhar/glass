---
id: native-engine-browser-854
scope: glass-browser/native-engine/xhr-cors-preflight-credentials
status: complete
depends-on: [native-engine-browser-853]
---

# Glass native-engine browser slice 854: XHR CORS preflight credentials

## Objective

Add process-backed HTTP coverage for cross-origin XMLHttpRequest requests that
require CORS preflight. Prove that OPTIONS never carries cookies, including
when the eventual XHR uses `withCredentials == true`, and preserve parent-only
cookie matching and response-cookie acceptance for the actual request.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-852.md` — direct cross-origin XHR.
- `docs/plan/tasks/native-engine-browser-853.md` — XHR redirect-hop credentials.
- `docs/plan/tasks/native-engine-browser-843.md` — parent cookie authority and
  broker contract.
- `docs/architecture/native-engine.md` — CORS preflight and parent broker.
- [XHR Standard: `withCredentials`](https://xhr.spec.whatwg.org/#the-withcredentials-attribute)
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A cross-origin page or Worker XHR using a non-safelisted method/header or
  content type performs a parent-brokered OPTIONS preflight before the actual
  request.
- The OPTIONS request contains the requesting `Origin`, requested method, and
  normalized requested-header names, but no Cookie header even when the actual
  request uses `include`.
- For default `withCredentials == false`, the preflight may authorize wildcard
  origin; the actual cross-origin request sends no cookies and its response
  cookie is not accepted.
- For `withCredentials == true`, the preflight response must authorize the
  exact origin, method, requested headers, and credentials support. The
  subsequent actual request carries only parent-matched cookies, and an
  authorized response cookie is accepted by the parent for later requests.
- Exercise asynchronous page XHR with default credentials, synchronous page
  XHR through the parent broker with `include`, and asynchronous
  DedicatedWorker XHR with `include`. Keep cookies HttpOnly and verify the
  script-visible projection remains filtered.
- The parent remains the sole cookie matcher, `Set-Cookie` acceptor, and jar
  owner. No raw Cookie/Set-Cookie header or full jar crosses IPC; exact owner
  checks remain mandatory and the child has no direct HTTP(S) retry.
- This focused test does not claim complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, or remote CI.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-854.md`
- `docs/plan/reviews/native-engine-browser-854-01.md`

## Verification

- Complete the process-backed page/Worker preflight regression before invoking
  Cargo.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression; do not repeat adjacent tests unless
  production request or owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after the final docs edits.
- Directly review OPTIONS credential exclusion, CORS authorization, and parent
  cookie ownership; no independent agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --test native_engine --locked --quiet` passed with
  the existing legacy-parser/dead-code warnings. It also identified one unused
  fixture variable, removed before the final test build.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_cors_preflight_never_carries_cookies -- --exact`
  passed (1 passed; 918 filtered; 24.91 seconds).
- The fixture observed three OPTIONS requests (default page, synchronous
  credentialed page, and credentialed DedicatedWorker), each with Origin,
  requested method/header names, and no Cookie. The actual default page POST
  also omitted cookies and its HttpOnly response cookie was not accepted.
  Credentialed actual requests carried parent-matched cookies under exact
  origin/credentials CORS authorization; their HttpOnly response cookies were
  visible on later requests but absent from `document.cookie`.
- No production Rust source changed. The parent remains the only cookie
  matcher, `Set-Cookie` authority, and jar owner; no cookie header or jar was
  added to IPC. The scoped check and focused process test reused the shared
  target directory.
- `cargo fmt --all -- --check`, `git diff --check`, and all four documentation
  gates passed after the final edits: release-truth scanned 1,491 Markdown
  files (83 current; zero current-claim failures); depth validated 93 guides
  and 19 contracts; shortcut inventory validated 15 keys and 63 markers;
  coverage validated 346 MCP tools (101 browser-only), 17 examples, and 22
  public modules.
- Direct self-review found no preflight credential, CORS, cookie-ownership, or
  IPC-boundary mismatch. This does not claim full CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, independent review, or remote CI.
  Issue #40 remains open.
