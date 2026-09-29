---
id: native-engine-browser-826
scope: glass-browser/native-engine/document-referrer-policy-fetch
status: complete
depends-on: [native-engine-browser-825]
---

# Glass native-engine browser slice 826: inherit Document referrer policy in Fetch

## Objective

Apply the active HTTP Document's response `Referrer-Policy` to page Fetch
requests whose per-request referrer policy is empty.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/maintainers/README.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard: main fetch](https://fetch.spec.whatwg.org/#main-fetch)
- [HTML Standard: policy containers](https://html.spec.whatwg.org/multipage/browsers.html#policy-containers)
- [Referrer Policy: header parsing](https://w3c.github.io/webappsec-referrer-policy/#parse-a-referrer-policy-from-a-referrer-policy-header)

## Contract

- A successfully committed HTTP(S) Document derives its referrer policy from
  its final response's `Referrer-Policy` header. Parse comma-separated tokens
  in order, ignore unknown tokens, and use the last recognized policy. If no
  token is recognized, the Document keeps the default
  `strict-origin-when-cross-origin` policy.
- Page Fetch requests with an empty `referrerPolicy` use the active Document's
  policy container. This changes the effective request policy, not the public
  `Request.referrerPolicy` value, which remains empty unless explicitly set.
- An explicit policy on a Fetch `Request` or `fetch()` init takes precedence
  over the Document default. Redirect responses continue to update the
  effective request policy before the next hop.
- Policy state is owned by the document/resource-loading path, is bounded with
  the existing document-policy state, and must not leak a prior Document's
  response policy into a later Document that has no valid policy header.
- A process-backed two-origin HTTP regression verifies last-recognized-token
  selection, unknown-token handling, invalid-only fallback, an explicit
  override, and clearing a prior policy when the same Document URL is served
  again without a valid policy header. The public `Request.referrerPolicy`
  remains empty when the Document container supplies the effective policy.
- This slice does not implement `meta name="referrer"`, element
  `referrerpolicy` attributes, independent Worker policy containers, policy
  inheritance for child contexts, or policy defaults for navigation and other
  element-initiated resources. It does not claim full Referrer Policy/WPT or
  browser completion.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-826.md`

## Verification

- `cargo fmt --all` passed.
- `cargo check -p glass-browser --features native-engine --test native_engine --locked --quiet` passed with 68 existing dead-code warnings from the superseded HTML parser.
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_inherits_document_referrer_policy_for_page_fetch --locked -- --exact --nocapture` passed (1 passed, 875 filtered; 32.45 seconds).
- `git diff --check` passed.
