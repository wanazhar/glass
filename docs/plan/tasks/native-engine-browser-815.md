---
id: native-engine-browser-815
scope: glass-browser/shared-worker-cookie-snapshot-reconciliation
status: done
depends-on: [native-engine-browser-814]
---

# Glass native-engine browser slice 815: preserve SharedWorker cookie changes

## Objective

Keep a later SharedWorker creation from erasing cookie writes or deletions
accepted by earlier session-scoped SharedWorker fetches when the creating page
still supplies an older cookie snapshot.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- [Slice 814](native-engine-browser-814.md) applies the module SharedWorker
  credentials mode to root and graph fetches.
- `NativeSharedWorkerCoordinator` owns one session-level resource loader, but
  each page-originated create currently replaces that loader's cookie jar with
  the page process snapshot. Response-cookie changes made by an earlier worker
  can therefore be lost when a later create carries a stale snapshot.

## Contract

- Reconcile creator cookie snapshots with cookie changes already accepted by
  the session SharedWorker loader. The latest session-worker write or deletion
  for each `(name, domain, path)` key must survive subsequent create snapshots.
- Record the bounded latest change per cookie key, including deletion
  tombstones; replay changes after importing a creator snapshot.
- Capture response-cookie changes from SharedWorker root/static/dynamic module
  fetches, including redirects, without weakening credentials, CORS, CSP,
  mixed-content, cookie-attribute, or existing profile-limit behavior.
- Exercise the behavior through real HTTP and the process-backed browser path:
  an eligible worker response sets a cookie, a later worker create using the
  unchanged page snapshot still sends it, and an accepted deletion is not
  resurrected by another stale snapshot.
- Do not claim live page-realm propagation or durable profile write-through;
  those remain separate issue #40 requirements. Do not expand to unrelated
  cookie APIs or full SharedWorker/WPT conformance.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-815.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before the focused behavior test.
- Add a process-backed HTTP regression that observes the later request Cookie
  headers after an earlier SharedWorker Set-Cookie and deletion, while the
  page's create snapshot remains unchanged.
- Run formatting, diff validation, and all four maintainer documentation
  gates. Record exact results, warnings, elapsed time, and the remaining
  cross-context/persistence boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

Implemented and validated locally.

- `cargo fmt --all` and `git diff --check` passed after the implementation
  batch.
- `cargo check -p glass-browser --test native_engine --locked --quiet` passed
  in about 29 seconds. The only warnings were existing dead-code warnings for
  the superseded legacy HTML parser in `native_engine/dom.rs`.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots --locked --quiet -- --exact`
  passed: 1 passed, 866 filtered; test runtime 20.52 seconds. Its initial
  cold test-target compile/link made the full command take about 2 minutes
  15 seconds.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies --locked --quiet -- --exact`
  passed after the coordinator change: 1 passed, 866 filtered; test runtime
  24.17 seconds.
- The process-backed regression observes the stale page value on the first
  request, a worker-updated value on the next create, and no cookie after an
  accepted deletion despite two more creates carrying the unchanged page
  snapshot. The coordinator keeps the latest update/tombstone per key and
  replays it after each creator-profile replacement. The journal is bounded
  to 128 distinct cookie keys, matching the existing cookie-profile cap.
- All four documentation gates passed. Exact summaries: release truth,
  `1443 Markdown documents; current documents=83; previous-version hits=63;
  semantic audit hits=1619; current-claim failures=0`; depth,
  `93 current guides routed/audited, 19 substantive contracts`; shortcuts,
  `15 implementation help keys; 63 documentation markers`; coverage,
  `1443 Markdown files, 346 full-product MCP tools (101 browser-only),
  17 examples, 22 public modules`. Measured command wall times were 1.87s
  for release truth, under 1s each for depth and shortcuts, 1.87s for coverage,
  and 4.15s for `cargo fmt --all -- --check && git diff --check`.
- This proves consistency only within the live SharedWorker coordinator.
  Updating already-live page/frame loaders and durable profile write-through
  remain open issue #40 requirements; no cross-context or restart-persistence
  claim is made. No remote CI was run.
