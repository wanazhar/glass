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

## Reporting a vulnerability

Do not open a public issue containing exploit details, credentials, profile
data, or other sensitive material. Report vulnerabilities through the
[private GitHub security-advisory form](https://github.com/wanazhar/glass/security/advisories/new).
Include the affected version, platform, reproduction steps, impact, and any
suggested mitigation.
