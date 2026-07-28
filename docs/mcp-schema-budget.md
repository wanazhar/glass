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
| `navigate` | `url`, `timeoutMs`, `expectedRevision` | `url` | Optional revision guard |
| `click` | `target`, `selector`, `expectedRevision` | one of | Locator forms; optional revision guard |
| `clickExpectPopup` | `target`, `selector` | one of | Causal popup verification |
| `doubleClick` | `target`, `selector` | one of | |
| `hover` | `target`, `selector` | one of | |
| `drag` | `source`, `destination` | both | |
| `key` | `key` | `key` | |
| `keyDown` | `key` | `key` | |
| `keyUp` | `key` | `key` | |
| `shortcut` | `shortcut` | `shortcut` | |
| `clear` | `target`, `selector` | one of | |
| `check` | `target`, `selector` | one of | |
| `uncheck` | `target`, `selector` | one of | |
| `select` | `target`, `value` | `target` | |
| `upload` | `target`, `files` | `target` | 1–16 regular files |
| `type` | `text`, `target`, `expectedRevision` | `text` | Optional target and revision guard |
| `screenshot` | `format`, `quality`, `scale`, `fullPage`, `clip`, `target` | none | Explicit visual capture |
| `observe` | `includeDom`, `includeScreenshot`, `includeFormValues` | none | Heavy payloads are opt-in |
| `preflight` | `target`, `action` | `target` | No browser mutation |
| `clickAt` | `x`, `y` | both | Policy-gated coordinate click |
| `getDOM` | (none) | — | Full DOM |
| `getText` | (none) | — | Visible text |
| `reconcileReferences` | `fromRevision`, `refs`, `hints`, `scopeRef` | `fromRevision`, `refs` | Bounded reconciliation |
| `observeDelta` | (none) | — | Bounded observation delta |
| `setNetworkConditions` | `preset`, `offline`, `latencyMs`, `downloadThroughput`, `uploadThroughput` | none | Scoped emulation |
| `clearNetworkConditions` | (none) | — | |
| `setCpuThrottling` | `rate` | `rate` | Scoped emulation |
| `clearCpuThrottling` | (none) | — | |
| `setUserAgent` | `userAgent`, `acceptLanguage`, `platform` | `userAgent` | Scoped override |
| `clearUserAgent` | (none) | — | |
| `exportCheckpoint` | (none) | — | Bounded checkpoint |
| `importCheckpoint` | checkpoint fields | schema version and fields | Bounded checkpoint |
| `evaluate` | `expression` | `expression` | Policy-gated |
| `batch` | `steps`, `atomic`, `mode`, `expectedRevision` | `steps` | Max 32 steps; mode is fixed, chain, or unguarded |
| `verify` | `predicate`, `timeoutMs` | `predicate` | Predicate depth 4, fan-out 8, deadline bounded |
| `scroll` | `dx`, `dy` | none | |
| `wait` | `condition`, `timeoutMs` | `condition` | Bounded deadline |
| `diagnostics` | `durationMs` | none | Bounded redacted evidence |
| `acceptDialog` | (none) | — | |
| `dismissDialog` | (none) | — | |
| `dismissConsent` | (none) | — | Recognized consent controls |
| `download` | `destination`, `timeoutMs` | `destination` | Scoped download lifecycle |
| `listTargets` | (none) | — | |
| `createTarget` | `url` | `url` | Does not select the target |
| `selectTarget` | `id` | `id` | |
| `closeTarget` | `id` | `id` | |
| `listFrames` | (none) | — | |
| `selectFrame` | `id` | `id` | |
| `cookies` | (none) | — | Persistent profile |
| `setCookies` | `cookies` | `cookies` | Persistent profile |
| `clearCookies` | (none) | — | Persistent profile |
| `localStorage` | (none) | — | Bounded storage |
| `sessionStorage` | (none) | — | Bounded storage |
| `printToPdf` | `paperWidth`, `paperHeight`, `printBackground` | none | Returns PDF data |
| `fillForm` | `fields`, `expectedRevision` | `fields` | Max 16 fields; optional revision guard |
| `clipboardRead` | (none) | — | Bounded text |
| `clipboardWrite` | `text` | `text` | Bounded text |
| `setGeolocation` | `latitude`, `longitude` | both | Override geolocation |
| `clearGeolocation` | (none) | — | |
| `setTimezone` | `timezoneId` | `timezoneId` | IANA timezone ID |

**Total: 70 tools.**

## Schema Size Estimate

Each tool contributes:
- `name`: ~15-25 bytes
- `description`: ~40-80 bytes
- `inputSchema`: ~80-300 bytes (typically `{"type":"object","properties":{...}}`)

The 0.2.0 release measures 21,070 UTF-8 bytes, or an estimated 5,268
tokens using the repository's four-bytes-per-token method. Re-measure a local
build with `GLASS_BINARY_PATH=target/debug/glass node
benchmarks/schema-scoreboard.mjs` when the tool descriptions or schemas change.

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
