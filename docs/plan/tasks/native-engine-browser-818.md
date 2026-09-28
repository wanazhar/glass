---
id: native-engine-browser-818
scope: glass-browser/page-response-cookie-live-context-sync
status: done
depends-on: [native-engine-browser-817]
---

# Glass native-engine browser slice 818: synchronize page-response cookies

## Objective

Propagate accepted cookie changes from ordinary page-owned HTTP responses to
every already-live page and frame in the same `NativeEngineBackend`, so its
request loaders and content realms share one current cookie jar.

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- [GCWP storage/network-origin contract](../native-engine-browser-profile.md#capability-matrix)
  requires cookie correctness, profile isolation, and no cross-profile leaks.
- [Native-engine architecture](../../architecture/native-engine.md) documents
  content-process ownership, backend topology, and cookie synchronization.
- [Slice 816](native-engine-browser-816.md) establishes page-profile write
  through for SharedWorker response cookies.
- [Slice 817](native-engine-browser-817.md) establishes same-backend fan-out
  for SharedWorker cookie changes.

## Contract

- On each ordinary content-process command completion, return the accepted
  cookie update/deletion journal produced by its existing resource loader,
  after the existing profile merge succeeds. Coalesce repeated changes by
  `(name, domain, path)` so the latest update or deletion tombstone wins, and
  enforce an explicit bounded IPC batch.
- Decode the journal as typed content-process data and queue it on the owning
  engine even if that process is replaced before the backend collects browser
  effects. Malformed or over-limit journals fail visibly.
- Fan each accepted batch to the active root page, active-target frames, every
  parked top-level target, and their live frames in the same backend. Update
  both each engine's parent request loader and its live content-process cookie
  view, and synchronize the backend's SharedWorker request loader plus its
  bounded latest-change journal. Preserve existing domain/path/expiry/
  Secure/SameSite/credentials and HttpOnly rules.
- The content process has already merged the accepted cookie state to the
  configured profile before returning its batch; fan-out refreshes live
  runtimes only and must not write the same profile once per peer. A backend
  with no live recipient returns a typed failure.
- A different `BrowserRuntimeSession`/backend remains isolated from live
  notifications, even when it uses the same profile path; it observes durable
  profile state on its normal profile-load path. Do not add another cookie
  database or change SharedWorker cookie semantics.
- A process-backed local-HTTP regression creates a parked peer target and
  child frame before a page `fetch()` response sets ordinary and HttpOnly
  cookies. The peer and frame must send both on their next request, expose only
  the non-HttpOnly cookie to script, and an independent backend/profile must
  remain cookie-free. A new engine reopening the original profile must send
  the durable cookies on its first navigation and next request.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-818.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before focused tests.
- Run the process-backed page-response fan-out/reload regression and the
  existing SharedWorker live-context fan-out regression.
- Run formatting, diff validation, and the four maintainer documentation
  gates. Record exact summaries, warnings, elapsed time, and remaining
  boundaries.
- Keep the checkpoint local; do not push or run remote CI.

## Results

The content worker now returns bounded, typed cookie deltas after the profile
merge; the backend queues them before other worker effects, merges them into
the same-backend SharedWorker loader/journal, and refreshes live page/frame
engines without repeating the profile write. The process-backed regression
passed (1 passed, 869 filtered; 69.98 seconds), covering same-backend peer and
frame requests, script-visible versus HttpOnly state, replacement and deletion
of repeated cookie keys, separate-profile isolation, and persistence after a
fresh engine restart. The focused integration-test check passed with existing
dead-code warnings from the superseded HTML parser in `dom.rs`; formatting and
diff validation passed. The companion SharedWorker-originated live-context
regression passed in the preceding focused group (2 passed, 868 filtered;
115.58 seconds).

Direct end-to-end observation of a page-originated cookie update from inside
an already-running SharedWorker is not covered here; the backend loader/journal
update is implemented, but that runtime probe remains an explicit coverage
boundary. The attempted process-backed probe did not receive the expected
second SharedWorker message or complete its HTTP request sequence before its
bounded timeout; the cause was not isolated, so this is not treated as evidence
of either a runtime defect or successful synchronization. Cross-backend live
notifications and broader cookie/WPT and cross-platform coverage also remain
open. Issue #40 remains open.
