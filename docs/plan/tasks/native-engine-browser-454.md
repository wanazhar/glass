# Native engine browser-complete slice 454: Blob object URLs

- Status: complete locally
- Scope: `native-engine` / Blob object URLs and script request consumers
- Issue: #40
- Depends on: [native-engine-browser-453](native-engine-browser-453.md)

## Objective

Give page and worker scripts a bounded object-URL lifecycle for Blob and File
values. A generated `blob:` URL must be readable through the native Fetch and
XHR owners without becoming an unbounded string or byte transfer.

## Contract

- `URL.createObjectURL(blob)` accepts native Blob/File values, returns a
  deterministic origin-labelled `blob:` URL, and retains the bounded bytes in
  the owning realm's registry.
- Each registry is bounded by the realm command limit; creating another entry
  after the bound is reached raises `RangeError`. IDs remain unique across
  bootstrap refreshes.
- Page and worker Fetch and asynchronous/synchronous XHR serve only GET and
  HEAD object-URL reads. Responses preserve status, `OK` status text, content
  type, byte length, URL, and body type; HEAD exposes no body.
- `URL.revokeObjectURL(value)` removes the matching entry. A later request no
  longer finds the local object URL and fails through the normal native loader
  error path. Object URLs are realm-local in this slice.

## Implementation

- Added persistent, bounded page and worker object-URL registries with
  monotonic IDs, origin labels, Blob type/length headers, and revocation.
- Added local object-URL response construction to page and worker Fetch before
  transport dispatch, preserving existing method/body validation.
- Added page and worker synchronous XHR object-URL reads; asynchronous XHR
  reuses the corresponding Fetch owner automatically.
- Added bounded QuickJS microtask draining for local top-level promise chains
  and local rejection for revoked page `blob:` URLs instead of falling through
  to unsupported network transport.
- Added page and worker integration witnesses covering Fetch, HEAD, async XHR,
  sync XHR, revocation, and non-Blob rejection.

## Tradeoffs

Realm-local registries avoid a new cross-process byte transport and keep Blob
ownership explicit, at the cost of not yet sharing an object URL with another
realm or using it for navigation/image/stylesheet/script subresources. The
response is synthesized from already-owned bounded bytes, so it does not
pretend to be a network response or bypass CSP/CORS for external requests.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked blob_object_urls -- --nocapture`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-454.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
