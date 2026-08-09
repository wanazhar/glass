# Persistent browser profiles

A persistent profile stores browser state between Glass sessions. Use a profile
when you need an existing login without copying a password into a command.

## Create and use a profile

Run:

```console
glass profiles create work
glass --headed --profile work tui
```

Enter `navigate https://app.example.com/login`, complete the login in the
headed browser, and close the owned session normally.

Use the profile later:

```console
glass --profile work tui
```

```text
navigate https://app.example.com/dashboard
observe
```

Navigation and observation share the resident TUI session. Separate one-shot
commands may reuse persisted cookies, but they do not share the current target
or revision.

## Export and import cookies

Use a bounded JSON file for an explicit cookie transfer:

```console
glass --profile work export-cookies ./work-cookies.json
glass --profile work import-cookies ./work-cookies.json
```

Cookie export and import require the persistent-profile capability. Imports are
limited to 256 cookies and 512 KiB.

Cookie files are secret material. Keep them out of source control. Do not pass
them through untrusted input.

## Stored browser state

A profile may contain:

- cookies;
- local storage and session storage;
- service-worker registrations and caches;
- IndexedDB and Cache API data;
- extension state; and
- permission grants.

Keep the profile directory as carefully as an API key.

## Profile lifecycle

List profiles:

```console
glass profiles
```

Delete a profile:

```console
glass delete-profile work
```

Deletion removes all stored state. The action cannot be undone.

Glass holds an exclusive profile lock while an owned workspace uses a named
profile. A second owner and profile deletion fail while that lock is active.
Different workspaces may exist, but they cannot concurrently mutate the same
profile-backed browser authority.

Close an owned session normally. Glass sends `Browser.close` before process
fallback so Chrome can flush cookies and storage. A hard process kill can lose
the newest profile writes. Attach mode never owns the external Chrome process
and therefore never sends this close operation.

Profiles live under the platform configuration `glass/profiles` directory, or
under `$GLASS_CONFIG_HOME/glass/profiles` when the override is set. Current
profiles use Chrome user-data directories. Glass migrates the supported legacy
`NAME_chrome` layout on use; do not rename profile directories manually.

Use `--incognito` when the session must not persist state.

## Attach mode

Attach to an existing Chrome process:

```console
glass --attach --port 9222 navigate https://example.com
```

Glass does not silently adopt a running browser. Attach mode does not create or
manage profiles. It uses the profile of the existing Chrome process.

The `persistent_profile` capability is denied by the `untrusted-mcp` policy.

## Failure and recovery

| Failure | Recovery |
|---|---|
| profile is active | close the owning workspace/browser, then delete or reopen |
| profile name rejected | use 1–64 ASCII letters, digits, `_`, or `-` |
| profile did not retain newest state | confirm the prior owned session closed cleanly; do not copy a live Chrome directory |
| attached browser has unexpected state | inspect the Chrome process/profile that was launched externally |
| cookie import rejected | verify the 512 KiB/256-cookie bounds and canonical JSON fields |

Deleting the Cargo packages does not delete profiles. Use the explicit
[complete uninstall procedure](installation.md#fully-uninstall-glass) when the
machine must retain no Glass-owned browser data.

Read [Policy reference](policy.md), [Installation and operations](installation.md),
and the [security policy](../SECURITY.md).
