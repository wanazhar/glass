# Native engine browser-complete slice 460: local Blob classic scripts

- Status: complete locally
- Scope: `native-engine` / local non-network page dynamic scripts
- Issue: #40
- Depends on: [native-engine-browser-459](native-engine-browser-459.md)

## Objective

Allow a local `fixture:` or other non-network page to attach a classic script
whose `src` is owned by the page's Blob object-URL registry. The local engine
must consume those bytes directly and preserve the existing dynamic script
event and single-shot execution contract.

## Contract

- A dynamically attached classic script with a runtime-owned `blob:` URL runs
  in a local document without requiring a content-process network loader.
- Blob script MIME, source-size, UTF-8, and recognized SRI checks remain
  enforced; unavailable or revoked entries dispatch `error` and do not run.
- Successful execution dispatches the normal target `load` event after the
  script body, with existing event ownership and command handling preserved.
- HTTP(S) dynamic scripts and external/module local scripts retain their
  existing ownership and failure behavior.
- No network or HTTP-cache request is created for the local Blob resource.

## Implementation

- Added a bounded synchronous local Blob classic-script loader path.
- Converted local dynamic Blob external sources into the existing native
  script scheduler and resource-event queue.
- Kept inline/module-inline handling and process-backed external/module
  loading on their existing paths.

## Tradeoffs and remaining scope

The Blob body is copied into the local script turn; the runtime registry stays
the lifetime authority and this is not a zero-copy execution claim. Local
Blob module dependencies, local Blob stylesheets/images, media consumers,
popup/window transfer, and cross-realm object-URL sharing remain separate
issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_classic_script_runs_after_late_attachment -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_inline -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_classic_script_subresources -- --nocapture`
- `git diff --check`
