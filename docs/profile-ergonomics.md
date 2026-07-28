# Persistent browser profiles

A persistent profile stores browser state between Glass sessions. Use a profile
when you need an existing login without copying a password into a command.

## Create and use a profile

Run:

`console
glass profiles create work
glass --headed --profile work navigate https://app.example.com/login
`

Complete the login in the headed browser. Close the session normally.

Use the profile later:

`console
glass --profile work navigate https://app.example.com/dashboard
glass --profile work observe
`

## Export and import cookies

Use a bounded JSON file for an explicit cookie transfer:

`console
glass --profile work export-cookies ./work-cookies.json
glass --profile work import-cookies ./work-cookies.json
`

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

`console
glass profiles
`

Delete a profile:

`console
glass delete-profile work
`

Deletion removes all stored state. The action cannot be undone.

Use `--incognito` when the session must not persist state.

## Attach mode

Attach to an existing Chrome process:

`console
glass --attach --port 9222 navigate https://example.com
`

Glass does not silently adopt a running browser. Attach mode does not create or
manage profiles. It uses the profile of the existing Chrome process.

The `persistent_profile` capability is denied by the `untrusted-mcp` policy.

Read [Policy reference](policy.md), [Installation and operations](installation.md),
and the [security policy](../SECURITY.md).
