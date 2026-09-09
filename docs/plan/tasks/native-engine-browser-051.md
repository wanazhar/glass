---
id: native-engine-browser-051
scope: glass-browser/native-engine/script-fetch
status: done
depends-on: [native-engine-browser-050]
---

# BE-02w/BE-04ag: bounded script `fetch()` promises

## Objective

Expose a useful, policy-owned GET `fetch()` promise path to scripts in the
process-backed native document realm and commit asynchronous callback effects
through the existing typed document bridge.

## Contract

- `fetch(url)` accepts a string URL and performs a bounded GET through the
  existing child resource loader, including CSP connect policy, mixed-content,
  origin/CORS, referrer, cookie, redirect, timeout, and response-size rules.
- The returned response exposes bounded `ok`, `status`, `url`, content-type
  lookup, `text()`, and `json()` promise methods.
- Promise callbacks run in the persistent QuickJS realm; typed DOM mutations
  produced by those callbacks are committed atomically by the child owner.
- Fetch failures reject the promise rather than bypassing the resource policy.

## Deliberate boundary and tradeoffs

This is an explicit-evaluation, process-backed GET slice. Page-load script
fetch scheduling, POST/custom headers/body/stream uploads, AbortController,
cache-mode parity, service workers, XHR, WebSocket, and full Response/Headers/
Request Web IDL identity remain open. Local fixture realms fail closed rather
than pretending to provide network fetch.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_content_process_exposes_bounded_script_fetch_promises`
- existing `native_content_process_fetches_cors_authorized_cross_origin_get`
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
