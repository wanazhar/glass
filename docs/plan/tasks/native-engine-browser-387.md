# Native ArrayBuffer transfer and detachment (387)

```yaml
id: native-engine-browser-387
scope: native-engine/array-buffer-transfer-detachment
status: done
depends-on:
  - native-engine-browser-386
```

## Objective

Close the ArrayBuffer transfer-list gap in the native structured-clone owner.
ArrayBuffer bytes must cross every existing bounded message boundary while the
sender observes true detached-buffer behavior only after the clone has been
successfully encoded and admitted.

## Contract

- Page and worker realms use QuickJS-backed host hooks to distinguish attached
  ArrayBuffers and to detach them without exposing a Rust-owned backing
  pointer to the JSON-framed transport.
- Worker/global `postMessage()`, bridged `MessagePort` delivery, Service Worker
  message paths, and WindowProxy `postMessage()` accept ArrayBuffer members in
  their transfer lists. Typed arrays and DataViews remain cloneable values but
  are rejected as transfer-list members.
- The structured-clone graph encodes all reachable bytes and shared
  ArrayBuffer/view identity before any source buffer is detached. A failed
  clone, size check, invalid transfer member, duplicate member, or already
  detached buffer leaves the transfer sources unchanged.
- `structuredClone(value, { transfer: [arrayBuffer] })` returns a fresh clone
  and detaches the listed source. An options object without a transfer list
  retains ordinary clone behavior; MessagePort transfer remains owned by the
  message-delivery bridge.
- Detached ArrayBuffers fail closed with `DataCloneError` on later clone or
  transfer attempts. `SharedArrayBuffer` remains rejected by the existing
  policy.
- The existing JSON-safe payload, binary-byte, transfer-list, command, effect,
  target-origin, and content-process bounds remain authoritative.

## Delivered behavior

- Installed bounded `__glassNativeArrayBufferIsAttached` and
  `__glassNativeDetachArrayBuffer` host functions in page, dedicated/shared
  worker, Service Worker, and content-process QuickJS realms; captured hooks
  are released from the global host surface after bootstrap installation.
- Extended the common transfer preparation path to validate, encode, and
  commit ArrayBuffer transfers alongside existing MessagePort descriptors.
- Added structured-clone transfer options for ArrayBuffers and preserved the
  existing graph decoder so receiver buffers and typed views are independent
  while retaining their internal aliasing.
- Added page-to-worker, WindowProxy popup, and HTTP content-process witnesses
  covering byte preservation, view identity, sender detachment, invalid view
  members, detached-source rejection, and structuredClone transfer behavior.

## Tradeoffs and explicit follow-up

The native wire contract stays unchanged: transfer buffers are represented by
the existing bounded byte graph, so a receiver gets a fresh buffer and the
sender pays the bounded copy cost before detachment. This avoids sharing
QuickJS backing storage across realms or processes, at the cost of not
providing zero-copy transfer. Host hooks are installed only for the lifetime
of bootstrap setup and retained by the message closures, limiting accidental
script access to the Rust detach primitive. Multiple buffers are validated
before commit; the bounded single-threaded QuickJS turn makes the subsequent
detachment sequence deterministic.

Complete transferability for other platform objects, transferable streams,
browser-wide task-source ordering, broader Core Web Profile parity, and final
cross-platform/security/performance/production certification remain issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/README.md`
- `docs/plan/tasks/native-engine-browser-387.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine array_buffer_transfer --locked` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_transfers_array_buffers_between_page_and_worker_realms --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine message --locked` — 15 passed
- `cargo test --quiet -p glass-browser --test native_engine structured_clone --locked` — 2 passed
- `cargo fmt --all -- --check`
- documentation depth, coverage, TUI-shortcut, and release-documentation validators
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
