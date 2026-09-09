---
id: native-engine-browser-049
scope: glass-browser/native-engine/repeating-timer-turns
status: done
depends-on: [native-engine-browser-048]
---

# BE-01l/BE-04ae: bounded repeating timer turns

## Objective

Extend the persistent native timer host with bounded `setInterval` and
`clearInterval` behavior while preserving deterministic host-turn execution.

## Contract

- Local and child-owned realms expose `setInterval` and `clearInterval`.
- A due repeating timer runs at most once in one host turn, then reschedules
  from the host time observed by that turn.
- Due timers retain due-time/ID ordering; a callback can cancel itself through
  `clearInterval`, and pending timers can be canceled before their due turn.
- Interval callbacks retain their realm closure and typed DOM-command bridge;
  no executable state crosses the owner boundary.

## Deliberate boundary and tradeoffs

Repeating timers use the existing host-turn pump. A callback is therefore
observable only when a later Glass evaluation or action supplies a host turn;
the engine does not create an unbounded background loop, wake a page on wall
clock expiry, or claim HTML task-source fairness. Zero-delay intervals are
bounded to one callback per supplied turn, and animation/idle callbacks remain
open.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_interval_reschedules_until_cleared`
- `native_content_process_interval_reschedules_until_cleared`
- existing zero-delay microtask/timer regression
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
