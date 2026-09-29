---
id: native-engine-browser-823
scope: glass-browser/native-engine/shared-worker-fetch-credentials
status: done
depends-on: [native-engine-browser-822]
---

# Glass native-engine browser slice 823: preserve SharedWorker Fetch credentials

## Objective

Preserve the Fetch credentials mode for direct worker `fetch()` requests and
apply it correctly to same-origin, cross-origin, and redirect URLs.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/maintainers/README.md`
- [Fetch Standard: credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)

## Contract

- Direct Fetch through the Dedicated/SharedWorker registry and
  `Request.credentials` default to `same-origin` and accept only `omit`,
  `same-origin`, and `include`.
- `include` sends and accepts cookies at same-origin and cross-origin URLs;
  `omit` sends no cookies and ignores response cookies.
- `same-origin` sends and accepts cookies only when each current request URL
  matches the worker origin. Re-evaluate the rule after every redirect.
- CORS response checks use the effective credential state for the final URL;
  cross-origin requests under `same-origin` do not require credentialed CORS
  headers.
- Exercise HTTP traffic with local same-origin and cross-origin fixtures,
  including a same-origin redirect to cross-origin. Verify request cookies,
  response `Set-Cookie` handling, and a subsequent worker request.
- Keep SharedWorker module-graph credentials behavior unchanged. Do not infer
  complete Fetch, WPT, cross-platform, or browser-completion coverage.
- Page Fetch and Service Worker Fetch are separate owners and are not changed by
  this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-823.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed.
  Its 68 warnings are the existing unused symbols from the superseded HTML
  parser in `dom.rs`.
- `cargo test -p glass-browser --test native_engine native_content_process_shared_worker_fetch_credentials_modes_follow_redirects --locked --quiet`
  passed (1 passed; 21.71 seconds).
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies --locked --quiet`
  passed (1 passed; 24.50 seconds).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- All four maintainer documentation gates passed: release truth (1,451 Markdown
  documents; 83 current; zero current-claim failures), documentation depth
  (93 current guides; 19 contracts), TUI shortcut inventory (15 help keys; 63
  markers), and documentation coverage (346 full-product MCP tools, including
  101 browser-only; 17 examples; 22 public modules).
- Committed locally with a Conventional Commit. No push or remote CI was run.
