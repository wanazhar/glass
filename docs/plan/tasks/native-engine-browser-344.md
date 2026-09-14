# Native Service Worker client enumeration (344)

Status: implemented locally in the current 0.3.14 checkout.

This slice replaces the empty native `clients.matchAll()` placeholder with a
bounded current-client view for an active Service Worker fetch:

- the native owner supplies a stable opaque client id derived from the
  top-level document URL, the document URL, window/top-level metadata,
  visibility, focus, and control state;
- `clients.matchAll()` accepts the standard `window`, `worker`,
  `sharedworker`, and `all` filters, honors `includeUncontrolled`, returns the
  current window client for the supported window view, and returns an empty
  result for worker-only views because this content process has no such
  client; and
- lifecycle evaluations clear fetch-only client state, so an activation or
  install event cannot observe a stale page client from an earlier request.

## Tradeoffs

- A content process currently owns one top-level document, so enumeration is a
  bounded one-client projection rather than a multi-tab/frame registry. The
  native owner still filters by client type and control state instead of
  returning a misleading universal list.
- Client identity is stable for repeated fetches from the same document URL,
  but history/navigation context identity is not yet a separate browser-wide
  ledger. Full frame/tab lifecycle, `Client.postMessage()`, `openWindow()`,
  and exact task-source ordering remain issue #40 promotion work.
- `includeUncontrolled` is honored against the owner-provided control bit;
  service-worker interception currently supplies a controlled top-level page
  because the native registry owns the matching scope. No Chromium/CDP query
  is used.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked` — 1 passed
- `git diff --check`

The HTTP(S) integration witness registers and activates a worker, then checks
the default window view, the `all`/`includeUncontrolled` view, the empty
worker-only view, and the client metadata while the worker handles a fetch.
All evidence is local; this checkout has not been pushed and has no remote CI
result.
