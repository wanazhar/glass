---
id: native-engine-browser-143
scope: glass-browser/native-engine/download-ownership
status: completed
depends-on: [native-engine-browser-142]
---

# Native engine browser slice 143: anchor download ownership

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make an ordinary HTML anchor download usable through the native browser
ownership model. A semantic click queues the download without navigating the
current page, and an explicit operation completes it into an already-
authorized directory without Chromium, CDP, or hidden browser state.

## Contract

- anchors with a `download` attribute remain in the content-process click and
  script-navigation event path;
- allowed activation queues a bounded parent-owned download record and leaves
  the committed page URL unchanged;
- the navigation-mode loader supports cross-origin HTTP(S) download bytes,
  follows bounded redirects, and never exposes those bytes to page script;
- transfer size is capped at 16 MiB and completion has a bounded deadline;
- destination directories must already exist and are selected by the caller;
- file names are sanitized, collision-free, and written with exclusive file
  creation; partial files are removed on write failure;
- completion returns a stable native GUID, filename, byte counts, SHA-256,
  target/frame ownership, and bounded list/cancel bookkeeping;
- runtime, CLI, MCP, and `DownloadStarted` verification reach the same native
  owner;
- non-download anchors retain the existing external-link navigation behavior;
- no CDP fallback, hidden browser process, or third crate is introduced.

## Tradeoffs

The click boundary only queues the transfer; the explicit download operation
performs the bytes and file write. This keeps action latency and file-system
authorization separate and lets callers choose a destination. The current
public backend contract can list and cancel queued IDs while the
runtime/CLI/MCP download operation completes the oldest queued transfer.

The transfer is intentionally bounded and parent-owned. It covers ordinary
HTTP(S) anchor downloads and cross-origin bytes, but does not yet imply chooser
UI, programmatic blob/object-URL downloads, streaming/progress events,
service-worker interception, or multi-target/frame topology. Those surfaces
remain explicit follow-on issue #40 work rather than being silently treated as
complete.

## Implementation surface

- `browser/native_engine/resource_loader.rs`: navigation-mode download fetch,
  redirect/CSP handling, and a 16 MiB response limit;
- `browser/native_engine/dom.rs`: download-attribute ownership lookup;
- `browser/native_engine/engine.rs`: bounded queue, activation routing,
  collision-safe file writer, completion digest, and IDs;
- `browser/native_backend.rs`, `browser/runtime.rs`: backend and verification
  projection;
- `cli/runner.rs`, `mcp/server.rs`: native download dispatch;
- `tests/native_engine.rs`: cross-origin transfer regression plus timer/form
  regressions found by the full native gate;
- synchronized architecture, analysis, and plan records.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_external_download_link_transfers_cross_origin_bytes -- --exact --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine
```

Completed evidence:

- formatting completed successfully;
- the feature-gated `glass-browser` test-target check passed;
- the cross-origin download test passed, including queue ownership, bounded
  transfer, SHA-256 completion evidence, and file contents;
- the full native integration target passed: 411 tests, 0 failures;
- a flaky interval assertion and a real `.value` mutation bug for non-text
  controls were repaired during the gate; both targeted regressions pass;
- no Chromium endpoint, CDP fallback, or third crate was introduced.

Popup/new-target behavior, child-frame ownership, chooser/programmatic
downloads, per-resource lifecycle events, universal workflow parity, and
native production promotion remain issue #40 work.
