# Native cross-window message dispatch (376)

```yaml
id: native-engine-browser-376
scope: native-engine/page-cross-window-message-transport
status: done
depends-on:
  - native-engine-browser-375
```

## Objective

Remove generated JavaScript source from browser-owned cross-window
`postMessage` delivery. Message data and source metadata must reach the
installed page dispatcher as structured values in both local and
content-process realms.

## Delivered behavior

- Browser-owned message deliveries use `NativePageMessageEvent` records in the
  serialized `NativePageEventBatch` rather than an interpolated source string.
- Local and HTTP(S) content-process page turns parse the bounded event record
  once and call `__glassDispatchMessage` directly in the existing QuickJS
  owner.
- Existing target-origin checks, WindowProxy source identity, message queue
  limits, and structured-clone bounds remain in force.
- Local and HTTP(S) witnesses deliver a 20,000-byte message payload, which is
  larger than the old 16 KiB authored-script source budget, without failing
  delivery or changing the reply round trip.
- The unused generated `host_message_event_script` helper is removed.

## Contract and tradeoffs

This removes source-size coupling for cross-window message delivery, but the
message remains JSON-backed and bounded by the existing native structured
message limit. Transferable MessagePort delivery continues to use its own
structured event path, and source/origin validation remains explicit. The
message still runs on the current serialized page host turn; this slice does
not introduce a resident event loop or claim browser-wide task-source
arbitration. Full Core Web Profile conformance, recovery/cancellation, and
cross-platform certification remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-376.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine post_message --locked -- --nocapture` — 3 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
