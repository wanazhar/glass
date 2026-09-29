---
id: native-engine-browser-824
scope: glass-browser/native-engine/page-service-worker-fetch-credentials
status: done
depends-on: [native-engine-browser-823]
---

# Glass native-engine browser slice 824: preserve page Fetch credentials across Service Workers

## Objective

Carry the Fetch credentials mode intact from page JavaScript through a
controlled Service Worker request and any `fetch(event.request)` it initiates.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/maintainers/README.md`
- [Fetch Standard: credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)

## Contract

- Page `Request.credentials` defaults to `same-origin`; only `omit`,
  `same-origin`, and `include` are accepted by `Request` and `fetch()`.
- Page Fetch transports the mode through buffered, streaming, and
  Service-Worker-controlled request paths without collapsing it to a boolean.
- A controlled `FetchEvent.request` exposes the incoming mode, and
  Service Worker `fetch(event.request)` preserves it in the resource loader.
- `same-origin` reevaluates cookie send/accept behavior against each request
  URL; `omit` suppresses cookies and response cookies; `include` sends and
  accepts cookies across origins subject to credentialed CORS.
- A process-backed two-origin fixture verifies mode propagation, request
  cookies, Set-Cookie acceptance/rejection, defaults, and invalid values.
- This focused slice does not claim complete Fetch, Web IDL, WPT,
  cross-platform, or browser-completion coverage.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-824.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed;
  output contains the existing dead-code warnings from the superseded HTML
  parser in `dom.rs`.
- `cargo test -p glass-browser --test native_engine native_content_process_page_fetch_credentials_survive_service_worker_handoff --locked --quiet`
  passed (1 passed; 873 filtered; 31.55 seconds). The two-origin fixture
  verified page and controlled Service Worker modes, Request defaults and
  validation, request-cookie suppression/inclusion, and response-cookie
  acceptance/rejection.
- `cargo fmt --all` and `git diff --check` passed.
- Maintainer documentation gates passed: release truth (1,452 Markdown files;
  83 current; zero current-claim failures), documentation depth (93 current
  guides; 19 contracts), TUI shortcut inventory (15 help keys; 63 markers),
  and documentation coverage (346 full-product MCP tools, including 101
  browser-only; 17 examples; 22 public modules).
