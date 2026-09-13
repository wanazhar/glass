# Glass native engine browser slice 283: dedicated Web Workers

Status: completed locally.

## Objective

Make the first ordinary multi-realm browser workflow executable in the native
backend. A page can create a classic dedicated `Worker`, load its script
through the native resource/security owner, exchange structured-clone messages,
observe startup/runtime failures, and terminate the worker without CDP.

## Contract

- `new Worker(url)` accepts bounded classic worker scripts from HTTP(S) pages
  and registered local `fixture:` pages.
- Worker source loading uses the shared URL, redirect, cookie, MIME, byte,
  mixed-content, and `worker-src`/`default-src` policy path for HTTP(S).
- Every worker runs in its own bounded QuickJS realm with `self`, `location`,
  `onmessage`, `addEventListener`, `postMessage`, and `close`; it receives no
  page `document` or page DOM handles.
- Page-to-worker and worker-to-page messages use bounded JSON-backed
  structured-clone data and are delivered at explicit native page-turn
  boundaries.
- Worker startup, evaluation, and transport failures become page `error`
  events; they do not publish false success or silently fall back to CDP.
- `terminate()` and worker-side `close()` remove the native worker owner and
  suppress later message delivery.
- Worker commands emitted by initial page scripts, ordinary evaluations, and
  dynamically attached scripts route through one bounded host registry in both
  local and HTTP(S) content-process paths.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

The page bootstrap now exposes a bounded `Worker` object and emits typed
create/post/terminate/close commands. `NativeWorkerRegistry` owns one isolated
runtime per worker, loads worker source through `NativeResourceLoader`, runs
the classic script, and queues bounded message/error descriptors for the page
realm. Worker command buffers are kept separate from document mutation
commands, so the document applier cannot accidentally treat worker lifecycle
as a DOM operation.

Local fixture workers and HTTP(S) content-process workers share the registry.
The content process clears workers on full navigation, schedules workers
created by page-load scripts, and processes workers created by later script or
DOM mutation turns. Pending worker events are serialized into the next page
evaluation, preserving a deterministic host boundary while allowing handlers
to mutate the page through the existing transaction path.

## Tradeoffs and follow-up

The first slice uses serialized page turns instead of a continuously running
parallel JavaScript event loop. This keeps the two-crate native architecture,
IPC ownership, resource limits, and test timing deterministic, at the cost of
not yet modeling browser task-source fairness or real-time worker scheduling.
The structured clone is JSON-backed, so transferables, binary buffers,
`SharedArrayBuffer`, module workers, `SharedWorker`, service workers,
BroadcastChannel, worker import graphs, worker Fetch/XHR, worker timers, and
complete Worker Web IDL descriptors remain issue #40 expansion work. The
native engine is not yet certified production-parity complete by this slice.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked -- --nocapture` (9 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
