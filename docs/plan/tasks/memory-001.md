---
id: memory-001
scope: extensions/memory
status: done
depends-on: []
---

## Objective

Cross-session project memory following the `MEMORY.md` convention (Claude
auto memory / Codex memories / Maki), shipped as an out-of-process
extension: a per-project directory with a size-bounded index injected into
the system prompt, topic files read on demand, and add/read/list/remove as
a tool.

## Context

- User-level only (`~/.config/glass-core/memory/<slug>/`, `GC_MEMORY_DIR`
  overrides): a repository can never add memories to its own prompt, so
  memory stays outside the trust gate — deliberate, documented.
- Index cap 200 lines / 25 KB with a visible truncation marker; read cap
  8 KB; names constrained to lowercase `[a-z0-9._-]{1,64}`.
- Injection uses the `system_prompt` hook, which the host caches once per
  session; the index is therefore read at handshake.

## Paths

- `extensions/memory/memory.py` + README (new); `addons.rs` bundle entry.
- `scripts/smoke-memory.py` (protocol-level, no binary needed).

## Verification

- Selftest: round-trip, replace-keeps-one-line, caps, name validation.
- `scripts/smoke-memory.py`: hello injection, add/read/list/remove through
  the wire protocol, caps enforced.
