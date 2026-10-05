---
id: native-engine-browser-852
scope: glass-browser/native-engine/xhr-cross-origin-credentials-cors
status: complete
depends-on: [native-engine-browser-851]
---

# Glass native-engine browser slice 852: cross-origin XHR credentials

## Objective

Add process-backed two-origin coverage for XMLHttpRequest credentials and CORS
through the parent broker. A same-host/different-port loopback target keeps
cookie host/path matching eligible while making request origins distinct.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-823.md` — existing two-origin Fetch
  credentials and redirect behavior.
- `docs/plan/tasks/native-engine-browser-851.md` — XHR mode mapping and
  same-origin parent-cookie regression.
- `docs/architecture/native-engine.md` — parent broker and XHR paths.
- [XHR Standard](https://xhr.spec.whatwg.org/#the-withcredentials-attribute)
- [Fetch Standard credentials modes](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- Page asynchronous XHR, page synchronous XHR through the parent broker, and
  DedicatedWorker asynchronous XHR use the captured `same-origin`/`include`
  mode when contacting a different origin.
- With the default `withCredentials == false`, the parent sends no matching
  cookies to the cross-origin target and does not accept that response's
  `Set-Cookie`, even though ordinary cookie host/path matching would match.
- With `withCredentials == true`, the request may carry parent-matched
  cookies and its response cookie may be accepted only when the credentialed
  CORS response authorizes the page origin. Later requests observe accepted
  cookies through the parent loader.
- All seed and response cookies in the test are HttpOnly. The script-visible
  projection remains filtered. IPC carries only existing owner-tagged writes
  and the bounded URL-scoped `document.cookie` projection, never raw
  Cookie/Set-Cookie headers or the full jar.
- Existing exact context/frame/generation/document owner checks remain
  mandatory. The content process must not retry through direct HTTP(S).
- This slice tests direct cross-origin requests, not XHR redirect-chain
  behavior, full CORS/WPT conformance, other browser APIs, or cross-platform
  certification.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-852.md`
- `docs/plan/reviews/native-engine-browser-852-01.md`

## Verification

- Complete the two-origin process-backed regression before invoking Cargo.
- Run scoped `cargo check` for `glass-browser` library and test metadata before
  the exact integration test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run broad workspace tests.
- Run the exact process-backed regression and adjacent Slice 851 XHR parent
  cookie test if the change alters shared request/owner code.
- Run `cargo fmt --all -- --check`, `git diff --check`, and the documentation
  release-truth, depth, shortcut, and coverage checks after the final doc edit.
- Directly review CORS credentials and cookie owner boundaries; no independent
  agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --lib --tests --locked --quiet` passed with existing
  warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_cross_origin_credentials_remain_parent_owned`
  passed (1 passed; 916 filtered; 24.23 seconds). It covered async page XHR,
  synchronous page XHR through the parent broker, and async DedicatedWorker
  XHR across distinct loopback origins.
- The test observed that default `same-origin` mode sent no cross-origin
  cookies and rejected response cookies; explicit `include` sent parent-matched
  cookies and accepted response cookies under exact-origin credentialed CORS.
  Subsequent requests observed only accepted cookies, all HttpOnly, while
  `document.cookie` stayed empty. The parent remained the only cookie matcher,
  response-cookie authority, and jar owner; no cookie headers or jar crossed
  IPC.
- The scoped check and focused process test used the shared target directory;
  the process test reused the unchanged Slice 851 content-worker binary. No
  production Rust source changed in this slice.
- `cargo fmt --all -- --check`, `git diff --check`, and all four documentation
  gates passed after the final edits: release-truth scanned 1,487 Markdown
  files (83 current; zero current-claim failures); depth validated 93 guides
  and 19 contracts; shortcut inventory validated 15 keys and 63 markers;
  coverage validated 346 MCP tools (101 browser-only), 17 examples, and 22
  public modules.
- Direct self-review found no credential-mode, CORS, cookie-ownership, or
  IPC-boundary mismatch. This does not claim XHR redirect-chain behavior, full
  XHR/Web IDL or WPT conformance, cross-platform certification, independent
  review, or remote CI. Issue #40 remains open.
