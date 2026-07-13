---
id: policy-016
scope: browser safety policy
status: complete
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

## Completion evidence

- Development and hardened presets are shared by CLI, MCP, and TUI. Hardened
  startup requires an owned incognito browser and an exact host allow list
  unless attach/profile authority is explicitly granted.
- Session-lifetime Fetch interception covers redirects, popups, selected
  targets, and attached child/OOPIF targets. New targets wait for the debugger
  until interception is enabled; cancellation and lag remain fail closed.
- Allowed DNS names resolve only to public destinations and are pinned into
  owned Chrome with resolver rules, eliminating independent-resolution
  rebinding. Hardened attach accepts public IPv4 literals only.
- Evaluate, attach, persistence, upload, download, screenshot/screencast, and
  raw CDP are typed capability checks. Confirmation tokens are consumable once;
  the unlimited raw-CDP escape hatch requires an explicit allow instead.
- URL alternate forms, reserved/private IP ranges, trailing-dot hosts,
  conflicting configuration, and symlink path escapes have deterministic
  tests. Real Chromium successfully navigated and created a target under an
  exact pinned hardened host rule.
- One million allowed capability checks measured 5 ns/check in a release build.
  The release CLI grew from 5,054,640 to 5,120,208 bytes (+65,568, 1.30%).
- `cargo test --all-targets --all-features` and strict all-feature Clippy pass;
  independent adversarial review passed at `255273b`.
