---
id: model-fallback-001
scope: crates/glass provider chain, config, provider construction
status: done
depends-on: []
---
# Model fallback

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Automatic model fallback: `fallback = ["model-b", ...]` in config; when the
primary fails before any event is emitted, the next model serves the turn,
with a visible warning naming the swap. Ported from the provider failover
and account-rotation features in Amp, opencode, and Codex's route-around
guidance (research/harness-features-2026.md).

## Context

- The safety rule matches `stream_with_retry`: fall through only when the
  failed attempt produced no events, which is exactly what
  `Provider::stream` returning `Err` means. Once a stream starts, the chain
  never abandons it.
- 401/403 are credential problems and are never routed around; the next
  model would fail identically.
- Each level is individually metered with its own rate card, so accounting
  follows the model that actually served the request.

## Paths

- `crates/glass/src/provider/chain.rs` (new): `Chain` provider — ordered
  levels, per-level request rebuild (the model id travels in the request),
  leading `Warning` event on swap, first-error reporting, primary identity
  and context window.
- `crates/glass/src/config.rs`: `fallback` list, blank entries dropped.
- `crates/glass/src/main.rs`: `make_metered` + `make_provider` build the
  chain; unusable fallback entries are skipped with a note.

## Integration

Config → `make_provider` → primary + fallback levels wrapped in `Metered` →
`Chain::stream` → `stream_with_retry` per the existing turn loop. Applies to
the interactive session, headless runs, delegated sub-agent tasks, and the
cache warmer because they all share the provider factory.

## Verification

- Unit tests with stub providers: healthy primary (no warning), transient
  fallthrough (warning names primary/model/status; fallback receives its own
  model id), auth never routed around, all-fail reports the primary's error,
  identity/window from the primary.
- Config tests: defaults, blank-entry trimming.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
