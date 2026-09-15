# Native Service Worker fetch-navigation suspension (361)

```yaml
id: native-engine-browser-361
scope: native-engine/service-worker-fetch-navigation-suspension
status: done
depends-on:
  - native-engine-browser-360
```

## Objective

Complete the native browser handoff for a controlled Service Worker fetch
event that awaits `clients.openWindow()`. The original navigation request and
fetch continuation must survive the content-process IPC boundary, resume
after the browser owner creates the requested WindowClient, and return the
actual committed navigation state without selecting the parked target or
falling back to CDP.

## Delivered behavior

- Service Worker navigation interception now has typed handled, not-handled,
  and suspended outcomes, with a bounded continuation keyed by worker and
  `clients.openWindow()` request identity.
- The content process retains the original load request and runtime state,
  reports a suspension to the browser owner, and accepts a versioned load
  resume command after the open-window effect resolves.
- The browser owner creates the parked native target, synchronizes its client
  projection, resolves the worker Promise with the new `WindowClient`, and
  resumes the suspended fetch continuation.
- A resumed Service Worker `Response` is committed as the navigation
  document. If the continuation resolves without a response, the original
  request follows the bounded native network path instead.
- Response-carried and newly queued Service Worker effects are merged with
  explicit bounds instead of allowing the IPC merge to discard an
  `openWindow()` request. The returned navigation snapshot is refreshed after
  the complete browser-owned effect cascade.
- The HTTP integration witness verifies registration and control, fetch-event
  suspension, parked-window creation, continuation resumption, final URL,
  resumed DOM, and the two-target topology.

## Contract and tradeoffs

The continuation is owned by the content process until the browser resolves
the specific `clients.openWindow()` request; the browser routes only the
opaque worker/request/context/frame identities. This preserves exact target
ownership across the IPC boundary and keeps the parked target unselected.

The handoff is bounded to the existing native effect and worker-message
limits. A fetch continuation that remains pending for another unsupported
reason still fails closed rather than creating an unbounded suspended task.
The resumed path supports the Service Worker response and network fallback
needed by this navigation contract; richer transferables, browser-wide
registration arbitration, durable live-client leases, complete task-source
conformance, and final Core Web Profile certification remain separate issue
#40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-361.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked`
- `cargo test -p glass-browser --test native_engine native_runtime_service_worker_ --locked -- --nocapture` — 3 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
