# Native engine browser-complete slice 469: static HTTP media resources

- Status: complete locally
- Scope: `native-engine` / static HTTP(S) `<audio>` and `<video>` resources
- Issue: #40
- Depends on: [native-engine-browser-468](native-engine-browser-468.md)

## Objective

Extend the media resource owner from realm-owned Blob URLs to ordinary
HTTP(S) media sources. Initial document discovery and later script mutations
must use the same bounded resource/event state without borrowing Fetch’s
`connect-src` semantics or silently skipping static media.

## Contract

- HTTP(S) media requests use a bounded GET, native environment headers,
  referrer, same-origin cookie policy, and the existing request-delay,
  offline, timeout, and response-throughput controls.
- The owner applies mixed-content checks, `media-src`/`default-src` CSP, and
  the same policy to every bounded redirect; report-only observations use the
  existing violation queue.
- Responses must be successful, carry a valid ASCII content type when one is
  present, and remain within `MAX_NATIVE_MEDIA_BYTES` both by declared length
  and while streaming. Supported MIME types or bounded payload sniffing then
  produce the same metadata used by Blob media, including WAV duration.
- Initial parser-discovered media and later script-discovered media use one
  content-process helper. Blob sources still resolve through the runtime
  registry; non-Blob sources use HTTP media transport only for network owners.
- Decoder-backed playback, audio/video output, range requests, media cache
  revalidation, richer timelines, static data/file media, and complete
  media/Web IDL parity remain explicit issue #40 work.

## Implementation

- Added the bounded HTTP media loader with redirect, mixed-content, media CSP,
  cookie, content-type, streaming-size, and environment controls.
- Updated initial and mutation-time content-process media discovery to choose
  the runtime Blob owner for Blob URLs and the HTTP media owner for network
  URLs, preserving the existing resource wire and load/error event path.
- Added an HTTP(S) integration regression serving a static WebM payload and
  verifying initial media readiness, current source, MIME support, and load
  event delivery.

## Tradeoffs and remaining scope

The current resource state retains metadata rather than decoded samples, so a
successful HTTP response proves bounded resource admission but not playback.
There is intentionally no media cache or byte-range transport yet; those need
explicit lifetime, seek, and privacy contracts before they can share the
metadata owner. Unsupported, oversized, denied, redirected, or malformed
responses fail through the existing media error state instead of bypassing
policy.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked` (passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_static_http_media_subresource` (1 passed)
- `git diff --check`
