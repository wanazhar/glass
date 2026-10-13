# Markdown memory extension

Store project-specific Markdown references across sessions. This optional extension
has a separate store and inclusion policy from native neuralnets.

## Storage and retrieval

Files live under `~/.config/glass-core/memory/<project>/`, or `GC_MEMORY_DIR`.
`MEMORY.md` is the index; other Markdown files hold topics. The index contributes
to the system prompt at session start, capped at 200 lines / 25 KiB with marked
truncation. Topic reads are capped at 8 KiB.

The `memory` tool accepts `action: add|read|list|remove`, with `name` and `text`
where required. Add replaces a named topic and updates the sorted index. Names
use lowercase letters, digits, dots, underscores, or hyphens, with a 64-character
limit. Remove deletes the topic and its index entry.

The native `/memory` command opens neuralnets. This extension does not replace
that command or migrate native facts. See [Project knowledge](../../docs/knowledge.md)
for native review controls, migration, and retrieval limits.

## Access

Memory files are user-level configuration. Enabling the extension permits its
saved index to enter prompt context. Review saved content and readable-file
permissions; project trust and execution confinement remain separate controls.
Use `state` for notes specific to the current conversation.

Checks: `python3 memory.py --selftest` from this directory, or
`python3 scripts/smoke-memory.py` from the repository root.
