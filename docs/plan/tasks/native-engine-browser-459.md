# Native engine browser-complete slice 459: Blob module dependencies

- Status: complete locally
- Scope: `native-engine` / process-backed dynamic page modules
- Issue: #40
- Depends on: [native-engine-browser-458](native-engine-browser-458.md)

## Objective

Allow a dynamically inserted page module backed by a Blob URL to import other
runtime-owned Blob modules. The content-process owner must preserve the
bounded module graph contract and must not turn a missing Blob dependency into
an HTTP or cache request.

## Contract

- A dynamic `<script type="module">` whose source is a runtime-owned Blob URL
  can execute after the existing document policy checks.
- Absolute `blob:` static imports are resolved, snapshotted from the owning
  page runtime, and evaluated through the existing bounded module graph.
- Blob dependency loads reuse CSP, Blob-origin, MIME, source-size, UTF-8,
  deduplication, and graph-limit checks.
- Missing, revoked, blocked, malformed, or over-limit dependencies fail the
  module evaluation without network or HTTP-cache fallback.
- Existing HTTP(S), relative network imports, document order, failure, and
  evaluation behavior remain on their existing paths.

## Implementation

- Passed the live page JavaScript runtime into process-backed module
  dependency discovery.
- Added absolute Blob-specifier resolution and runtime object-URL snapshots
  to dependency loading while retaining the existing network loader path.
- Reused the current bounded dependency map, cycle/deduplication behavior, and
  module evaluation owner.

## Tradeoffs and remaining scope

Dependency source bytes are copied into the content-process module snapshot;
this is a bounded ownership handoff rather than a zero-copy shared registry.
This slice intentionally does not define relative Blob-derived dependency
naming, parser-created cross-realm Blob modules, media consumers, popup/window
transfer, local-inline dynamic consumers, or broader cross-realm object-URL
sharing.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_module_dependencies -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_external_modules_in_document_order -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_prefetches_static_module_graphs -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_isolates_static_module_dependency_failure -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_reports_external_module_evaluation_failure_without_aborting_document -- --nocapture`
- `git diff --check`
