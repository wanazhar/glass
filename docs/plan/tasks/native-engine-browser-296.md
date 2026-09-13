# Glass native engine browser slice 296: worker runtime primitives

Status: completed locally.

## Objective

Cover the standard encoding, cloning, event, and exception primitives that
ordinary dedicated-worker libraries use before they perform network or DOM
work.

## Contract

- Dedicated workers expose bounded UTF-8 `TextEncoder` and `TextDecoder`
  constructors, including `encodeInto()` and invalid-input handling within
  the native body-size limit.
- `atob()` and `btoa()` provide bounded padded base64 conversion for binary
  Latin-1 strings and reject invalid input rather than silently truncating it.
- `structuredClone()` returns a bounded JSON-backed clone, preserving the
  existing worker message-clone limits; `queueMicrotask()` schedules a
  callable in the existing QuickJS pending-job queue.
- `DOMException`, `Event`, `CustomEvent`, `MessageEvent`, and `ErrorEvent`
  constructors expose their common fields and stable constructor identity
  across worker host re-entry.
- `EventTarget` supports listener registration/removal, function and
  `handleEvent` callbacks, dispatch, cancelable default prevention, and
  immediate-stop behavior inside the worker realm.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker bootstrap reuses its bounded UTF-8 and base64 helpers for the
encoding APIs, preserves constructor objects through the existing re-entrant
bootstrap markers, and exposes structured clone through the same bounded
JSON clone used by worker messages. It installs exception and event
constructors with common fields, listener dispatch, cancellation state, and
single-target `EventTarget` propagation. No page document, network loader,
or cross-realm object is exposed by this slice.

The local witness covers multibyte UTF-8, partial `encodeInto()`, base64
round-tripping, clone data, microtask delivery, cancelable CustomEvent
dispatch, MessageEvent/ErrorEvent fields, DOMException identity, and the
constructor surface.

## Tradeoffs and follow-up

The APIs are useful for ordinary worker libraries while retaining bounded
memory and deterministic pending-job behavior. `structuredClone()` remains a
JSON-backed subset and therefore does not carry transferables, typed object
graphs, cyclic values, or host handles. Text decoding is UTF-8 focused, event
propagation is single-target, and complete Web IDL descriptors, transferable
cross-realm `MessagePort`s, automatic background task scheduling, and the
final native/CDP replacement gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_worker_exposes_standard_runtime_primitives --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (20 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
