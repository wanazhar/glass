# Native Service Worker registration updates (342)

Status: implemented locally in the current 0.3.14 checkout.

This slice makes the active registration update path native-owned:

- page-side `ServiceWorkerRegistration.update()` emits a bounded host command;
- the native owner reloads the registered classic or module script and its
  module/import graph through the existing resource policy;
- a fresh isolated worker settles install and activate `waitUntil` work before
  it becomes active;
- the old worker's message routes are removed, the replacement registration is
  exposed to the page, and the durable profile is refreshed; and
- later navigation and Fetch requests are intercepted by the new worker
  version without Chromium/CDP.

## Tradeoffs

- This slice replaces the active worker immediately after bounded install and
  activate succeed. Installing and waiting worker objects, `updatefound`, and
  byte-identical script short-circuiting remain explicit follow-up conformance
  work rather than being represented by a misleading partial state.
- Updates reuse the persisted script URL, scope, worker type, and existing
  resource policy. That keeps cookies, redirects, MIME checks, and module graph
  loading consistent with registration and restart paths.
- A failed script load or lifecycle event leaves the previous active worker in
  place and returns an error. The durable registration profile is not removed
  as a side effect of an unsuccessful update.
- Replacing a worker removes the old message-port routes immediately. This
  prevents new traffic from reaching stale code; complete in-flight event and
  client lifecycle ordering remains a later gate.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_updates_service_worker_registration --locked` — 1 passed
- `git diff --check`

The integration witness registers a v1 worker, requests an update to v2,
confirms the replacement is active, and verifies that a subsequent navigation
and Fetch response come from v2 while the server receives no request for the
intercepted navigation. All evidence is local; this checkout has not been
pushed and has no remote CI result.
