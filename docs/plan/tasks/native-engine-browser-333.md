# Native semantic cookie writes (333)

Status: implemented locally in the native runtime.

The transport-neutral `StorageOperation::Write` contract now works for
`StorageScope::Cookies`. It assigns `key=value` through the current page's
`document.cookie` owner, synchronizes the result into the native loader/profile
and content worker as applicable, and returns the resulting name/value map.
Native local documents and process-backed HTTP(S) documents use the same
page-owned path. The CDP adapter uses the same current-origin assignment
semantics for the shared backend contract.

This intentionally remains the minimal semantic map operation: it cannot
represent domain, path, Secure, HttpOnly, SameSite, expiration, or priority.
Callers that need those attributes continue to use the full `setCookies`
import API, which validates and preserves cookie profiles. Non-HTTP(S)
documents reject semantic cookie writes rather than reporting a false success.

## Tradeoffs

- Current-origin assignment matches the Web API that a key/value map can
  faithfully express and keeps page script and backend storage synchronized.
- The richer cookie import API remains separate because flattening attributes
  into a map would silently weaken security and persistence semantics.
- Cookie policy, partitioning, eviction rules, and complete browser cookie
  parity remain outside the bounded native profile.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --locked semantic_storage_uses_page_owned_native_state -- --nocapture` — 1 passed
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

All evidence is local; this checkout has not been pushed and has no remote CI
result.
