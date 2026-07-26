# Bot-Protection & Consent Walls: The Honest Lane

Glass takes an explicit **honest lane** through bot protection. It never spoofs
fingerprints, evades detection, or solves CAPTCHAs. Instead it uses signed
identity, verified-bot registration, authenticated profiles, and consent
dismissal — the legitimate paths site operators expect from automation.

## Why CDP Automation Gets Challenged

Chrome DevTools Protocol (CDP) exposes browser internals that normal user
sessions do not. Common detection signals include:

- `navigator.webdriver` is `true` (set by Chrome when `--remote-debugging-port` is active)
- `Runtime.enable` and `Runtime.runIfWaitingForDebugger` leave detectable traces
- Headless mode (`--headless`) exposes distinct rendering and font stacks
- CDP keeps the renderer in a slightly different scheduling mode

Detection is not a Glass bug — it is a deliberate Chrome behaviour that
distinguishes automation from organic browsing.

## Path 1: Allowlist Your Own Agent (Sites You Operate)

If you control the target site, the strongest answer is to register your agent:

### Verified Bot Registration

- **Cloudflare Web Bot Auth**: Register a key pair, publish your public key at
  `/.well-known/http-message-signatures-directory`, and use Glass's RFC 9421
  HTTP message signing (see Gate F.3) to present a `Signature-Agent` header.
  Cloudflare will verify the signature and admit your traffic.

- **IP Allowlisting**: If your agent runs from a known IP range (CI, staging),
  configure your WAF/CDN to bypass bot detection for those addresses.

- **Custom Header / Token**: Issue a static bearer token to your agent and
  validate it in your application middleware behind the CDN. Combine with rate
  limiting per token.

### Why This Beats Stealth

- No fingerprint-spoofing arms race
- No risk of violating site ToS
- Verifiable, auditable, and revocable
- Survives browser updates that change detection surfaces

## Path 2: Authenticated `--profile` Sessions

Many sites challenge *unauthenticated* automation more aggressively than
logged-in sessions.

### How to Use Profiles

```sh
# Create a named persistent profile
glass profiles create myagent

# Use it for every session
glass --profile myagent "navigate to https://example.com"
```

The profile persists cookies, local storage, and service-worker registrations
across restarts. Log in once manually (or via a one-time automated flow), and
subsequent sessions will carry the authenticated session state.

### What Profiles Enable

- OAuth / SSO session cookies survive across agent runs
- Site preferences and consent decisions are remembered
- Rate limits are scoped to the authenticated identity, not a fresh fingerprint

### Limitations

- Some sites expire sessions aggressively; you may need a periodic refresh flow
- Multi-factor authentication cannot be fully automated (by design)
- Profiles are Chrome user-data directories — treat them as secrets

## Path 3: Use the Site's API

When a site offers a documented API, the agent should use it instead of screen
automation:

- **Higher reliability**: APIs are versioned and contract-bound; DOM scraping
  breaks on redesigns.
- **Lower cost**: API calls consume fewer tokens than full page observations.
- **No bot-detection risk**: API traffic is expected and rate-limited, not
  challenged.

Glass is a browser control plane, not a universal web scraper. When the target
offers a first-class API path, prefer it.

## Path 4: Consent & Overlay Dismissal

Many sites present consent walls (cookie banners, age gates, newsletter popups)
that must be dismissed before page interaction.

### Glass Consent Helpers (Gate F.4)

Glass provides bounded helpers for common consent frameworks:

| Framework | Detection | Action |
|-----------|-----------|--------|
| OneTrust | CSS class `.onetrust-*` | Click "Accept All" or "Reject All" button |
| Cookiebot | `#CybotCookiebotDialog` | Click configured consent button |

Outcomes are typed: `dismissed`, `no_consent_found`,
`unrecognized_framework`. Glass does not click generic text matches.

**Important:** Consent helpers are UX assistance, not evasion. They interact
with visible consent UI exactly as a user would. They are part of Glass's
overlay corpus and subject to the same zero-wrong-action gate.

## Path 5: Polite-Agent Mode (Gate F.5)

Opt-in mode for public-facing sites:

```sh
glass --policy polite "navigate to https://example.com"
```

When enabled:

- `robots.txt` is fetched and respected (disallowed paths are denied)
- `Crawl-Delay` directives are honoured between requests
- A declared Glass `User-Agent` string is sent
- Policy can be set to `warn` (log violations) or `fail-closed` (deny)

This does not bypass bot protection, but it declares intent honestly and
respects site owner preferences.

## What Glass Does NOT Do

Glass will **never** include:

- Stealth / fingerprint spoofing
- CAPTCHA solving services
- Anti-bot evasions
- `navigator.webdriver` removal patches
- User-agent rotation or IP proxy chaining

These are explicit non-goals. If your use case requires them, Glass is the
wrong tool. Consider a dedicated web scraping framework instead.

## Decision Matrix

| Scenario | Recommended Path |
|----------|-----------------|
| CI testing on your own site | Path 1: IP allowlisting or verified bot |
| Authenticated agent on a SaaS you pay for | Path 2: `--profile` with login session |
| Public data from a site with an API | Path 3: Use the API |
| One-off navigation past a consent wall | Path 4: Consent dismissal helper |
| Polite crawl of public pages | Path 5: `--policy polite` |
| Bypassing bot protection on a third-party site | **Not supported** — use the site's API or contact the operator |

## Further Reading

- [Detection-surface transparency report](detection-surface.md)
- [Security policy](../SECURITY.md)
- [MCP integration](mcp.md) — policy presets for hardened agent deployments
