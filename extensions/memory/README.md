# memory

Durable, cross-session project memory — the `MEMORY.md` convention the big
harnesses converged on, in glass-core's extension model.

- **Layout**: `~/.config/glass-core/memory/<slug-of-canonical-workspace>/`
  (or `GC_MEMORY_DIR`). `MEMORY.md` is the index, one line per memory; every
  other `*.md` is a topic file read on demand.
- **Injection**: the index is capped at 200 lines / 25 KB (truncation marked)
  and added to the system prompt at session start, so the model begins every
  session knowing what earlier ones learned.
- **Tool**: `memory {action: add|read|list|remove, name?, text?}`. `add`
  writes or replaces a named memory (lowercase `[a-z0-9._-]`, ≤64 chars) and
  keeps the index sorted; `read` caps at 8 KB; `remove` deletes both file
  and index line.
- **Command**: `/memory` prints the index and the directory path.

User-level only by design: a repository can never add memories that flow
into its own prompt, so cloning an untrusted repo changes nothing here.
This matters for a durable-consent model — the trust gate covers repository
instructions, and memory is deliberately outside that surface.

Use it for conventions, gotchas, and user preferences. Transient task state
belongs in `state` (which survives compaction but not the session).

Offline checks: `python3 memory.py --selftest` and `scripts/smoke-memory.py`.
