# Native Promise-rejection dispatch (374)

```yaml
id: native-engine-browser-374
scope: native-engine/page-promise-rejection-transport
status: done
depends-on:
  - native-engine-browser-373
```

## Objective

Remove generated JavaScript source from page `unhandledrejection` and
`rejectionhandled` delivery. Host-owned rejection batches must reach the
installed page dispatcher as structured values after the microtask checkpoint.

## Delivered behavior

- Unhandled and handled rejection reason descriptors are parsed once as
  bounded QuickJS values and passed to `__glassDispatchPromiseRejections`.
- The old aggregate source-size check and JSON interpolation path are gone;
  the existing dispatcher still owns event construction, handler/listener
  order, cancelability, and event flags.
- The existing 4,096-character per-reason bound and bounded rejection queue
  remain authoritative, and both ordinary and module evaluation paths use the
  same direct dispatch helper.
- A local fixture witness dispatches five 4,096-character unhandled reasons in
  one batch, which is larger than the old 16 KiB generated-source budget.

## Contract and tradeoffs

This removes source-size coupling without changing the JSON-backed reason-text
projection: rejected Promise identity remains represented as `null` by the
existing page dispatcher, and reason text is still truncated to the existing
bound. The structured values remain finite and are delivered on the current
serialized JavaScript turn; no resident background loop or browser-wide
task-source arbitration is introduced. Full Core Web Profile conformance,
recovery/cancellation, and cross-platform certification remain open issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-374.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine rejection --locked -- --nocapture` — 3 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
