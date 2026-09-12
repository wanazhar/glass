# Glass native engine browser slice 270: resilient optional resources and navigation metadata

Status: completed locally.

## Objective

Make ordinary document loading resilient to failed optional HTTP(S)
subresources, and carry the navigation metadata that page code needs to
distinguish normal replacement from a BFCache restore.

## Contract

- A failed external stylesheet does not abort the owning HTML document. The
  stylesheet element receives one bounded non-bubbling `error` event and the
  document continues through its normal ready-state and window-load phases.
- A failed external classic or module-root script does not abort the owning
  HTML document. The script element receives one bounded non-bubbling `error`
  event and no failed source is evaluated.
- Successful stylesheet and external-script loads retain their existing
  element `load` events.
- Normal replacement navigation and child-frame lifecycle injection expose a
  `persisted` event property with value `false`; the native path does not claim
  BFCache restoration for these transitions.
- The resource boundary remains typed and bounded: failures are observable as
  resource events, while the main document remains the authoritative commit.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-270.md`

## Implementation

The content-process document loader now converts stylesheet and external-script
fetch, policy, MIME, and bounded-size failures into per-element `Error`
events. It only adds successfully fetched stylesheets to the parsed style
inputs and only schedules successfully fetched scripts for evaluation. The
resource-event list retains deterministic document order, so load and error
events cross the existing persistent-runtime boundary alongside the normal
ready-state sequence.

The shared host-event descriptors now include `persisted`, and the JavaScript
bootstrap carries it onto every event object. Normal navigation descriptors
send `false`; the same field is present for projected nested-frame lifecycle
events so local and content-process realms expose the same contract.

## Tradeoffs and follow-up

This slice deliberately isolates optional resource failure from the main
document commit. Static module dependency failures after a successfully loaded
module root still use the existing bounded module-graph error path; they need a
separate module error-event/failed-evaluation contract. Resource scheduling is
still deterministic and serialized rather than browser-grade concurrent, and
the native engine does not yet implement BFCache restoration, full resource
timing, or complete lifecycle/Web IDL parity. Those remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_full_navigation_orders_page_lifecycle_events --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_orders_navigation_lifecycle_events --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_keeps_document_alive_when_optional_resources_fail --locked -- --test-threads=1 --nocapture --exact` (1 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
