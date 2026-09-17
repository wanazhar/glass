# Native engine browser-complete slice 455: Blob document navigation

- Status: complete locally
- Scope: `native-engine` / page-created Blob URL document navigation
- Issue: #40
- Depends on: [native-engine-browser-454](native-engine-browser-454.md)

## Objective

Make a same-realm page navigation to a page-created `blob:` URL load a real
fresh native document in both the inline engine and the process-backed content
owner. The navigation must preserve the Blob creator origin without treating
the object URL as an ordinary network request.

## Contract

- A page-created Blob URL can be used by `location.assign()`, link, form, and
  page-script navigation paths that remain in the current browsing context.
- The live realm registry is consulted before the old document is replaced.
  A bounded byte snapshot, not a live JavaScript Blob object, crosses the
  navigation boundary.
- Only a bodyless `GET` is admitted for a Blob document navigation. The body is
  bounded by the native document/script limits and must be valid UTF-8 HTML.
- `blob:null/...` produces the opaque native origin. A Blob URL created by an
  HTTP(S) page produces the creator's tuple origin on the new document.
- The destination receives a fresh page realm and normal document parsing,
  title, visible-text, URL, and origin projections.
- A missing or revoked registry entry remains unavailable; no synthetic
  network fallback is introduced.

## Implementation

- Added a bounded `NativeObjectUrlResource` payload with explicit base64 byte
  serialization for IPC and origin recovery for `blob:` URLs.
- Snapshot page-registry entries while turning page-script, lifecycle, link,
  and form navigation results into native navigation requests.
- Thread the payload through parent navigation state and content-process load
  IPC, with boundary validation for byte and metadata limits.
- Load object-URL documents directly from the validated snapshot, bypassing
  network and Service Worker interception, then commit the fresh document
  state. HTTP(S) navigation retains its existing loader path.
- Permit only validated `blob:` final URLs in content-process load responses;
  all other non-HTTP(S) final URLs remain rejected.

## Tradeoffs and remaining scope

The bounded snapshot is intentionally a copy: it gives the destination a
stable document input and avoids transferring a live JavaScript object across
realms or processes, at the cost of not modeling a shared Blob registry.
This slice covers top-level same-context document navigation only. Generic
Blob URL image, stylesheet, script, media, and other subresource consumers,
popup/window transfer, and broader cross-realm object-URL sharing remain
separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked blob_object_urls -- --nocapture`
- `git diff --check`
