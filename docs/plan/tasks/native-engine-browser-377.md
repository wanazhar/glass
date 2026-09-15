# Native same-origin frame transport (377)

```yaml
id: native-engine-browser-377
scope: native-engine/same-origin-frame-transport
status: done
depends-on:
  - native-engine-browser-376
```

## Objective

Remove generated JavaScript source from same-origin frame command and event
delivery. Frame IDs, node generations, event metadata, and DOM command values
must cross the page-owner boundary as structured data in local and
content-process realms.

## Delivered behavior

- Child-frame event descriptors use `NativeFrameEventBatch` records in the
  serialized `NativePageEventBatch` instead of interpolated JavaScript.
- Parent-issued frame commands use the same typed page-event boundary and
  enter the installed `__glassApplyNativeCommand` or
  `__glassQueueNativeCommands` function through parsed values.
- Frame command validation preserves the existing DOM-only allowlist, batch
  count, command-size, same-origin, node-generation, and IPC/document limits.
- The static `undefined;` continuation preserves the existing serialized
  child-owner ordering and effect propagation.
- The HTTP same-origin frame witness commits and reads back a 16,300-byte
  `innerHTML` value. The former generated command source would be 16,393
  bytes, exceeding the 16 KiB authored-script limit.

## Contract and tradeoffs

The structured bridge removes coupling between user-controlled frame payloads
and the authored JavaScript source budget. It does not make the frame surface
unbounded: values remain JSON-backed, command batches remain finite, and the
existing content-process IPC and document-wire limits still apply. Same-origin
authorization and frame-local node identity remain enforced by the browser
owner and the receiving runtime. This slice does not add a resident event loop
or claim browser-wide task-source arbitration; Core Web Profile conformance,
recovery/cancellation, and cross-platform certification remain open issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-377.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_same_origin_frame_script_projection_matches_window_contract --locked -- --nocapture` — 1 passed
- `git diff --check`

The nested-frame contract was bounded at 90 seconds during investigation and
did not produce a test assertion or compiler failure; it is not counted as
passing evidence for this checkpoint. Remote CI, push, release, tag, and
registry publication evidence are not part of this local-only checkpoint.
