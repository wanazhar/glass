---
id: native-engine-browser-816
scope: glass-browser/shared-worker-cookie-page-and-profile-sync
status: done
depends-on: [native-engine-browser-815]
---

# Glass native-engine browser slice 816: synchronize SharedWorker cookie changes

## Objective

Make credential-accepted SharedWorker response-cookie changes visible to its
owning live page/frame content process and durable in the configured native
profile, using the existing cookie-change and profile persistence contracts.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- [Slice 814](native-engine-browser-814.md) applies module SharedWorker
  credentials to root and graph fetches.
- [Slice 815](native-engine-browser-815.md) prevents stale creator snapshots
  from rolling back accepted writes/deletions inside the live session
  coordinator, but intentionally does not synchronize the page loader or
  durable profile.

## Contract

- Deliver accepted SharedWorker cookie changes (including deletion tombstones)
  to the live page/frame that owns the creating connection. Refresh its
  resource loader and `document.cookie` view without exposing HttpOnly cookies
  to script.
- Persist those changes through the existing native web-storage profile merge
  path. Do not introduce a second cookie database or weaken profile bounds.
- Preserve Secure, HttpOnly, SameSite, domain/path, expiry, and request
  credentials behavior. Updates from both initial/module-graph creation and
  worker-directed module loads must use the same synchronization path.
- Process-backed HTTP coverage must show: a worker response sets a cookie;
  the owning page's next HTTP request sends it without creating another worker;
  after closing and reopening a session with the same profile, a request still
  sends it; and an accepted deletion stops later requests from sending it.
- Do not claim fan-out to unrelated already-live page/frame processes. That
  remains an explicit issue #40 requirement. Do not expand to unrelated
  storage APIs or full cookie/WPT conformance.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-816.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before the focused process-backed test.
- Verify owning-page request headers, script visibility rules, profile reload,
  and cookie deletion through local HTTP servers.
- Run formatting, diff validation, and all four maintainer documentation
  gates. Record exact outputs, warnings, elapsed time, and remaining
  cross-process fan-out boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

Implemented and validated locally.

- `cargo fmt --all` and `git diff --check` passed.
- `cargo check -p glass-browser --test native_engine --locked --quiet`
  passed. The 68 warnings are existing dead-code warnings from the superseded
  legacy HTML parser in `native_engine/dom.rs`; the Slice 816 test code adds no
  compiler warnings.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_cookie_changes --locked --quiet`
  passed: 2 passed, 866 filtered; test runtime 69.53 seconds. This exercises
  the stale-snapshot update/deletion journal across two live pages and the
  owning-page/profile synchronization path. The changed integration target
  required a fresh relink; the full command took about 2m40s wall-clock.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies --locked --quiet`
  passed: 1 passed, 867 filtered; test runtime 24.08 seconds.
- The process-backed profile test verifies that an accepted worker
  `Set-Cookie` reaches the owning page's next fetch, remains present after
  reopening the configured profile, stays hidden from `document.cookie` when
  HttpOnly, and is absent from later requests both immediately after deletion
  and after another profile reload.
- SharedWorker changes are applied to the owning content process and its
  loader, then persisted through the existing profile merge path. The
  coordinator's bounded update/tombstone journal still protects stale
  create-time snapshots. No second cookie database was added.
- Cross-process fan-out to unrelated already-live page/frame processes,
  broader cookie/WPT conformance, and cross-platform coverage remain open
  issue #40 requirements. No remote CI was run.
- All four maintainer documentation gates passed: release truth,
  `1444 Markdown documents; current documents=83; previous-version hits=63;
  semantic audit hits=1621; current-claim failures=0`; documentation depth,
  `93 current guides routed/audited, 19 substantive contracts`; TUI shortcuts,
  `15 implementation help keys; 63 documentation markers`; documentation
  coverage, `1444 Markdown files, 346 full-product MCP tools (101
  browser-only), 17 examples, 22 public modules`. Final measured wall times
  were 8.65s for release truth, under 1s for depth and shortcut validation,
  5.96s for coverage, and 14.79s for `cargo fmt --all -- --check`; `git diff
  --check` completed in under 1s.
