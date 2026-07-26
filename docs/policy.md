# Policy Reference

Glass enforces typed safety policies that gate high-risk browser capabilities.
Every session runs under a named preset. Capabilities are either allowed,
denied, or require a one-shot confirmation token.

## Named Presets

### `dev` (alias: `development`)

The default for local development. All capabilities are allowed without
confirmation.

| Capability | Decision |
|-----------|----------|
| `attach` | Allow |
| `persistent_profile` | Allow |
| `evaluate` | Allow |
| `upload` | Allow |
| `download` | Allow |
| `screenshot` | Allow |
| `raw_cdp` | Allow |
| `read_form_values` | Allow |
| `read_sensitive_form_values` | Allow |

```sh
glass --policy dev
```

### `ci`

Designed for CI pipelines. Allows all capabilities but rejects `raw_cdp` and
`persistent_profile` by default. Host allowlisting is optional.

| Capability | Decision |
|-----------|----------|
| `attach` | Allow |
| `persistent_profile` | Deny (disposable sessions only) |
| `evaluate` | Allow |
| `upload` | Allow |
| `download` | Allow |
| `screenshot` | Allow |
| `raw_cdp` | Deny |
| `read_form_values` | Allow |
| `read_sensitive_form_values` | Allow |

CI sessions are typically short-lived and disposable. Profiles are denied to
prevent state leakage between runs. Raw CDP is denied because it bypasses all
policy checks.

```sh
glass --policy ci
```

### `polite`

The opt-in public-site preset follows `robots.txt`, honors `Crawl-delay` with
a bounded minimum delay, and identifies requests as `Glass/<version>`. A
robots failure or disallowed path fails closed. It does not evade bot
protection or rotate identities.

```sh
glass --policy polite navigate https://example.com/public-page
```

### `hardened`

The production preset for untrusted content. Every capability is denied by
default; only explicitly allowed capabilities are permitted. Host allowlisting
is mandatory (`--policy-allow-host`).

| Capability | Default | With allow |
|-----------|---------|------------|
| `attach` | Deny | RequireConfirmation |
| `persistent_profile` | Deny | RequireConfirmation |
| `evaluate` | Deny | RequireConfirmation |
| `upload` | Deny | RequireConfirmation |
| `download` | Deny | RequireConfirmation |
| `screenshot` | Deny | Allow |
| `raw_cdp` | Deny (always) | Deny (always) |
| `read_form_values` | Deny | Allow |
| `read_sensitive_form_values` | Deny (always) | Deny (always) |

```sh
glass --policy hardened --policy-allow-host example.com
```

Hardened mode also validates host configurations:
- Attach mode requires public IP-literal allow rules to prevent DNS rebinding
- Host resolution must produce only public addresses

### `untrusted-mcp`

The strictest preset for MCP servers that you did not build yourself. All
capabilities require confirmation tokens. Every high-risk action must be
explicitly confirmed per invocation. Host allowlisting is mandatory.

| Capability | Decision |
|-----------|----------|
| `attach` | RequireConfirmation |
| `persistent_profile` | Deny (always) |
| `evaluate` | RequireConfirmation |
| `upload` | RequireConfirmation |
| `download` | RequireConfirmation |
| `screenshot` | RequireConfirmation |
| `raw_cdp` | Deny (always) |
| `read_form_values` | RequireConfirmation |
| `read_sensitive_form_values` | Deny (always) |
| `consent_dismissal` | Allow only for recognized visible UX controls |
| `declared_agent_identity` | Deny unless explicitly allowed |

```sh
glass --mcp --policy untrusted-mcp --policy-allow-host example.com
```

## Confirmation Tokens

Capabilities in `RequireConfirmation` state need a one-shot confirmation before
use. Clients send a confirmation token via the MCP `confirm` tool or the
`--policy-confirm` CLI flag.

Tokens are scoped to a single capability and consumed on use. After
confirmation, the capability acts as `Allow` for the remainder of the session.

```sh
# CLI: confirm a capability
glass --policy-confirm evaluate navigate https://example.com

# MCP: send a confirmation token
# The client calls the "confirm" tool with {"capability": "evaluate"}
```

## Capability Reference

| Capability | What it gates |
|-----------|--------------|
| `attach` | Connecting to an existing Chrome instance (`--attach`) |
| `persistent_profile` | Using a named, persistent profile (`--profile`) |
| `evaluate` | `Runtime.evaluate` and `callFunctionOn` in page context |
| `upload` | File upload via `<input type="file">` |
| `download` | File download with `Page.setDownloadBehavior` |
| `screenshot` | Screen capture (`Page.captureScreenshot`) |
| `raw_cdp` | Direct CDP method dispatch (unbounded escape hatch) |
| `read_form_values` | Reading non-password form field values |
| `read_sensitive_form_values` | Reading password/credit-card field values |
| `consent_dismissal` | Clicking documented OneTrust/Cookiebot consent controls |
| `declared_agent_identity` | Signing an explicitly supplied request with an opt-in Ed25519 key |

## Host Allowlisting

The `--policy-allow-host` flag restricts navigation to specific hosts. In
`hardened` and `untrusted-mcp` presets, at least one host must be provided.

```sh
# Single host
glass --policy hardened --policy-allow-host example.com

# Multiple hosts
glass --policy hardened --policy-allow-host example.com --policy-allow-host api.example.org
```

Hosts must resolve to public IP addresses. Private/link-local addresses are
rejected. In attach mode, only IP literals are accepted to prevent DNS
rebinding attacks.

## Quick Decision Matrix

| Preset | Evaluate | Screenshot | Upload | Download | Raw CDP | Profiles |
|--------|----------|------------|--------|----------|---------|----------|
| `dev` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ci` | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| `hardened` | 🔐 | ✅ | 🔐 | 🔐 | ❌ | 🔐 |
| `untrusted-mcp` | 🔐 | 🔐 | 🔐 | 🔐 | ❌ | ❌ |

- ✅ = Allow
- 🔐 = RequireConfirmation (or Deny without explicit allow)
- ❌ = Deny (always)
