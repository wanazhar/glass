# Glass native engine browser slice 276: rejection settlement events

Status: completed locally.

## Objective

Complete the bounded Promise rejection notification pair by reporting when a
previously delivered unhandled rejection later acquires a handler, without
turning pre-checkpoint handled promises into false positives or allowing
reported state to grow without bound.

## Contract

- Rejections handled before the first rejection checkpoint remain absent from
  both event surfaces.
- A rejection already delivered through `unhandledrejection` is retained by
  bounded promise identity until it is handled or evicted by the cap.
- A later handler queues one `rejectionhandled` notification for that promise.
- Multiple settlement notifications preserve host callback order.
- `rejectionhandled` dispatches a non-bubbling, non-cancelable
  `PromiseRejectionEvent` to the page window through both listeners and
  `window.onrejectionhandled`.
- The event exposes the bounded original reason text and the current
  `promise === null` bridge placeholder.
- Reported and settlement queues remain bounded; dropped records do not make
  the host fail or retain unbounded page state.
- Runtime/bootstrap/dispatch failures remain typed native errors.

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
- `docs/plan/tasks/native-engine-browser-276.md`

## Implementation

The QuickJS host rejection tracker now maintains pending, reported, and
settlement queues behind one bounded state owner. A rejection handled before
delivery is removed from `pending`; after delivery, its promise key and
bounded reason move out of `reported` into the ordered settlement queue. The
reported set evicts its oldest order when capacity is reached, while the
settlement queue remains bounded as well.

The host dispatch helper now carries an allow-listed event type and explicit
cancelability into the realm. The existing native `PromiseRejectionEvent`
constructor and event-state machinery deliver `unhandledrejection` as
cancelable and `rejectionhandled` as non-cancelable. The bootstrap installs the
corresponding `onrejectionhandled` property without replacing handlers during
refresh.

## Tradeoffs and follow-up

The Rust/QuickJS bridge still does not retain or expose the original Promise
object; page code observes `promise === null`. Reasons remain bounded strings,
so object identity and structured-clone semantics are not represented.
Settlement delivery occurs at the existing explicit evaluation/event-turn
checkpoints, not yet through a complete browser task-source scheduler. Full
Promise rejection Web IDL descriptors, structured identity, parser-accurate
script timing, and complete event-loop parity remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_unhandled_rejection_dispatches_window_event --locked -- --exact --test-threads=1` (1 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
