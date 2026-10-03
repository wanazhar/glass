---
id: native-engine-browser-842
scope: glass-browser/native-engine/parent-owned-cookie-authority
status: in-progress
depends-on: [native-engine-browser-821, native-engine-browser-822]
---

# Glass native-engine browser slice 842: parent-owned cookie authority

## Objective

Remove the complete cookie profile from the content-to-browser SharedWorker
creation message. The browser parent must derive the SharedWorker's cookie
state from the exact live page/frame owner, then preserve that state and its
later updates in the browser-coordinated SharedWorker loader.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-821.md`
- `docs/plan/tasks/native-engine-browser-822.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [HTML Standard `document.cookie`](https://html.spec.whatwg.org/multipage/dom.html#dom-document-cookie)

## Contract

- `SharedWorkerCreate` and `NativeSharedWorkerCreateRequest` do not carry a
  cookie profile or any other cookie value from the content process to the
  browser parent. The content-side producer must not serialize its
  `NativeResourceLoader` profile for this route.
- After validating the captured context, frame, and document generation, the
  browser parent resolves that exact source frame owner and reads the
  parent-owned engine loader's cookie profile under the owner lock. Do not use
  the currently selected target. A stale or changed owner fails explicitly.
- Seed the browser-coordinated SharedWorker loader from that parent profile,
  then replay the coordinator's bounded cookie overrides so previously
  accepted page/worker updates and deletions survive creation of later workers.
- Preserve supported credential modes, redirects, profile import/clear,
  response-cookie update/deletion, and separate-profile isolation on this
  browser-coordinated route.
- This task does not remove the content process's general cookie mirror or
  cookie-profile persistence and does not add the general parent network
  broker. Those are the separate [Slice 843](native-engine-browser-843.md)
  objective. The overall parent-only end state remains normative in the
  [versioned profile](../native-engine-browser-profile.md).

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-842.md`
- `CHANGELOG.md`

## Verification

- Add a process-protocol regression proving that no full cookie profile,
  crosses in the `SharedWorkerCreate` message.
- Run the parent/backend process-backed regression for
  `native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots`
  and preserve its profile update/deletion behavior.
- Retain the live-cookie and credentials-mode SharedWorker process regressions
  as adjacent behavior guards; diagnose any failure and distinguish the local
  content-process route from browser-coordinated routing.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
- Run only the focused socket-free and process-backed regressions after the
  check; keep unrelated package/workspace suites for the final issue #40 gate.
- `cargo fmt --all -- --check`
- `git diff --check`
- Run relevant documentation inventory/coverage checks.

## Current Evidence

- The current content process still owns a `NativeResourceLoader` and
  persists cookie state for other routes. This slice removes the full profile
  from externally routed SharedWorker creation. The parent resolves the exact
  source frame, checks context and document generation under that owner's
  awaited lock, seeds its SharedWorker loader from the parent engine's profile,
  and replays the coordinator's cookie overrides.
- Slices 820-823 provide process-backed evidence for profile journals,
  import/clear, and SharedWorker Fetch credential behavior. Those results are
  baseline behavior to preserve, not evidence that the parent-only boundary is
  implemented.
- Before the owner-lock fix, the scoped
  `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
  passed (exit 0; existing superseded HTML-parser dead-code warnings).
- `shared_worker_create_serialization_has_no_cookie_profile` passed (1 passed,
  1,682 filtered). `native_content_process_shared_worker_fetch_api_uses_live_cookies`
  passed (1 passed, 891 filtered).
- `native_content_process_shared_worker_fetch_credentials_modes_follow_redirects`
  failed twice (exit 101), timing out while waiting for the worker's Fetch
  sequence to settle at `crates/glass-browser/tests/native_engine.rs:33229`.
  This local content-process route remains undiagnosed and is tracked separately
  by Slice 843; credential/redirect behavior is not verified here.
- On commit `940a2afb`,
  `native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots`
  failed at `crates/glass-browser/tests/native_engine.rs:34669` after 83.45s.
  The first `session.script("connectCookieWorker('/writer.js', 'cookie-writer'); true")`
  returned `Lifecycle { operation: "script", state: "busy", reason: "frame owner is processing asynchronous work; retry the synchronous operation" }`;
  the outer test thread then panicked at line 17347. The create route had
  synchronous `try_lock` probes both before reading the profile and in the
  generic MessagePort route registration. Removing only the preflight probes
  still produced the same busy failure, confirming the transfer-route check
  also raced the async owner pump.
- The final change validates context and document generation under the
  awaited lock on the exact source owner, reads that parent's cookie profile,
  re-resolves the frame and rejects an owner-identity change, then inserts the
  already-validated transfer route without reacquiring the owner. Other
  MessagePort callers keep their existing generic validation path. No test-side
  retry was added. A broader experiment making generic registration await
  owners was reverted after the process-backed regression hung for over five
  minutes; it is not part of the final change.
- Final `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed (exit 0; existing superseded HTML
  parser dead-code warnings). `rustfmt --edition 2024 --check` on the two Rust
  files and `git diff --check` passed.
- The exact process-backed regression was subsequently run from the freshly
  linked integration-test binary under GDB. `connect_native` and
  `native_create_target` completed, and the page's
  `connectCookieWorker('/writer.js', 'cookie-writer')` script returned, but
  the first `cookieWorkerMessages` poll reached its 25-second deadline without
  receiving `writer-ready`. The parent snapshot/override/deletion behavior is
  therefore still unverified. An independent review also found that generation
  validation and route insertion were not one synchronized operation, and
  that early coordinator errors could leave a bounded orphan route.
- The current implementation patch validates the captured context and
  generation, snapshots the parent cookie profile, and registers both route
  tables during one synchronous section under the exact source owner's lock.
  It preflights connection identity before insertion and clears route state if
  coordinator reacquisition fails; worker-creation errors use the bridge-close
  path. Page MessagePort delivery now waits
  asynchronously for a busy owner instead of dropping the worker's first
  response; socket-free tests cover owner waiting and stale/invalid
  registration preflight.
- The first scoped check exposed a missing crate-visible re-export for
  `NativeCookieProfileEntry`; commit `ebf7bfa2` added it. The fresh scoped
  `cargo check -p glass-browser --features native-engine --lib --tests
  --locked --quiet` then passed (98.334 seconds). The focused unit tests
  `shared_worker_registration_preflight_failures_leave_no_routes` and
  `shared_worker_message_route_waits_for_busy_owner_without_holding_targets`
  passed (267.298 and 5.223 seconds).
- The exact process regression did not pass. Its run lasted 830.316 seconds
  and was interrupted after the test and content worker remained asleep on
  futex waits without an assertion result. A bounded GDB run from launch
  located the native async-effect pump blocked acquiring `NativeTargetState`
  in `sync_target_name` while processing frame-script effects. The parked
  MessagePort delivery arms held the target registry while awaiting an owner,
  creating a target/owner lock-order inversion. The current patch snapshots the
  exact parked owner under the target registry, releases the registry before
  awaiting that owner, and validates the owner before dispatch. A socket-free
  parked/parked close regression uses a barrier to prove the registry stays
  available while the owner is busy, then verifies delivery resumes and both
  route entries are removed. These latest code changes have not yet been
  compiled or tested. GDB attach itself is restricted by host
  `ptrace_scope`, so the trace was collected by launching the test under GDB.
  The exact test/content-worker process tree was stopped; no other Glass
  processes were touched.
- The previous blocked review remains open pending a passing process-backed
  regression and fresh independent review. Selected WPT and broader
  parent-only acceptance tests have not been run.
- `python3 scripts/check-documentation-coverage.py` reports one unrelated
  stale live measurement in `docs/mcp-schema-budget.md`: it omits the live
  row ``| Serialized `tools` array | 173,741 UTF-8 bytes |``.
  No Slice 842 link or inventory error was reported. Do not fold that separate
  MCP measurement repair into this cookie task.
- Issue #40 remains open.
