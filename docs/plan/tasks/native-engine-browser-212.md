# Native engine browser slice 212: reactive image sources

Status: completed locally.

## Objective

Make external image resources follow page-script DOM mutations instead of
being limited to the initial HTML source tree.

## Contract

- After a content-worker script mutation, the native owner rediscovers the
  current bounded external `<img src>` set. Existing valid resources are
  retained when their node and source still match; a new source or a changed
  source is loaded through the same HTTP(S), credential, redirect, referrer,
  cookie, mixed-content, and CSP `img-src` policy as initial loading.
- Newly successful PNG responses are bounded, decoded to RGBA, attached to the
  mutated document, and included in the same typed document snapshot returned
  to the parent. A resource failure remains a non-fatal broken-image result.
- Successful dynamically loaded images receive the existing typed `load`
  event through the persistent page realm before the mutation is published.
  The image load can therefore update page state and event effects are carried
  in the normal bounded mutation response.
- The asynchronous mutation helper is shared by ordinary script evaluation
  and script-fetch callback mutations. The parent continues to publish one
  validated document revision and never starts a second renderer or fallback
  path.

## Implementation

`content_process::load_external_images` walks current external image links,
skips resources whose node/source identity is still valid, and attaches newly
decoded images to the Rust-owned document. `mutate_script_document` now runs
that hydration after applying a script command batch, evaluates a typed image
`load` event for each success, and serializes the final resource-bearing
snapshot. Initial page loading uses the same helper, keeping initial and
mutation resource ownership aligned.

## Tradeoffs and follow-up

Hydrating synchronously before returning the script mutation gives callers a
stable post-script snapshot and deterministic `load` ordering, but it makes a
script mutation wait for its image request and currently performs one
initial-load walk per mutation batch. The current image map retains resources
only while node/source identity remains valid; decoded HTTP caching, request
deduplication, freshness/revalidation, recursive loads caused by an image
handler, `HTMLImageElement.src` reflection, responsive sources, CSS/SVG image
resources, animation, and additional formats remain active issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_image_after_script_source_mutation -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_resource_load_events_before_dom_content_loaded -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `4abd9433`.
