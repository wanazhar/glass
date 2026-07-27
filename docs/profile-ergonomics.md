# Logged-in Session Ergonomics

Glass supports persistent browser profiles so automation programs can reuse
authenticated sessions without copying credentials into command input.

## Quick Start

Create a named profile, log in once, and reuse it across runs:

```sh
# Create and use a profile
glass --headed --profile work navigate https://app.example.com/login

# Log in manually, then close the headed browser window

# Subsequent sessions carry the authenticated state
glass --profile work "navigate to https://app.example.com/dashboard"
glass --profile work observe
```

For an explicit, reviewable cookie hand-off, use bounded JSON files while the
profile is active:

```sh
glass --profile work export-cookies ./work-cookies.json
glass --profile work import-cookies ./work-cookies.json
```

Cookie import/export is policy-gated, limited to 256 cookies and a 512 KiB
import file, and should be treated as secret material. Keep these files out of
source control; do not pass them through untrusted command input.

## What Profiles Persist

- Cookies (session, persistent, HttpOnly)
- Local storage and session storage
- Service worker registrations and caches
- IndexedDB, Cache API data
- Extension state (if extensions are loaded)
- Permission grants (notifications, clipboard, etc.)

## Profile Lifecycle

```sh
# List profiles
glass profiles

# Delete a profile (all stored state is gone)
glass delete-profile work
```

Profiles are stored as Chrome user-data directories. Each profile is isolated
from others — cookies, storage, and extensions are not shared.

## Security Considerations

- **Profiles contain secrets.** Cookies for authenticated sessions grant
  access to whatever the logged-in user can do. Store the profile directory
  with the same care as an API key.
- **Incognito mode** (`--incognito`) is the opposite: every session is
  disposable and no state persists.
- **Attach mode** (`--attach`) connects to an existing Chrome instance.
  Glass never silently adopts a running browser. The port must be explicitly
  supplied.
- **Policy gating:** The `persistent_profile` capability is controlled by
  the policy preset. In `untrusted-mcp` mode, persistent profiles are denied.

## Use an Existing Login Without Pasting Passwords

1. **Create a profile:** `glass --headed --profile work navigate https://app.example.com`
2. **Log in once:** Use the headed browser (`--headed`) to complete the login
   flow manually. Close the session normally.
3. **Subsequent runs:** `glass --profile work navigate https://app.example.com/dashboard`
   — the session cookie is already present.
4. **Token refresh:** If the session expires, repeat step 2. OAuth refresh
   tokens stored in cookies will work automatically.

This pattern works for:
- SaaS applications with OAuth/SSO (Google, GitHub, Microsoft)
- Applications with long-lived session cookies
- Sites that remember "keep me logged in" preferences

## Attach Mode

Instead of launching Chrome, connect to an existing instance:

```sh
glass --attach --port 9222 navigate https://example.com
```

Attach mode uses whatever profile the target Chrome instance is currently
running. Glass does not create or manage profiles in attach mode.

**Attach mode remains explicit** — Glass never silently discovers or adopts
a running Chrome instance. The port must be supplied via `--attach` or
`--port`.

## Further Reading

- [Policy reference](policy.md) — `persistent_profile` capability gating
- [Installation & operations](installation.md) — profile directory locations
- [Security policy](../SECURITY.md) — credential handling
