---
id: native-engine-browser-053
scope: glass-browser/native-engine/script-fetch-post
status: done
depends-on: [native-engine-browser-052]
---

# BE-02y/BE-04ai: bounded same-origin POST fetch

## Objective

Extend the process-backed script `fetch()` path beyond GET for the common
same-origin JSON form/API case without widening the native engine's network
policy or introducing a second request owner.

## Contract

- Explicit evaluations and initial page scripts may issue bounded `GET` or
  `POST` fetch requests.
- POST bodies are optional strings capped by the existing bounded form-body
  limit; the only script header accepted is an optional `Content-Type`.
- Same-origin POST requests preserve the existing CSP, HTTPS mixed-content,
  cookie, referrer, redirect, response-size, and response-CORS checks.
- 301/302/303 redirects convert POST to GET and discard its body/content type;
  307/308 redirects retain the method and request data.
- The persistent page realm resolves the bounded response `text()`/`json()`
  promises and commits callback mutations through the existing child owner.

## Deliberate boundary and tradeoffs

Cross-origin non-simple POST requires a preflight and is denied until the
preflight protocol exists. Cross-origin simple POST is not promoted by this
slice without dedicated CORS acceptance coverage. Custom headers beyond
`Content-Type`, multipart/FormData/blob/stream bodies, AbortController,
service workers, XHR/WebSocket, upload progress, full Fetch Web IDL identity,
and general request scheduling remain open. The JS surface intentionally
rejects unsupported methods and headers instead of silently approximating
browser behavior.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch -- --nocapture`
- fetch subset: 5 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
