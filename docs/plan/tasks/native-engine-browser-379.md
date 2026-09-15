# Native WindowProxy structured transport (379)

```yaml
id: native-engine-browser-379
scope: native-engine/window-proxy-transport
status: done
depends-on:
  - native-engine-browser-378
```

## Objective

Remove generated JavaScript source from browser-owned WindowProxy
synchronization. Target URL, identity, name, closed state, and cache-key
metadata must cross local and content-process page owners as structured values
while preserving popup, navigation, close, and cross-origin contracts.

## Delivered behavior

- `NativeWindowProxyUpdate` records remain validated and queued as typed Rust
  metadata.
- The runtime now parses queued updates once with the structured payload
  boundary and calls the installed `__glassSyncWindowProxies` function directly.
- Classic page evaluation and module evaluation both use the same direct
  dispatcher; the old interpolated update-script builder is removed.
- A maximum-size 16 KiB target URL is accepted, applied to the matching cached
  proxy, and reported through the normal JavaScript API.
- Existing popup identity/navigation/close and content-process WindowProxy
  integration coverage remains green.

## Contract and tradeoffs

The structured bridge removes the accidental authored-source limit from
WindowProxy metadata, but does not make browser state unbounded. Per-field
URL, context-ID, name, count, JSON-envelope, IPC, and host-turn limits remain
authoritative. Cache-key matching still uses the existing serialized Glass
separator and target-context identity remains browser-owned; page script cannot
manufacture a trusted update. This slice does not add a resident event loop or
claim browser-wide task-source arbitration. Full Core Web Profile conformance,
recovery/cancellation, and cross-platform certification remain open issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-379.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib native_window_proxy_tests::window_proxy_updates_accept_maximum_url_without_script_source_coupling --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_window_proxy --locked -- --nocapture` — 3 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
