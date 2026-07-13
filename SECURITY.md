# Security policy

## Supported versions

Until the first public release, only the latest commit is supported. After
release, security fixes will target the latest published `0.x` version.

## Trust model

Chrome DevTools Protocol access is equivalent to interactive control of the
selected browser profile. Glass can read page content, execute JavaScript,
navigate, capture screenshots, and interact with authenticated sessions.

- Never expose a Chrome debugging port to an untrusted network.
- Use a dedicated browser profile instead of a personal daily-use profile.
- Prefer `--incognito` when persistence is unnecessary.
- Treat profile data, screenshots, DOM output, and debug logs as sensitive.
- Only connect Glass to MCP clients and automation inputs you trust.
- Keep both Glass and Chrome/Chromium updated.

Glass protects against accidentally adopting an occupied CDP endpoint by
requiring `--attach`. This is a lifecycle safeguard, not an authentication or
network security boundary.

## Hardened agent policy

Use `--policy hardened --incognito` for untrusted automation. Hardened startup
requires at least one exact `--policy-allow-host`; Glass resolves each allowed
name once, rejects any non-public answer, and pins the accepted address into
the owned Chrome resolver. Session-lifetime Fetch interception checks every
document navigation, redirect, popup, and attached frame before it continues.
Localhost, credentials in URLs, non-HTTP(S) schemes, private/link-local/shared/
reserved networks, and hosts outside the exact allow list fail closed.

Hardened mode denies attach, persistent profiles, JavaScript evaluation,
uploads, downloads, screenshots, and raw CDP access by default. A capability
can be deliberately enabled with `--policy-allow CAPABILITY`, or configured to
return a structured confirmation requirement with `--policy-confirm
CAPABILITY`. `--policy-confirm-once CAPABILITY` supplies one consumable token;
it is invalid unless the same capability is confirmation-required. These flags
are authority grants and should be fixed by the deployment, never copied from
untrusted model output.

Filesystem reads and writes in hardened mode are canonicalized and confined to
the process working directory. Existing symlinks resolving outside that root
are rejected. Run Glass in a dedicated, minimally writable workspace because
path checks do not replace operating-system permissions or sandboxing.

## Reporting a vulnerability

Do not open a public issue containing exploit details, credentials, profile
data, or other sensitive material. Report vulnerabilities through the
[private GitHub security-advisory form](https://github.com/wanazhar/glass/security/advisories/new).
Include the affected version, platform, reproduction steps, impact, and any
suggested mitigation.
