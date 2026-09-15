# Native page script-error dispatch (375)

```yaml
id: native-engine-browser-375
scope: native-engine/page-script-error-transport
status: done
depends-on:
  - native-engine-browser-374
```

## Objective

Remove generated JavaScript source from page script-error reporting. Host-owned
error descriptors must reach the installed page `ErrorEvent` dispatcher as
structured data while preserving script isolation and document continuation.

## Delivered behavior

- Page script-error descriptors are parsed once at the QuickJS boundary and
  passed directly to `__glassDispatchScriptError`.
- Initial, dynamic, and post-mutation page script execution use the same direct
  dispatch method; the old source interpolation helper is removed.
- The existing bounded message, filename, node identity, line/column, target
  error, window error, and failed-script lifecycle behavior remain unchanged.
- A local fixture witness dispatches five 4,096-character script-error
  messages in one document, exceeding the old 16 KiB generated-source budget
  while allowing the document to finish loading.

## Contract and tradeoffs

The descriptor no longer consumes the authored JavaScript source budget, but
error messages remain bounded text and the existing `ErrorEvent` projection is
preserved. Dispatch still occurs on the serialized page owner turn; no
resident background loop or browser-wide task-source arbitration is introduced.
Full Core Web Profile conformance, recovery/cancellation, and cross-platform
certification remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-375.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_dispatches_large_script_error_batch_as_data --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_inline_script_failure_dispatches_error_without_aborting_document --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
