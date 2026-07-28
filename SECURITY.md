# Security policy

## Supported versions

Before the first public release, only the latest local commit is supported.
After a release, security fixes target the latest published 0.x version.

## Trust model

Chrome DevTools Protocol (CDP) access gives Glass control of the selected browser
profile. Glass can read page data, run JavaScript, navigate, capture
screenshots, and use authenticated sessions.

Protect the following resources:

- Keep the Chrome debugging port on the local host.
- Do not expose the debugging port to an untrusted network.
- Use a dedicated browser profile.
- Use `--incognito` when you do not need persistent state.
- Treat profiles, screenshots, DOM output, and logs as sensitive.
- Connect Glass only to trusted MCP clients and automation inputs.
- Keep Glass and Chrome or Chromium current.

Glass requires `--attach` before it uses an occupied CDP endpoint. This prevents
an accidental takeover. It does not provide authentication or network
protection.

## Hardened policy

Use `--policy hardened --incognito` for untrusted automation.

Hardened startup requires one or more exact `--policy-allow-host` values. Glass
resolves each host once. It rejects non-public addresses. It pins the accepted
address in the owned Chrome resolver.

Glass checks every document navigation, redirect, popup, and attached frame.
The check occurs before the document continues.

Hardened mode denies these capabilities by default:

- attach;
- persistent profiles;
- JavaScript evaluation;
- uploads;
- downloads;
- screenshots; and
- raw CDP.

You may enable a capability with `--policy-allow CAPABILITY`. You may request a
confirmation with `--policy-confirm CAPABILITY`. A one-use token is available
with `--policy-confirm-once CAPABILITY`.

Treat these options as deployment configuration. Do not copy them from
untrusted input.

Raw CDP returns an unrestricted protocol handle. It cannot use a one-use token.
It requires an explicit `raw-cdp` allow grant.

Glass canonicalizes filesystem paths in hardened mode. It keeps paths inside
the process working directory. An existing symlink that resolves outside this
directory is rejected. Run Glass in a dedicated workspace. File checks do not
replace operating-system permissions or sandboxing.

## Report a vulnerability

Do not open a public issue with exploit details, credentials, profile data, or
other sensitive data.

Use the [private GitHub security advisory form](https://github.com/wanazhar/glass/security/advisories/new).
Include:

- the affected version;
- the platform;
- reproduction steps;
- the impact; and
- a suggested mitigation, if known.
