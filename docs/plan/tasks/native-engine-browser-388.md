# Native same-realm transferable ports (388)

```yaml
id: native-engine-browser-388
scope: native-engine/same-realm-transferable-ports
status: done
depends-on:
  - native-engine-browser-387
```

## Objective

Close the same-realm transferable-port gap in the native structured-clone
owner. A local MessageChannel and `structuredClone()` must be able to transfer
an endpoint without routing through a fake cross-process bridge or losing the
entanglement with the original peer.

## Contract

- A same-realm `MessagePort.postMessage()` accepts bounded ArrayBuffer and
  MessagePort transfer members, while retaining the existing clone, duplicate,
  queue, and detached-source checks.
- A transferred local MessagePort is represented by a fresh receiving endpoint
  entangled with the original peer. The source endpoint is closed and removed
  from the realm registry only after the clone graph has decoded successfully.
- Queued messages already waiting on the source endpoint move to the fresh
  endpoint, and the endpoint start state remains observable through normal
  MessagePort delivery.
- `structuredClone(value, { transfer })` supports both ArrayBuffer and local
  MessagePort transfer members. The returned graph preserves `event.data` /
  endpoint identity, and source ArrayBuffers are detached after clone
  admission.
- Cross-realm and content-process MessagePort routes continue to use the
  browser-owned bridge descriptors and do not receive a same-realm shortcut.
- Typed arrays/DataViews remain cloneable values but are rejected as transfer
  list members; invalid, duplicate, detached, and SharedArrayBuffer members
  fail closed with `DataCloneError`.

## Delivered behavior

- Added local transfer preparation and commit around the existing tagged graph
  encoder/decoder. Local ports are created only after graph encoding, then
  re-entangled and committed after decode.
- Moved queued messages and the started state when an endpoint is transferred;
  local delivery now exposes transferred endpoints in `event.ports` and
  preserves the data/port identity relationship.
- Removed the old local-port transfer rejection while keeping remote bridge
  routing unchanged.
- Extended the structured-clone options path to admit local MessagePort
  transfers alongside ArrayBuffer transfers.
- Added a deterministic witness for local port transfer, local ArrayBuffer
  transfer, structuredClone port transfer, source invalidation, and reply
  delivery.

## Tradeoffs and explicit follow-up

Same-realm transfer allocates a fresh endpoint object and moves bounded queued
records, so it does not expose the original JavaScript object identity and is
not zero-copy for the message graph. Keeping the original peer and registry
ownership inside the realm avoids unnecessary Rust/browser-route bookkeeping;
cross-target transfers still use the validated browser-owned bridge. The
implementation assumes the existing serialized QuickJS turn, so validation,
decode, and re-entanglement cannot interleave with another local transfer.

Transferability for other platform objects and transferable streams, full
MessagePort Web IDL semantics, browser-wide task-source ordering, broader Core
Web Profile parity, and final cross-platform/security/performance/production
certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/README.md`
- `docs/plan/tasks/native-engine-browser-388.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_message_ports_support_transfer_lists_and_structured_clone --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine message --locked` — 16 passed
- `cargo test --quiet -p glass-browser --test native_engine structured_clone --locked` — 3 passed
- `cargo fmt --all -- --check`
- documentation depth, coverage, TUI-shortcut, and release-documentation validators
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
