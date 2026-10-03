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
  lock, seeds its SharedWorker loader from the parent engine's profile, and
  replays the coordinator's cookie overrides.
- Slices 820-823 provide process-backed evidence for profile journals,
  import/clear, and SharedWorker Fetch credential behavior. Those results are
  baseline behavior to preserve, not evidence that the parent-only boundary is
  implemented.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
  passed (exit 0; existing superseded HTML-parser dead-code warnings).
- `shared_worker_create_serialization_has_no_cookie_profile` passed (1 passed,
  1,682 filtered). `native_content_process_shared_worker_fetch_api_uses_live_cookies`
  passed (1 passed, 891 filtered).
- `native_content_process_shared_worker_fetch_credentials_modes_follow_redirects`
  failed twice (exit 101), timing out while waiting for the worker's Fetch
  sequence to settle at `tests/native_engine.rs:33229`. The cause is
  undiagnosed; credential/redirect behavior is not verified by this
  checkpoint.
- Formatting, `git diff --check`, the exact-owner stale-create regression,
  selected WPT, and broader parent-only acceptance tests have not been run.
- The documentation coverage check currently reports an unrelated stale live
  measurement in `docs/mcp-schema-budget.md` (`Serialized tools` array); the
  new Slice 842 documents themselves are inventoried. Do not fold that separate
  MCP measurement repair into this cookie task.
- Issue #40 remains open.
