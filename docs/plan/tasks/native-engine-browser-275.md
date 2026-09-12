# Glass native engine browser slice 275: unhandled Promise rejections

Status: completed locally.

## Objective

Make unhandled Promise rejections observable in the persistent native page
realm after a bounded microtask checkpoint, while suppressing rejections that
acquire a handler and preserving document continuity.

## Contract

- The QuickJS host rejection tracker records at most 64 pending rejection
  records.
- Each rejection reason is converted to bounded text before crossing the
  Rust/QuickJS boundary.
- A rejection that acquires a handler is removed before delivery.
- Multiple pending rejections are delivered in host callback order.
- The page receives a cancelable, non-bubbling `PromiseRejectionEvent` at the
  window through both `unhandledrejection` listeners and
  `window.onunhandledrejection`.
- The event exposes its bounded textual `reason`, a currently-null `promise`
  placeholder, and normal target/dispatch state.
- Rejection delivery occurs after pending QuickJS jobs for explicit script and
  module evaluation; page continuation and document commit remain intact.
- Tracker setup, event bootstrap, and dispatch failures remain typed native
  errors rather than being silently discarded.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-275.md`

## Implementation

`NativeJavaScriptRuntime` installs QuickJS's host Promise rejection tracker
alongside the existing interrupt handler. It stores promise-keyed records in a
bounded queue, assigns a monotonic delivery order, removes handled promises,
and truncates converted reasons to 4096 Unicode scalar values. Both explicit
script and module evaluation drain the queue after pending jobs and inject a
bounded JSON descriptor list into the existing page realm.

The bootstrap exposes `PromiseRejectionEvent`, retains its constructor across
bootstrap refreshes, and installs `onunhandledrejection` through the shared
event-handler property path. The host dispatch helper targets the window and
uses the normal event-state machinery, so listeners see `target`,
`cancelable`, `bubbles`, and `preventDefault()` behavior consistently with
other native events.

## Tradeoffs and follow-up

The current bridge transports reason text rather than the original JavaScript
value and uses `promise === null`; this avoids retaining QuickJS values or
creating an unsafe cross-boundary handle. The queue is bounded and delivery is
performed at explicit evaluation checkpoints, not yet at every browser task
source. `rejectionhandled`, structured reason/promise identity, full Promise
event Web IDL descriptors, parser-accurate script timing, and complete event
loop semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_unhandled_rejection_dispatches_window_event --locked -- --exact --test-threads=1` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_ --locked -- --test-threads=1` (83 passed before the final queue-ordering refinement; the exact ordered-rejection test passed after it)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
