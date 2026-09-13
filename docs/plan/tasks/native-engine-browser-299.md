# Glass native engine browser slice 299: page runtime primitives

Status: completed locally.

## Objective

Make ordinary document realms expose the standard byte/text, cloning, and
event-target primitives already required by common page libraries. These
surfaces must use the existing document realm and event owner rather than
introducing a second execution or transport path.

## Contract

- Page `TextEncoder` and `TextDecoder` support UTF-8, `encodeInto`, bounded
  `ArrayBuffer`/view input, and explicit unsupported-encoding errors.
- Page `atob` and `btoa` use the existing bounded base64 implementation and
  preserve the Latin-1 validation boundary.
- Page `structuredClone` uses the existing bounded clone owner and fails
  explicitly when a value cannot cross the JSON-backed contract.
- Page `EventTarget` creates independently owned targets, dispatches through
  the existing event path, supports function and `handleEvent` listeners,
  honors capture/once/default-prevention behavior, and removes the original
  listener identity correctly.
- Page `queueMicrotask` remains host-turn compatible and is covered with the
  new primitives.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The page bootstrap installs persistent constructor identities after its event
dispatcher is available. Text and binary methods reuse the existing bounded
Blob/base64/UTF-8 helpers; `EventTarget` assigns a unique internal owner key
and routes listeners through the page listener registry. Listener records now
retain the original object/function so `removeEventListener` can remove a
`handleEvent` object without leaking its wrapper.

## Tradeoffs and follow-up

Reusing the current page helpers keeps resource limits and event semantics in
one place, but the clone contract remains JSON-backed and therefore does not
yet preserve transferables, cycles, Maps/Sets, typed-array identity, or
prototype identity. Text decoding is UTF-8-only and base64 remains bounded.
Those are explicit Web IDL/structured-clone follow-ups, not silently claimed
browser parity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_exposes_standard_runtime_primitives --locked -- --nocapture`
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
