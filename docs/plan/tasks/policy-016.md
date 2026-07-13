---
id: policy-016
scope: browser safety policy
status: in-progress
depends-on: [input-012, diagnostic-014]
---

# Enforce side-effect and data-access policy

## Objective

Add a small typed policy layer that can harden untrusted agent workloads without
making normal local development cumbersome.

## Context

- `docs/architecture/automation.md`
- `SECURITY.md`

## Path

- `src/browser/`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `SECURITY.md`
- `docs/installation.md`
- `docs/mcp.md`

## Requirements

- Provide documented development and hardened presets.
- Gate schemes/hosts/private networks, evaluate, attach, persistent profiles,
  upload/download, screenshots, and filesystem paths before side effects.
- Support explicit allow/deny and a typed confirmation-required outcome without
  embedding a prompt UI in the browser layer.
- Validate configuration strictly and fail closed for invalid hardened policy.
- Keep evaluation and attached authenticated profiles visibly privileged.

## Verification

- Bypass tests for redirects, alternate URL forms, symlinks, private-network
  destinations, data/file URLs, and frontend inconsistencies.
- CLI/MCP/TUI enforce the same session policy.
- Policy checks add negligible hot-path memory and latency.

## Accepted contract

Policy is an immutable value owned by `BrowserSession`; every frontend selects
the same preset at session construction and cannot bypass checks with raw CDP.
`development` preserves the current local workflow. `hardened` permits only
public HTTP(S) navigation by default, requires an incognito owned browser, and
denies evaluate, attach, persistent profiles, upload, download, and screenshots
unless that capability is explicitly allowed.

Policy evaluation returns one of `Allow`, `Deny(reason)`, or
`RequireConfirmation(reason)`. Confirmation is a caller-supplied capability
token scoped to one operation class; the browser layer never prompts or treats
an omitted answer as approval. Invalid hardened configuration is a startup
error.

URL policy canonicalizes with the URL parser, rejects credentials and unknown
schemes, normalizes DNS names, resolves every address, and rejects loopback,
private, link-local, multicast, unspecified, and documentation-only network
ranges. Navigation interception applies the same decision to redirects before
Chrome follows them. Filesystem policy canonicalizes the existing target or
its nearest existing parent and then enforces the configured workspace root,
so `..`, alternate spellings, and symlinks cannot escape it.

Privileged capabilities remain visible in structured CLI/MCP results and logs:
JavaScript evaluation and attached authenticated profiles are never folded into
a generic browser failure.
