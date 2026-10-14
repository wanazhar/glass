# Todo compatibility adapter

The optional `todo` extension forwards add, check, uncheck, remove and list calls
to GC's native `state.todo` checklist. It keeps no task state in the Python process.
Writes return changed tasks; list reads the complete checklist and revision.
Legacy 1-based indices are resolved against a native read and guarded by its
revision, so a stale index cannot silently modify a different task.

Native checklists require no extension. New agents should use `state.todo` to
batch related changes and address stable task IDs. `/todo` opens the same durable
checklist in the terminal. Old textual snapshots can be imported explicitly from
that modal; a session with no surviving snapshot cannot be reconstructed.

See the [checklist reference](../../docs/reference/checklist.md) for status,
persistence, ownership, limits and continuation behavior.
