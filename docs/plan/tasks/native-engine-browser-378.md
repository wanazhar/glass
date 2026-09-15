# Native hash-change transport (378)

```yaml
id: native-engine-browser-378
scope: native-engine/hash-change-transport
status: done
depends-on:
  - native-engine-browser-377
```

## Objective

Remove generated JavaScript source from same-document hash-change delivery.
Old/new URL values must cross local and content-process page owners as
structured metadata while preserving the existing navigation, history,
handler, and re-entry contracts.

## Delivered behavior

- Hash-change metadata uses `NativeHashChangeEvent` records in the typed
  `NativePageEventBatch`.
- Local and HTTP(S) fragment navigation invoke the installed
  `__glassDispatchHostEvents` function through a static `undefined;`
  continuation rather than embedding URLs in generated source.
- URL shape, same-document/different-fragment validation, history mutation,
  handler-generated commands, and re-entry navigation remain unchanged.
- Local and content-process witnesses pass an 8.2 KiB fragment pair. The old
  generated dispatch source for that pair is larger than the 16 KiB authored
  script budget.

## Contract and tradeoffs

The structured bridge removes source-size coupling for hash-change URLs, but
does not make navigation metadata unbounded. Both URLs remain validated text,
the page-event envelope remains JSON-backed, and the existing IPC,
document-wire, revision, history, and operation limits still apply. The event
continues to run on the current serialized page host turn; this slice does not
add a resident event loop or claim browser-wide task-source arbitration. Full
Core Web Profile conformance, recovery/cancellation, and cross-platform
certification remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-378.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_hashchange_accepts_large_urls_without_source_coupling --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_fragment_navigation_dispatches_hashchange_in_place --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine hashchange --locked -- --nocapture` — 6 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
