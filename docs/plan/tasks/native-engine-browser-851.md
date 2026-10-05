---
id: native-engine-browser-851
scope: glass-browser/native-engine/xhr-same-origin-credential-mode
status: complete
depends-on: [native-engine-browser-850]
---

# Glass native-engine browser slice 851: XMLHttpRequest credential mode

## Objective

Implement and verify the XHR credential-mode contract for page and Worker
requests. `withCredentials == false` must use Fetch's `same-origin` mode;
`withCredentials == true` must use `include`. Preserve parent-only cookie
ownership for content processes and the parent loader for local-owner realms.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md` — XHR bridge and request-owner paths.
- `docs/plan/tasks/native-engine-browser-843.md` — parent-brokered synchronous
  XHR and cookie owner checks.
- `docs/plan/tasks/native-engine-browser-850.md` — parent-only cookie authority
  regression pattern.
- [XHR Standard](https://xhr.spec.whatwg.org/#the-withcredentials-attribute)
- [Fetch Standard credentials modes](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- XHR's false/default `withCredentials` value selects `same-origin`, not
  `omit`: same-origin requests may send matching cookies and accept response
  cookies, while cross-origin requests omit credentials and ignore response
  cookies. This mode applies per redirect hop.
- `withCredentials == true` selects `include`; the parent still applies normal
  cookie matching, response-cookie policy, CORS, and redirect rules.
- Async page/Worker XHR, synchronous content-process XHR through the parent
  broker, and synchronous local-owner XHR must share this behavior.
- The parent remains the sole authority for content-process cookie matching,
  `Set-Cookie` acceptance, expiry, and persistence. IPC carries neither raw
  cookie headers nor the full jar. Exact context/frame/generation/document
  owner checks remain mandatory; no child HTTP(S) retry is allowed.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-851.md`
- `docs/plan/reviews/native-engine-browser-851-01.md`

## Verification

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser
  --features native-engine --lib --tests --locked --quiet` passed; emitted the
  existing unused/dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo build -p glass-browser
  --features native-engine --bin glass-native-content-worker --locked --quiet`
  passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --lib --locked --quiet
  xhr_credentials_modes_match_same_origin_and_include_contracts`: 1 passed,
  1,694 filtered, 0.01 seconds.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser
  --features native-engine --test native_engine --locked --quiet
  native_content_process_synchronous_xhr_uses_parent_cookie_authority`: 1
  passed, 915 filtered, 28.80 seconds.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation gates passed after the final edit: release-truth scanned 1,485
  Markdown files (83 current, zero current-claim failures); depth validated 93
  guides and 19 contracts; shortcut inventory validated 15 keys and 63
  markers; coverage validated 346 MCP tools (101 browser-only), 17 examples,
  and 22 public modules.
- Direct self-review found no owner-boundary or credential-mode mismatch. No
  independent agent review, remote CI, cross-platform certification, broad XHR
  conformance, or WPT result is claimed.
