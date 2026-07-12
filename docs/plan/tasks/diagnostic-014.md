---
id: diagnostic-014
scope: scoped browser diagnostics
status: in-progress
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
