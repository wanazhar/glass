# MCP Schema Budget

Glass maintains a bounded, audited MCP tool surface. This document inventories
the current tool definitions and their JSON Schema size for token-cost
accounting.

## Budget Target

**Target:** Well below Chrome DevTools MCP (~18k-token class). Glass targets
under 4k tokens for the full `tools/list` response.

## Current Tool Inventory

| Tool | Properties | Required | Notes |
|------|-----------|----------|-------|
| `navigate` | `url`, `timeoutMs` | `url` | |
| `click` | `target`, `selector` | one of | Locator forms |
| `clickExpectPopup` | `target`, `selector` | one of | Causal popup verification |
| `doubleClick` | `target`, `selector` | one of | |
| `hover` | `target` | `target` | |
| `drag` | `source`, `destination` | both | |
| `type` | `text`, `target` | `text` | |
| `key` | `key` | `key` | |
| `keyDown` | `key` | `key` | |
| `keyUp` | `key` | `key` | |
| `shortcut` | `shortcut` | `shortcut` | |
| `clear` | `target` | `target` | |
| `check` | `target` | `target` | |
| `uncheck` | `target` | `target` | |
| `select` | `target`, `value` | `target` | |
| `upload` | `target`, `files` | `target` | |
| `screenshot` | `format`, `quality`, `scale`, `fullPage`, `clip`, `target` | none | Seven optional properties |
| `observe` | `includeDom`, `includeScreenshot`, `includeFormValues` | none | Opt-in heavy payloads |
| `getDom` | (none) | — | |
| `getText` | (none) | — | |
| `reconcileReferences` | `fromRevision`, `refs` | both | Max 16 refs |
| `exportCheckpoint` | (none) | — | ≤ 4 KiB output |
| `importCheckpoint` | `checkpoint` | `checkpoint` | |
| `evaluate` | `expression` | `expression` | Policy-gated |
| `batch` | `steps` | `steps` | Max 32 steps; 14 action enum values |
| `scroll` | `dx`, `dy` | none | |
| `wait` | `condition`, `timeoutMs` | `condition` | |
| `diagnostics` | `durationMs` | none | 1–30s range |
| `acceptDialog` | (none) | — | |
| `dismissDialog` | (none) | — | |
| `download` | `destination`, `timeoutMs` | `destination` | |
| `listTargets` | (none) | — | |
| `createTarget` | `url` | `url` | |
| `selectTarget` | `id` | `id` | |
| `closeTarget` | `id` | `id` | |
| `listFrames` | (none) | — | |
| `selectFrame` | `id` | `id` | |
| `cookies` | (none) | — | Requires persistent profile |
| `setCookies` | `cookies` | `cookies` | Array of cookie objects |
| `clearCookies` | (none) | — | Requires persistent profile |
| `localStorage` | (none) | — | Bounded: 64 entries, 1 KiB per value |
| `sessionStorage` | (none) | — | Bounded: 64 entries, 1 KiB per value |
| `printToPdf` | `paperWidth`, `paperHeight`, `printBackground` | none | Returns base64 data |
| `fillForm` | `fields` | `fields` | Max 16 fields, atomic resolution |
| `clipboardRead` | (none) | — | Returns up to 8 KiB |
| `clipboardWrite` | `text` | `text` | Truncated to 8 KiB |
| `setGeolocation` | `latitude`, `longitude` | both | Override browser geolocation |
| `clearGeolocation` | (none) | — | Reset geolocation override |
| `setTimezone` | `timezoneId` | `timezoneId` | IANA timezone ID |

**Total: 59 tools.**

## Schema Size Estimate

Each tool contributes:
- `name`: ~15-25 bytes
- `description`: ~40-80 bytes
- `inputSchema`: ~80-300 bytes (typically `{"type":"object","properties":{...}}`)

Measured `tools/list` response size: **~12.7 KiB** (about 3.2k tokens using
the repository's four-bytes-per-token estimate, well under the 18k-token
reference for Chrome DevTools MCP). Re-measure with
`GLASS_BINARY_PATH=target/debug/glass node benchmarks/schema-scoreboard.mjs`.

## Design Principles

1. **Stable verbs, not tool sprawl.** The surface uses a fixed set of action
   names (`navigate`, `click`, `type`, etc.) rather than creating new tools
   for every operation variant.
2. **Locator forms are a single parameter.** `target` accepts all locator
   forms (ref, name, role+name, text, CSS, ordinal). One input, not six tools.
3. **Heavy payloads are opt-in.** `observe` returns compact accessibility by
   default; DOM and screenshots require explicit boolean flags.
4. **No tool accepts unbounded arrays without a documented cap.** `batch.steps`
   caps at 32; `reconcileReferences.refs` caps at 16.
5. **No tool duplicates an existing capability under a different name.**

## Rejection Criteria for New Tools

A proposed tool is rejected if it:
- Duplicates an existing verb (use a parameter, not a new tool)
- Accepts unbounded input without a documented cap
- Exposes raw CDP directly (use `evaluate` with policy gating instead)
- Introduces tool-specific output schemas larger than the existing observe
  response
- Requires a new CDP domain that is not already optionally enabled

## Periodic Audit

This document is updated whenever tools are added, removed, or renamed. The
`tools/list` response size is measured on each release and must not exceed
12 KiB without a documented justification in the changelog.
