# Glass native engine browser slice 284: dedicated worker timers

Status: completed locally.

## Objective

Make dedicated-worker background scheduling useful for ordinary browser code.
Worker callbacks must survive across page evaluations, honor bounded
cancellation, and deliver their messages through the same local and
HTTP(S)-content-process ownership boundary as ordinary worker messages.

## Contract

- A classic dedicated worker can call bounded `setTimeout` and `setInterval`
  with callback arguments.
- `clearTimeout` and `clearInterval` cancel queued or currently running
  callbacks through one worker timer owner.
- Worker `performance.now()` is finite and advances from the worker realm's
  monotonic native clock.
- Due timers run at the next native page-turn boundary in both the local
  engine and the HTTP(S) content process.
- Timer callbacks may use the existing worker `postMessage` and `close`
  commands; command and message limits remain enforced.
- A timer callback failure is handled by the existing worker error/lifecycle
  boundary and never falls back to CDP.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

Dedicated worker bootstrapping now installs a separate bounded timer map,
running-timer map, timer-id allocator, cancellation functions, and due-turn
pump. The runtime's existing monotonic clock drives both page and worker
timer inspection without adding another scheduler. `NativeWorkerRegistry`
inspects all live worker queues in stable worker-id order, runs one due turn per
ready worker, and routes emitted messages/errors/lifecycle commands through its
existing queue.

Both host paths pump worker timers before draining pending worker messages into
the next page script. This ordering is important: the content process must
merge newly due messages before serializing the page evaluation prefix, or
worker output would be delayed an extra turn. The HTTP witness covers that
cross-process boundary.

## Tradeoffs and follow-up

The current scheduler is deliberately page-turn driven and bounded. It gives
deterministic testable delivery and preserves the two-crate ownership model,
but it is not yet a continuously running browser task-source scheduler. Timer
delivery while a page waits on unrelated asynchronous work, task-source
fairness, background throttling, worker Fetch/XHR, module/shared/service
workers, transferables, and complete Worker/Web IDL semantics remain issue #40
expansion work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked -- --nocapture` (10 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
