---
id: native-engine-browser-853
scope: glass-browser/native-engine/xhr-redirect-hop-credentials
status: complete
depends-on: [native-engine-browser-852]
---

# Glass native-engine browser slice 853: XHR redirect-hop credentials

## Objective

Add process-backed HTTP coverage proving that XHR credentials mode is applied
to each redirect URL, not only the initial URL. Preserve parent-only cookie
matching, response-cookie acceptance, and jar ownership across asynchronous
page XHR, synchronous parent-brokered XHR, and asynchronous DedicatedWorker
XHR.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-851.md` — XHR credentials mode.
- `docs/plan/tasks/native-engine-browser-852.md` — direct cross-origin XHR.
- `docs/plan/tasks/native-engine-browser-823.md` — per-hop Fetch credentials
  precedent through SharedWorker Fetch.
- `docs/architecture/native-engine.md` — parent broker and redirect owner.
- [XHR Standard](https://xhr.spec.whatwg.org/#the-withcredentials-attribute)
- [Fetch Standard credentials modes](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A page or Worker XHR begins at origin A and redirects to origin B on a
  different loopback port. Each hop uses the request's captured credentials
  mode (`same-origin` by default, `include` when `withCredentials` is true).
- At origin B, default credentials mode sends no matching parent cookie and
  does not accept B's `Set-Cookie`, even if matching cookies are already in the
  parent jar. A later same-origin request still uses accepted parent cookies.
- With `include`, origin B may receive parent-matched cookies and its response
  cookie may be accepted when credentialed CORS authorizes origin A. A later
  include request observes that cookie through the parent loader.
- Exercise async page XHR, synchronous page XHR through the parent broker, and
  async DedicatedWorker XHR. Keep response cookies HttpOnly and verify the
  script-visible projection remains filtered.
- The parent remains the only cookie matcher, `Set-Cookie` acceptor, and jar
  owner. IPC carries neither raw Cookie/Set-Cookie headers nor the full jar;
  existing exact context/frame/generation/document owner validation remains
  mandatory. The content process may not retry through direct HTTP(S).
- This is focused redirect-hop regression coverage, not full redirect
  semantics, XHR/Web IDL or WPT conformance, cross-platform certification, or
  remote CI.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-853.md`
- `docs/plan/reviews/native-engine-browser-853-01.md`

## Verification

- Complete the process-backed redirect regression before invoking Cargo.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression. Re-run adjacent XHR owner tests only
  if production request/owner code changes.
- Run formatting, `git diff --check`, and the four maintainer documentation
  gates after the final docs edits.
- Directly review per-hop mode selection and parent cookie ownership; no
  independent agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.

## Results

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --test native_engine --locked --quiet` passed with
  existing legacy-parser/dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_xhr_redirect_credentials_are_parent_owned_per_hop
  -- --exact` passed (1 passed; 917 filtered; 21.65 seconds).
- The process fixture observed async page, synchronous parent-brokered page,
  and async DedicatedWorker XHR redirect from origin A to origin B. Default
  mode omitted cookies at B and rejected B's response cookie. Explicit
  `include` sent parent-matched cookies under exact-origin credentialed CORS;
  accepted HttpOnly response cookies appeared on later requests. The
  script-visible projection remained empty.
- No production Rust source changed. The parent remains the only cookie
  matcher, `Set-Cookie` authority, and jar owner; no cookie header or jar was
  added to IPC. The test and scoped check reused the shared target directory.
- `cargo fmt --all -- --check`, `git diff --check`, and all four documentation
  gates passed after the final edits: release-truth scanned 1,489 Markdown
  files (83 current; zero current-claim failures); depth validated 93 guides
  and 19 contracts; shortcut inventory validated 15 keys and 63 markers;
  coverage validated 346 MCP tools (101 browser-only), 17 examples, and 22
  public modules.
- Direct self-review found no per-hop credentials, CORS, cookie-ownership, or
  IPC-boundary mismatch. This does not claim full redirect semantics, XHR/Web
  IDL or WPT conformance, cross-platform certification, independent review,
  or remote CI. Issue #40 remains open.
