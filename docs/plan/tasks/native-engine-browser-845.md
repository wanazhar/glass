# Native engine browser slice 845: Service Worker lifetime Fetch cookie ownership

```yaml
id: native-engine-browser-845
scope: native-engine/content-process-cookie-authority
status: done
depends-on: [native-engine-browser-843]
```

## Objective

Verify that a Service Worker `fetch()` started by `event.waitUntil()` remains
independent of the `respondWith()` response while its HTTP request and response
cookies stay exclusively under the parent browser's cookie authority. Fix any
process-backed gap exposed by the regression.

## Context

- `docs/architecture/native-engine.md` — content-process ownership and
  parent-brokered network boundary.
- `crates/glass-browser/src/browser/native_backend.rs` — runtime async-effect
  pump and backend operation ordering.
- `crates/glass-browser/src/browser/native_engine/content_process.rs` —
  bounded content-process owner-turn timeout.
- `crates/glass-browser/src/browser/native_engine/engine.rs` — page owner-turn
  dispatch.
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority` —
  GCWP cookie authority and Service Worker lifetime contract.
- `docs/plan/tasks/native-engine-browser-843.md` — parent-owned cookies and
  fail-closed uncovered request classes.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- The parent selects the HttpOnly request cookie for a lifetime Fetch and
  accepts its HttpOnly response cookie into the browser-context jar.
- A later parent-owned request observes that response cookie; neither value is
  exposed through page `document.cookie` or installed as child cookie state.
- The independent FetchEvent response commits before the lifetime response is
  released, including when held beyond the ordinary page-script deadline.
- The asynchronous owner-turn pump cannot overtake the browser operation that
  produced its notification. Owner turns that wait for a parent-brokered Fetch
  use the same 30-second bound as network requests; ordinary script calls keep
  their 5-second bound.
- A missing parent broker fails closed and never retries through the
  content-process loader.
- This is process-backed evidence for this request class, not a claim that all
  Service Worker or browser network request classes are complete.

The operation gate serializes later backend requests behind an owner turn that
has already started. This slice guarantees that queued lifetime work cannot
delay the operation that produced it; it does not claim parallel page and
Service Worker execution.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-843.md`
- `docs/plan/tasks/native-engine-browser-845.md`

## Verification

- Extend `native_service_worker_wait_until_fetch_does_not_delay_response` to
  check parent-selected HttpOnly request cookies, response-cookie acceptance,
  later request reuse, and script invisibility in the process-backed session.
- `cargo check -p glass-browser --features native-engine --test native_engine --locked --quiet` — passed.
- `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --test native_engine --locked --quiet native_service_worker_wait_until_fetch_does_not_delay_response -- --exact` — 1 passed; the server held the lifetime response for six seconds before release.
- `cargo fmt --all -- --check`
- `git diff --check`
