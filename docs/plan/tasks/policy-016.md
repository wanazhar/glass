---
id: policy-016
scope: browser safety policy
status: pending
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
