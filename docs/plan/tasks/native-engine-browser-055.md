---
id: native-engine-browser-055
scope: glass-browser/native-engine/javascript-xhr
status: done
depends-on: [native-engine-browser-054]
---

# BE-04ak: bounded XMLHttpRequest compatibility bridge

## Objective

Expose the common asynchronous XHR application path without creating a second
network owner. XHR must reuse the existing script fetch command, CORS
preflight, CSP, redirect, cookie, referrer, and response-size policy.

## Contract

- The page realm exposes `XMLHttpRequest` with asynchronous `open()` for
  bounded `GET` and `POST` requests and string-body `send()`.
- `setRequestHeader()` accepts the currently supported `Content-Type` header;
  request processing is delegated to the existing fetch path.
- `readyState`, `status`, `statusText`, `responseText`, `responseURL`,
  `getResponseHeader()`, and `getAllResponseHeaders()` expose bounded text
  response evidence.
- `onreadystatechange`, `onload`, and `onerror` callbacks run in the
  persistent page realm and their typed DOM mutations are committed through
  the existing owner.

## Deliberate boundary and tradeoffs

Synchronous XHR, upload/progress events, binary `responseType` variants,
timeout/abort semantics, the complete XHR event graph, forbidden/custom
headers, streaming response bodies, WebSocket/EventSource, and full Fetch/Web
IDL identity remain open. The bridge intentionally keeps one request path so
XHR and `fetch()` cannot diverge on security policy or redirect behavior.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_content_process_exposes_bounded_xhr_fetch_bridge` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch -- --nocapture` — 8 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
