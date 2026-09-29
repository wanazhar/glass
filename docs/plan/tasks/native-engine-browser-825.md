---
id: native-engine-browser-825
scope: glass-browser/native-engine/fetch-referrer
status: done
depends-on: [native-engine-browser-824]
---

# Glass native-engine browser slice 825: carry Fetch referrers and policies

## Objective

Implement Fetch `Request.referrer` and `Request.referrerPolicy` for page and
worker requests, including propagation through controlled Service Workers and
redirected network requests.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/maintainers/README.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard: Request](https://fetch.spec.whatwg.org/#request-class)
- [Referrer Policy](https://w3c.github.io/webappsec-referrer-policy/)

## Contract

- Page and Worker `Request` objects expose `referrer` and `referrerPolicy`.
  Their defaults are `about:client` and the empty policy string.
- `Request` construction, cloning, and `fetch(input, init)` preserve inherited
  values and honor explicit overrides. A referrer of `""` suppresses the
  `Referer` header; `"about:client"` uses the creating environment's URL; a
  same-origin URL is accepted; a cross-origin URL falls back to the client.
- The accepted policies are the empty string plus the eight Referrer Policy
  tokens. Invalid `Request` values throw `TypeError`; invalid `fetch()` values
  reject with `TypeError`. The empty policy uses the standard default,
  `strict-origin-when-cross-origin`.
- The network owner computes a sanitized `Referer` separately for each target
  URL using the original referrer source and current policy. Redirect responses
  may update the policy before the next hop. A page script cannot forge the
  forbidden `Referer` request header.
- `FetchEvent.request` exposes the initiating request's public referrer fields
  while retaining its effective referrer source internally; Service Worker
  `fetch(event.request)` must preserve both fields end-to-end.
- Fetch cache identity distinguishes the effective referrer to prevent a
  response fetched under one referrer from satisfying a request with another.
- A process-backed HTTP regression covers API defaults, overrides, invalid
  values, cross-origin policy outcomes, redirect behavior, and the controlled
  Service Worker handoff.
- Document/Worker policy-container delivery from response headers, meta
  elements, and element attributes remains a separate profile integration;
  this slice does not claim complete Referrer Policy/WPT conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-825.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed;
  existing dead-code warnings from the superseded HTML parser in `dom.rs`
  remain.
- `cargo test -p glass-browser --test native_engine native_content_process_fetch_referrer_policy_survives_service_worker_handoff --locked --quiet`
  passed (1 passed; 874 filtered; 37.37 seconds). The process-backed
  two-origin regression covers page and worker defaults/overrides, invalid
  values, redirect policy updates, and the controlled Service Worker handoff.
- `cargo test -p glass-browser --lib fetch_referrer --locked --quiet` passed
  (3 passed; 1,640 filtered), covering policy outcomes, all accepted tokens,
  empty-policy defaulting, and referrer source sanitization.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Maintainer documentation gates passed: release truth (1,453 Markdown
  documents; zero current-claim failures), documentation depth (93 current
  guides; 19 contracts), TUI shortcut inventory (15 help keys; 63 markers),
  and documentation coverage (346 full-product MCP tools, including 101
  browser-only; 17 examples; 22 public modules).
- Committed locally with a Conventional Commit. No push or remote CI was run.
