# Native WindowProxy transfer lists (385)

```yaml
id: native-engine-browser-385
scope: native-engine/window-proxy-message-transfer
status: done
depends-on:
  - native-engine-browser-384
```

## Objective

Carry the existing bounded structured-clone MessagePort transfer contract
through WindowProxy `postMessage`, so cross-window page messages do not lose
their transfer list when they cross a native frame, target, or content-process
boundary.

## Contract

- WindowProxy and `globalThis.postMessage` accept both the legacy third
  transfer-list argument and the options-object form.
- Message data and transfer descriptors cross the Rust boundary as one
  validated structured envelope.
- Receiving page `MessageEvent` instances decode transferred ports and expose
  the same port object when a transferred port also appears inside `event.data`.
- Source MessagePort endpoints are committed only after structured cloning
  succeeds; invalid clone data does not consume the transfer list.
- Existing target-origin, target-selection, size, bridge-key, and descriptor
  bounds remain enforced.

## Delivered behavior

- `NativeScriptCommand::PostMessage`, `NativePostMessageRequest`, and
  `NativePageMessageEvent` carry bounded transfer descriptors.
- Local and process-backed page dispatch pass the descriptors to the installed
  `__glassDispatchMessage` function, which uses the shared envelope decoder.
- WindowProxy option normalization shares the worker/service-worker transfer
  list parser and preserves the default `/` target-origin behavior.
- A popup integration witness exercises both overloads, source detachment,
  `event.ports`, and data/port identity.

## Tradeoffs and explicit follow-up

The transfer descriptors are now admitted by WindowProxy delivery, but the
receiving port's subsequent `postMessage` return route is not yet connected to
the browser-wide owner map; that remains a separate issue-40 slice rather than
silently dropping a command or pretending that a local bridge is cross-target.
ArrayBuffer detachment remains outside this slice. Full browser-wide task-source
arbitration and Core Web Profile certification remain open.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-385.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_window_proxy_post_message_transfers_message_ports --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine message --locked -- --nocapture` — 13 passed
- `cargo test --quiet -p glass-browser --test native_engine native_http_post_message_crosses_content_worker_and_replies --locked -- --nocapture` — 1 passed
- `cargo fmt --all -- --check`
- `git diff --check`

The first message-group run exposed a missing page `cloneMessageData` helper;
the helper was restored in the same batch and the rerun passed 13/13.

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only slice.
