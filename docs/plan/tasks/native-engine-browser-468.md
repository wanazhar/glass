# Native engine browser-complete slice 468: bounded media-resource admission

- Status: complete locally
- Scope: `native-engine` / Blob-backed `<audio>` and `<video>` resource ownership
- Issue: #40
- Depends on: [native-engine-browser-467](native-engine-browser-467.md)

## Objective

Close the first media-resource boundary without pretending that metadata
admission is a decoder. Local fixture documents and HTTP(S) content-process
documents must be able to select a runtime-owned Blob URL for an audio/video
element, apply the document media policy, retain bounded resource metadata,
and expose a coherent DOM state and event result.

## Contract

- `<audio>` and `<video>` select their own non-empty `src` first, then the
  first direct `<source>` child whose declared type is supported.
- Blob URLs are resolved only through the owning JavaScript realm's bounded
  object-URL registry. The media path never falls through to HTTP transport or
  the HTTP cache.
- HTTP(S) documents apply `media-src` with `default-src` fallback, record
  report-only observations through the existing policy owner, and fail closed
  when the enforced policy denies the Blob URL. Non-network fixture owners
  retain the existing local policy boundary.
- Supported declared MIME types and bounded sniffed payloads are admitted only
  within `MAX_NATIVE_MEDIA_BYTES`; WAV payloads expose duration when a bounded
  RIFF/fmt/data header provides a valid byte rate and data length.
- The document wire and persistent JavaScript snapshot carry selected source,
  content type, byte length, optional duration, readiness/network state, and a
  stable media error code. `load`/`error` events use the existing native event
  path, and `HTMLMediaElement` exposes bounded constants, `load()`,
  `canPlayType()`, time ranges, and state accessors.
- Codec decoding, playback clock progression, audio/video output, static HTTP
  media transfer, and full media/Web IDL parity remain explicit future issue
  #40 gates.

## Implementation

- Added bounded media metadata admission and MIME/sniff/WAV-duration helpers
  to the native resource loader, including Blob-origin validation and the
  document media CSP owner for HTTP(S) content-process documents.
- Added selected media-source discovery, per-node load/resource/error state,
  validation, serialization, restoration, refresh, and detach cleanup to the
  native DOM owner.
- Added audio/video DOM properties, constants, source re-selection, reset and
  load commands, error objects, bounded time ranges, event handlers, and the
  explicit decoder-unavailable `play()` rejection to the persistent realm.
- Connected local and content-process Blob media discovery to the existing
  post-mutation event/evaluation loop.
- Added local and HTTP(S) content-process regressions for successful WAV/WebM
  Blob admission, source selection, unsupported payload errors, and enforced
  `media-src 'none'` blocking.

## Tradeoffs and remaining scope

The slice copies only bounded metadata and retains no unbounded decoded frame
or sample buffers. MIME admission is useful for DOM/resource ownership but is
not equivalent to codec support; `play()` therefore rejects explicitly until
a real decoder/output owner is implemented. Static HTTP media is not silently
treated as Blob media, and unsupported or revoked resources surface the
existing error path instead of bypassing policy or cache ownership.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked` (passed)
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_media` (3 passed)
- `cargo test -p glass-browser --lib --locked media_metadata_accepts_supported_types_and_bounds_payloads` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_media_subresources` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_blocks_csp_disallowed_blob_media_subresources` (1 passed)
- `git diff --check`
