# Native engine browser-complete slice 472: data URL media resources

- Status: complete locally
- Scope: `native-engine` / bounded `data:` audio/video resource ownership
- Issue: #40
- Depends on: [native-engine-browser-471](native-engine-browser-471.md)

## Objective

Complete the embedded-media URL family for the current native media owner.
Static and dynamically attached `data:` media must use the same bounded
metadata, policy, and event path as Blob and HTTP(S) media instead of silently
falling into the unsupported-source state.

## Contract

- `data:` media sources are resolved as embedded bytes, never as network
  requests or cache entries.
- Base64 and percent-encoded payloads are decoded with the existing 16 MiB
  media bound; malformed escapes and invalid base64 fail through the native
  error path.
- An explicit supported media type is honored; an omitted type may use the
  bounded media sniffer. Unsupported explicit types are not promoted by
  sniffing.
- Local fixture/data documents admit static data media during navigation and
  dynamic data sources after DOM mutation. HTTP(S) content processes admit
  data media through the same media CSP/report-only policy boundary before
  decoding.
- The selected source, readiness/network state, MIME, duration, and existing
  playback timeline remain shared with Blob and HTTP(S) media. No CDP,
  network, or cache fallback is introduced.

## Implementation

- Added a media-specific URL resolver for network, Blob, and data schemes.
- Added bounded binary data-URL base64 and percent decoding and reused the
  media metadata/sniffing owner.
- Added static local data-media admission during navigation and dynamic local
  data-media admission after script mutation.
- Added loader unit coverage plus a static navigation integration fixture.

## Tradeoffs and remaining scope

This slice retains metadata rather than embedded bytes in the document wire,
so it does not add file URLs, decoder/frame output, independent media task
source scheduling, byte-range transport, or complete media/Web IDL parity.
HTTP(S) data media remains policy-checked; local fixture/data documents retain
their opaque local policy boundary. These limits remain explicit issue #40
work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked` (passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked media` (9 passed)
- `cargo test --quiet -p glass-browser --lib --locked media_` (4 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-472.json` (current-claim failures=0)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
