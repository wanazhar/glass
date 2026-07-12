---
id: diagnostic-014
scope: scoped browser diagnostics
status: done
depends-on: [wait-010, topology-011]
---

# Add bounded console, network, dialog, and download evidence

## Objective

Expose the evidence agents need to diagnose and complete workflows while
keeping expensive CDP domains disabled outside explicit scopes.

## Context

- `docs/architecture/automation.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `docs/architecture/`

## Requirements

- Lease-enable Runtime/Log, Network, dialog, and download monitoring domains.
- Bound event counts, field sizes, body capture, and retention time.
- Redact cookies, authorization, request bodies, typed secrets, and URL query
  values by default.
- Add dialog accept/dismiss and download lifecycle primitives.
- Report dropped-event counts rather than silently losing evidence.

## Verification

- Real console error, failed request, redirect, auth-header redaction, dialog,
  download, lag, and lease-cleanup tests.
- Default observation has no Network-domain or diagnostic-retention regression.

## Completion

- Added explicit CLI, MCP, and library diagnostic scopes with route-keyed
  Runtime, Log, and Network leases, immutable session filtering, 30-second
  deadlines, bounded fields/vectors, and explicit dropped-event counts.
- URL credentials and query values, sensitive header names and all header
  values, request bodies, and Runtime console argument values are omitted or
  redacted. Browser Log text receives conservative marker and URL redaction.
- Added accept/dismiss dialog primitives and a serialized browser-global
  download lifecycle with authorized destinations, frame correlation, bounded
  outcomes, and deny-on-drop cleanup.
- Deterministic cancellation coverage proves all leased domains disable when a
  diagnostic future is dropped. Real Chrome covers console errors, failed
  requests, redirects, auth/query redaction, both dialog outcomes, and download
  terminal evidence; default observation continues to enable only Page and DOM.

## Commit

`feat: add scoped browser diagnostics`
