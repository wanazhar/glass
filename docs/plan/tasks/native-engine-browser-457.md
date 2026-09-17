# Native engine browser-complete slice 457: Blob classic script subresources

- Status: complete locally
- Scope: `native-engine` / process-backed page classic external scripts
- Issue: #40
- Depends on: [native-engine-browser-456](native-engine-browser-456.md)

## Objective

Allow a page-created Blob URL to serve a classic external script after a
script mutation. The content-process owner must take a bounded snapshot from
the live page registry, keep HTTP(S) loading unchanged, and preserve the
ordinary script resource-event contract.

## Contract

- A runtime-verified page Blob URL can populate a dynamically created
  classic `<script src=...>` in a process-backed page.
- Document `script-src` policy is evaluated before execution. A disallowed
  Blob script is rejected through the normal resource-error path.
- Blob script content type, maximum source size, UTF-8 decoding, and
  subresource-integrity checks remain enforced.
- A successfully loaded Blob script executes once and delivers its normal
  target `load` event; an unavailable, revoked, blocked, invalid, or failed
  source remains an error and never falls back to HTTP or the script cache.
- Temporary DOM script nodes retain listener identity when their native
  committed index is assigned, so listeners installed before append remain
  observable after the content-process turn.
- Existing HTTP(S), parser-inserted, module, worker, CSP-reporting, redirect,
  cache, and resource-order behavior remains on its existing owner path.

## Implementation

- Added an object-URL-aware script loader entry point and passed live runtime
  Blob snapshots into process-backed dynamic page script resolution.
- Reused the existing script CSP/report-only, MIME, size, integrity, UTF-8,
  execution, and resource-event owners; Blob sources bypass HTTP transport
  and HTTP script-cache lookup.
- Added stable element event-owner keys and routed normal/inline element
  listeners through them, preserving callbacks across temporary-to-committed
  DOM node remapping.

## Tradeoffs and remaining scope

This is a bounded copy from the page registry into the script executor. It
does not claim zero-copy sharing or a cross-process live Blob registry. The
slice is limited to process-backed classic external scripts; Blob stylesheet
loading, module dependency graphs, media, popup/window transfer, local-inline
dynamic consumers, and broader cross-realm object-URL sharing remain separate
issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_classic_script_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_dynamic_inline_script_runs_once_after_late_attachment -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_runs_nested_dynamic_external_scripts -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_resolves_dynamic_script_fetch -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_classic_external_scripts_in_document_order -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_enforces_subresource_integrity_for_scripts_and_styles -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_dispatches_resource_load_events_before_dom_content_loaded -- --nocapture`
- `git diff --check`
