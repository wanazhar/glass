# Glass native engine browser slice 278: bounded idle callbacks

Status: completed locally.

## Objective

Expose a usable bounded `requestIdleCallback()` surface in the native page
realm, including cancellation and timeout-aware deadlines, while keeping idle
work on the existing serialized host turn.

## Contract

- `requestIdleCallback(callback)` accepts a callable and returns a stable
  bounded callback ID.
- An optional object `timeout` is normalized to a finite non-negative delay;
  invalid option shapes fail explicitly.
- `cancelIdleCallback(id)` removes a pending callback without invoking it.
- A due callback receives an `IdleDeadline`-shaped object with boolean
  `didTimeout` and finite non-negative `timeRemaining()` output.
- Callbacks execute with the page window as `this`, once, after due timers and
  animation-frame callbacks in the same host turn.
- Expired timeout callbacks report `didTimeout === true`; callbacks without a
  timeout report `false` in the bounded idle turn.
- Idle callback state is retained across bootstrap refreshes and bounded by
  the existing timer/effect limit.

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
- `docs/plan/tasks/native-engine-browser-278.md`

## Implementation

The bootstrap now keeps `idleCallbacks` and a monotonic ID counter in the
persistent realm. The bounded scheduler includes idle timeout deadlines when
the host asks for its next turn, runs due idle callbacks after timers and
animation frames, and constructs a deadline object whose budget is measured
against the current `performance.now()` value. The callback and cancellation
functions are installed on every refreshed window projection without replacing
the retained map.

## Tradeoffs and follow-up

This is a host-turn contract, not a full browser scheduler. There is no
background page loop, visibility/throttling policy, task-source fairness,
real renderer idle-budget arbitration, or cross-document idle coordination.
The deadline is bounded and deterministic rather than a renderer-provided
budget. Complete `IdleDeadline`, animation, and scheduling Web IDL semantics
remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_idle_callbacks_honor_cancellation_and_deadline --locked -- --exact --test-threads=1` (1 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
