# Native structured-clone message transport (384)

```yaml
id: native-engine-browser-384
scope: native-engine/structured-clone-message-transport
status: done
depends-on:
  - native-engine-browser-383
```

## Objective

Replace the native message layer's JSON-only value projection with a bounded
structured-clone representation that can cross the existing JSON-framed Rust
boundary without losing browser-visible cloneable values, cycles, or object
identity.

## Contract

- Page, dedicated-worker, shared-worker, Service Worker, MessagePort,
  BroadcastChannel, and WindowProxy message paths use the same clone owner.
- `undefined`, non-finite and negative-zero numbers, BigInt, Date, RegExp,
  Map, Set, cycles, shared references, ArrayBuffer, typed arrays, DataView,
  Blob/File metadata and bytes, and Error values survive a clone round trip.
- Local recipients decode an independent graph; BroadcastChannel recipients do
  not share a mutable decoded object.
- Existing MessagePort transfer descriptors and owner-routed bridge keys remain
  unchanged, and source ports are committed only after cloning succeeds.
- Functions, symbols, Promise-like objects, SharedArrayBuffer, malformed
  descriptors, and untransferred MessagePorts fail with `DataCloneError`.
- The payload remains bounded by the existing native message-size limit and
  remains JSON-safe at the Rust IPC boundary.

## Delivered behavior

- A tagged graph encoder preserves references through node IDs and represents
  sparse arrays, primitive edge values, binary buffers/views, maps, sets,
  regular expressions, dates, errors, and native Blob/File values.
- A validating decoder reconstructs the corresponding native values in the
  receiving realm, safely defines object keys such as `__proto__`, and keeps
  shared ArrayBuffer/view identity.
- `structuredClone()` and page/worker message helpers now use the shared graph
  owner. Host-dispatched page messages decode the graph before creating the
  `MessageEvent`.
- Local MessagePort and BroadcastChannel delivery decode fresh graph instances.
- The transfer-list rollback regression now uses a function as its intentionally
  uncloneable value; cyclic values are covered as supported behavior.
- A native page/worker integration witness covers rich values, cycles, shared
  identity, local and cross-realm delivery, and binary data.

## Tradeoffs and explicit follow-up

The graph adds metadata and performs a second decode, so small messages are
larger and richer messages use more CPU than the old JSON projection. The
existing size limit and binary-byte budget prevent that overhead from becoming
unbounded. ArrayBuffer transfer-list detachment is not implemented in this
slice; ArrayBuffers are cloned, while MessagePort transfer remains the only
committed detachable bridge. SharedArrayBuffer is rejected rather than
silently downgraded. Prototype/descriptor parity beyond the supported
structured-clone types, full Web IDL identity, and browser-wide task-source
arbitration remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-384.md`

## Verification

The following checks passed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_message_transport_preserves_structured_clone_values --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine message --locked -- --nocapture` — 12 passed
- `cargo test --quiet -p glass-browser --test native_engine standard_runtime_primitives --locked -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — 17 passed
- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-384.json >/dev/null`
- `python3 scripts/check-documentation-depth.py >/dev/null`
- `python3 scripts/check-tui-shortcuts.py >/dev/null`
- `python3 scripts/check-documentation-coverage.py >/dev/null`
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
