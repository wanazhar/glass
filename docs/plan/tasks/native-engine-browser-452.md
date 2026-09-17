# Native engine browser-complete slice 452: general XHR method tokens

- Status: complete locally
- Scope: `native-engine` / XMLHttpRequest method ownership
- Issue: #40
- Depends on: [native-engine-browser-451](native-engine-browser-451.md)

## Objective

Remove the remaining fixed seven-method ceiling from native XMLHttpRequest
`open()` while preserving XHR-specific error and synchronous/async behavior.
Valid HTTP extension methods such as `REPORT` must reach both asynchronous and
synchronous native request paths.

## Contract

- XHR method input is string-converted, ASCII-uppercased, bounded to the shared
  64-byte native method limit, and restricted to HTTP token characters.
- `CONNECT`, `TRACE`, and `TRACK` remain rejected as `SecurityError`; malformed
  token input remains `SyntaxError`, and over-limit valid tokens remain bounded
  by the native method owner.
- `GET` and `HEAD` remain bodyless through the existing Fetch loader checks.
- Async and sync XHR use the same native HTTP method owner and reqwest method
  conversion; response-state, timeout, credentials, MIME, and body policies are
  unchanged.
- Document navigation, form submission, history, and target-window navigation
  continue using the closed `NativeNavigationMethod` enum.

## Implementation

- Removed page and worker XHR allowlists and retained dedicated error-category
  checks for invalid and forbidden methods.
- Routed synchronous XHR method validation through `NativeFetchMethod` instead
  of the navigation-only method enum.
- Reused the existing async `fetchNative` bridge and native loader body/CORS/
  redirect/cookie policy for custom XHR methods.

## Tradeoffs

The shared method limit keeps child-process commands and request diagnostics
bounded while supporting ordinary extension verbs. XHR still does not inherit
document navigation semantics, and its existing native response/timeout policy
remains separate; broadening those contracts here would make an unrelated
behavioral change harder to audit.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --tests --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_sync_xhr_uses_fixture_loader -- --nocapture` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_xhr_response_type_is_canonical_and_state_aware -- --nocapture` (1 passed)
- `git diff --check`

