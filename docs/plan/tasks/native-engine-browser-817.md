---
id: native-engine-browser-817
scope: glass-browser/shared-worker-cookie-live-context-fanout
status: done
depends-on: [native-engine-browser-816]
---

# Glass native-engine browser slice 817: fan out SharedWorker cookies to live contexts

## Objective

Propagate accepted SharedWorker response-cookie changes to every already-live
page and frame content process owned by the same `NativeEngineBackend`, so all
contexts using that backend's cookie profile immediately observe one session
cookie jar.

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- [GCWP storage/network-origin contract](../native-engine-browser-profile.md#capability-matrix)
  requires profile isolation, cookie correctness, and no cross-profile leaks.
- [Native-engine architecture](../../architecture/native-engine.md) documents
  target/frame ownership and cookie synchronization.
- [Slice 815](native-engine-browser-815.md) preserves accepted changes against
  stale SharedWorker creator snapshots.
- [Slice 816](native-engine-browser-816.md) updates the owning page/frame and
  persists changes through the existing profile merge path.

## Contract

- Fan out each accepted SharedWorker cookie update or deletion to the active
  root page, active-target frames, every parked top-level target, and their
  live frames within this backend. The backend clones one `NativeEngineConfig`
  for these engines, so they share the same configured profile or the same
  in-memory session boundary.
- Update each engine's request loader and content-process cookie view. Preserve
  domain/path/expiry/Secure/SameSite/credentials checks and never expose an
  HttpOnly cookie through `document.cookie`.
- Persist the change through the existing profile merge once per fan-out batch,
  not once per live process. Keep the bounded update/tombstone journal and
  existing 128-cookie profile limits.
- Do not fan out across independent `BrowserRuntimeSession`/backend instances,
  even if they happen to reference the same profile path; cross-process profile
  notification and general cookie mutation synchronization remain separate
  issue #40 work. Do not introduce a second cookie database or change
  ordinary-page response cookie handling in this slice.
- Process-backed HTTP coverage must create all tested targets/frames before
  the worker response, then prove that another already-live top-level page and
  a live frame see the cookie in both `document.cookie` (when not HttpOnly) and
  their next request. A separate backend/profile must not receive the update.
- Propagate typed content-process failures; do not silently report successful
  fan-out when a live engine rejects the cookie update. Keep iteration bounded
  by existing target/frame topology limits.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-817.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before focused tests.
- Add a local-HTTP process-backed regression for a pre-existing parked target
  and a live child frame, plus isolation against a separate backend/profile.
- Rerun the focused Slice 816 profile/HttpOnly/deletion test and the Slice 815
  stale-snapshot test after fan-out integration; rerun Slice 814 credentials
  only if its path is affected.
- Run formatting, diff validation, and all four maintainer documentation
  gates. Record exact summaries, warnings, wall time, and remaining boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

Implemented and validated locally.

- `cargo fmt --all` and `git diff --check` passed.
- `cargo check -p glass-browser --test native_engine --locked --quiet`
  passed with 68 existing dead-code warnings from the superseded legacy HTML
  parser in `native_engine/dom.rs`; no new warnings were introduced.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_cookie_changes --locked --quiet`
  passed: 3 passed, 866 filtered; test runtime 126.59 seconds. It covers the
  Slice 815 stale-snapshot/tombstone journal, Slice 816 page/profile and
  HttpOnly behavior, and this slice's live top-level target/frame fan-out plus
  separate-profile isolation. The changed integration target required a fresh
  relink; the full command took about 3m22s wall-clock.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies --locked --quiet`
  passed: 1 passed, 868 filtered; test runtime 25.05 seconds.
- The first fixture draft failed before reaching a fan-out assertion because a
  helper function was absent from the parked page's realm. Direct evaluation
  replaced that helper; the final focused group passed. Exact temporary profile
  JSON and lock artifacts from that failed attempt were removed.
- The new real-HTTP regression creates the worker page, another parked
  top-level page, its child frame, and an independent backend with a different
  profile before the worker response. It proves both peer realms expose the
  cookie and send it on their next HTTP requests, while the independent
  backend's `document.cookie` and request remain cookie-free.
- The backend applies the complete change batch to the active root, active
  frames, parked targets, and their frames. The first live engine performs the
  existing profile merge; remaining engines refresh only their in-memory
  loader/content-process view, avoiding one profile write per target. Existing
  target/frame bounds cap the fan-out. Errors from any live engine are
  returned rather than hidden.
- Independent backend/session notification even when sharing one profile path,
  ordinary page-response cookie synchronization, broader cookie/WPT
  conformance, and cross-platform certification remain open issue #40 work.
  No remote CI was run.
- All four maintainer documentation gates passed: release truth,
  `1445 Markdown documents; current documents=83; previous-version hits=63;
  semantic audit hits=1623; current-claim failures=0`; documentation depth,
  `93 current guides routed/audited, 19 substantive contracts`; TUI shortcuts,
  `15 implementation help keys; 63 documentation markers`; documentation
  coverage, `1445 Markdown files, 346 full-product MCP tools (101
  browser-only), 17 examples, 22 public modules`. On the final rerun, measured
  wall times were 1.10s for release truth, under 1s for depth and shortcut
  validation, 1.79s for coverage, 8.24s for `cargo fmt --all -- --check`, and
  1.61s for `git diff --check`.
