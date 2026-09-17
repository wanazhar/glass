# Native engine browser-complete slice 463: Blob popup navigation

- Status: complete locally
- Scope: `native-engine` / auxiliary browsing-context Blob navigation
- Issue: #40
- Depends on: [native-engine-browser-462](native-engine-browser-462.md)

## Objective

Preserve a page-owned Blob object-URL snapshot when `window.open()` or a
`WindowProxy` navigation targets that URL. The browser-effect boundary must
materialize the new or existing target from the bounded payload and commit a
fresh Blob document without attempting a network or cache lookup.

## Contract

- `window.open(blobURL, target)` carries the runtime-owned Blob bytes into the
  popup request and creates a target with the Blob document loaded.
- `popup.location.assign(blobURL)` and `replace(blobURL)` carry the same
  bounded payload through `WindowProxy` navigation requests.
- Local and content-process browser-effect queues preserve the optional
  object-URL payload; absent legacy payloads remain compatible.
- New Blob targets bootstrap from `about:blank` before the normal native
  navigation owner commits the Blob document, avoiding an unsupported initial
  network-loader route.
- Popup/window object-URL payloads are bounded again when decoded from the
  content-process response.
- HTTP(S), fixture, named-target reuse, opener metadata, and ordinary popup
  navigation remain unchanged.

## Implementation

- Added optional bounded Blob snapshots to popup and `WindowProxy` navigation
  request envelopes.
- Captured the snapshot at the originating JavaScript runtime and preserved
  it through local queues and content-process serialization.
- Updated backend request conversion and new-target bootstrap to pass the
  payload into the existing native document navigation owner.
- Added content-process payload-size validation for both browser-effect
  request types.

## Tradeoffs and remaining scope

The object URL is transferred as a bounded byte snapshot, not as a live shared
registry across targets. The destination receives a fresh document realm, so
later source-realm revocation does not mutate an already committed document.
Media consumers and cross-realm object-URL registry sharing remain separate
issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_window_open_loads_blob_document_target -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked window_open -- --nocapture`
- `git diff --check`
