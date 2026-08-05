# Policy reference

Glass applies one named policy to each session. A policy marks a capability as
`allow`, `deny`, or `require_confirmation` in serialized policy decisions.

## Presets

| Preset | Use |
|---|---|
| `dev` or `development` | Local development. Capabilities are allowed. |
| `ci` | CI work. Persistent profiles and raw CDP are denied. |
| `polite` | Public read access with robots and delay checks. |
| `hardened` | Untrusted content. Capabilities and hosts are denied by default. |
| `untrusted-mcp` | An MCP client that you did not build. High-risk operations need confirmation. |

Start a session with a preset:

```console
glass --policy dev
glass --policy ci
glass --policy polite navigate https://example.com/public-page
glass --mcp --policy untrusted-mcp --policy-allow-host example.com
```

The `ci` preset intentionally permits public HTTP(S) navigation without
host-pinning. It still denies persistent profiles and raw CDP. Use `hardened`
or `untrusted-mcp` with exact `--policy-allow-host` rules when DNS pinning and
host isolation are required.

## Hardened policy

Hardened mode requires at least one exact `--policy-allow-host` value:

```console
glass --policy hardened --policy-allow-host example.com
```

Glass rejects wildcard hosts, overlapping allow and deny rules, private
addresses, reserved addresses, and a missing host rule.

Glass resolves each allowed host once. It accepts only public addresses. In an
owned session, it pins the accepted address in Chrome. Hardened attach mode
requires an explicit `attach` grant and IP-literal host rules.

## Capability decisions

These capabilities are policy controlled:

| Capability | Operation |
|---|---|
| `attach` | Connect to an existing Chrome process. |
| `persistent_profile` | Use a named persistent profile. |
| `evaluate` | Run JavaScript in the page. |
| `upload` | Upload local files. |
| `download` | Save a browser download. |
| `screenshot` | Capture page pixels. |
| `raw_cdp` | Send direct CDP methods. |
| `read_form_values` | Read non-password form values. |
| `read_sensitive_form_values` | Read password or payment form values. |
| `read_sensitive_extraction` | Read secret-like structured extraction fields. |
| `consent_dismissal` | Click recognized consent controls. |
| `declared_agent_identity` | Sign an explicitly supplied request. |

`read_sensitive_form_values` does not grant `read_sensitive_extraction`.
Structured extraction checks the dedicated capability independently and fails
closed when secret-like names or paths are requested without it.

Raw CDP is an unrestricted escape hatch. Use an explicit allow decision. A
confirmation token cannot grant raw CDP.

## Confirmation

A capability with `require_confirmation` returns a typed
`confirmation_required` policy result until a one-use token is available.
Supply a token from fixed CLI configuration:

```console
glass --policy-confirm-once evaluate navigate https://example.com
```

For an MCP server, configure `--policy-confirm-once` when starting the server;
there is no request-controlled `confirm` tool. Glass scopes the token to that
capability and consumes it when the operation runs. Keep policy options in
fixed deployment configuration. Do not copy them from untrusted page data or
request data.

## Host rules

In `hardened` and `untrusted-mcp` modes, provide at least one host rule:

```console
glass --policy hardened \
  --policy-allow-host example.com \
  --policy-allow-host api.example.org
```

Host rules must use exact DNS names or public IPv4 literals. Glass rejects
private, link-local, shared, and reserved networks. Attach mode accepts only
IP literals.

## Decision matrix

| Preset | Evaluate | Screenshot | Upload | Download | Raw CDP | Profile |
|---|---|---|---|---|---|---|
| `dev` | Allow | Allow | Allow | Allow | Allow | Allow |
| `ci` | Allow | Allow | Allow | Allow | Deny | Deny |
| `hardened` | Confirm | Allow | Confirm | Confirm | Deny | Confirm |
| `untrusted-mcp` | Confirm | Confirm | Confirm | Confirm | Deny | Deny |

A policy error is serialized as the versioned `PolicyErrorContract`:
`schemaVersion`, `policyVersion`, `kind`, optional `operation`, `ruleId`,
`phase`, `reason`, optional `remediation`, and `overridePossible`.
`kind` is `denied`, `confirmation_required`, or `invalid_configuration`;
`operation` uses the stable snake-case capability names listed above.
Denied and confirmation-required errors occur during `preflight`; invalid
configuration errors occur during `configuration`. Glass rejects invalid
policy combinations before it starts Chrome.
