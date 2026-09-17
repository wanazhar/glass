# Native engine browser-complete slice 465: page-window Blob message transfer

- Status: complete locally
- Scope: `native-engine` / page-window structured-message Blob URL ownership
- Issue: #40
- Depends on: [native-engine-browser-464](native-engine-browser-464.md)

## Objective

Make a Blob URL referenced by a `WindowProxy` message usable in the
destination page realm. Popup and page-window delivery must use the same
bounded browser-owned resource envelope as page/worker delivery across both
inline and content-process owners.

## Contract

- A parent-owned Blob URL embedded at any supported bounded depth in a message
  to a popup or child window is snapshotted before the message leaves the
  parent realm and is available to child Fetch/XHR after delivery.
- A child-owned Blob URL embedded in a message back to its opener is
  snapshotted before the message leaves the child realm and is available to
  parent Fetch/XHR after delivery.
- Active targets, parked named targets, native frames, and HTTP(S)
  content-process routes carry the same typed transfer envelope; messages
  without transfers remain compatible.
- The destination installs each bounded snapshot before structured-clone
  decoding. The original URL string is preserved, while source-realm
  revocation remains independent from the destination snapshot.
- Transfer count, URL, MIME, byte, nesting, aggregate-message, and
  content-process framing limits remain enforced; malformed transfers fail
  closed without network or cache fallback.
- Ordinary non-Blob popup/window navigation does not touch the Blob registry
  during an active QuickJS turn, so fixture/HTTP navigation remains
  deadlock-free.

## Implementation

- Extended page-window post-message requests and page-message events with the
  bounded `NativeObjectUrlTransfer` list already used by worker messages.
- Snapshotted Blob URLs in the active page context, validated them at the
  engine and content-process boundaries, and propagated them through active,
  parked, frame, and content-process dispatch routes.
- Installed destination Blob snapshots before clone decoding in the page
  message bridge.
- Split Blob registry access into an outer context-opening helper and an
  in-context helper; popup/window navigation uses the latter only for `blob:`
  URLs and avoids reentrant context entry for ordinary URLs.

## Tradeoffs and remaining scope

Transfers intentionally copy bounded Blob bytes. This keeps realm ownership,
revocation, and process boundaries explicit, but rejects oversized or revoked
entries rather than creating unbounded shared state. The page-window contract
does not yet close MessagePort Blob transfer, Service Worker client-message
Blob transfer, media consumers, or the remaining browser/Web IDL conformance
gates; those remain separate issue #40 slices.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_post_message_ -- --nocapture` (3 passed)
- `cargo test -p glass-browser --test native_engine --locked native_http_post_message_transfers_blob_urls_between_content_processes -- --nocapture` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked window_open_ -- --nocapture` (4 passed)
- `git diff --check`
