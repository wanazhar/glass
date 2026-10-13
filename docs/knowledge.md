# Project knowledge and model guidance

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Neuralnets

Neuralnets stores attributed project knowledge locally. Agents save useful lessons,
search for relevant memories, correct outdated entries and forget stale information
through the `state` tool. No approval is required. `/neuralnets` and `/memory` open
the memory browser, where you can inspect sources, edit entries and remove them.

A save becomes active immediately and is available to search/read in the same turn.
The bounded automatic reference snapshot refreshes on the next user turn; saves do
not rewrite the system prefix between tool calls. Memory is reference data: it does
not grant permissions or override current instructions. Save durable facts,
procedures and verified corrections rather than task progress, raw transcripts,
credentials or guesses about preferences.

Model API examples:

```json
{"neuralnets":{"action":"save","kind":"lesson","text":"Run focused release checks after changing terminal layout."}}
{"neuralnets":{"action":"search","query":"terminal layout"}}
{"neuralnets":{"action":"update","id":"ENTRY_ID","expected":3,"text":"Updated verified lesson."}}
{"neuralnets":{"action":"forget","id":"ENTRY_ID","expected":4}}
```

`id` and `expected` come from the latest read/search/list result. Updates and removals
reject stale revisions so concurrent agents do not overwrite each other's changes.
Duplicate saves reuse the existing entry without a write. Forgotten entries remain
available for inspection and explicit restoration; search excludes them.

```sh
gc neuralnets save lesson "Use the release binary for PTY checks" --json
gc neuralnets list --json
gc neuralnets search "test setup" --json
gc neuralnets --help
gc guidance search "background jobs"
```

Detailed recipes are loaded on demand. `gc guidance` uses the same catalogue as
native discovery. Context manifests disclose supplied references; they do not
prove that a model used them.

## Existing facts and migration

`gc memory` retains the explicit fact interface:

```sh
gc memory set test-command "Run cargo test --workspace" "Project maintainer"
gc memory enable
gc memory list --json
gc memory forget test-command
```

Facts retain their keys, sources, review dates, and enable setting. Setting an
existing key replaces its value. Reviewed-fact retrieval remains opt-in.

Reviewed facts and neuralnets learning share one native store and lock. On the
first explicit mutation, legacy native facts are copied into that store atomically.
Reading the legacy fact store does not write it. Existing pending/confirmed
Neuralnets entries are treated as active on load; the next memory mutation saves
the normalized status. No approval step is needed for those earlier learnings. The old file remains as a backup;
use the current application version for later writes. Optional extension-memory
files are separate and are not migrated.

## Retrieval and storage limits

| Data | Limit |
| --- | --- |
| Retrieved reviewed facts | 16 facts / 8 KiB of escaped JSON |
| Retrieved active learning | 8 entries / 2 KiB of escaped JSON |
| Neuralnets entries | 512 entries |
| Combined native store | 1,088 KiB |

Retrieval deduplicates identical included fact text. Conflicting entries remain
available for inspection. Active saves, updates and removals change the next user
turn's reference snapshot. `propose` remains an alias for immediate saving, and
`approve` remains a compatibility alias for activation; neither is required.

Writes use private atomic storage, a nonblocking lock, strict parsing, and optimistic
revisions. Invalid stores are reported without discarding them. Read-only agents may inspect memory but cannot mutate it. An unrestricted shell can still invoke CLI commands
or edit readable files; these controls are not an OS security boundary.

## Optional memory extension

The separate `memory` extension stores Markdown topic files under
`~/.config/glass-core/memory/<project>/`, or `GC_MEMORY_DIR`. Its `MEMORY.md` index
is injected at session start, capped at 200 lines / 25 KiB. Topic bodies are read
on demand, with their own limits.

```toml
extensions = ["python3 /path/to/glass-core/extensions/memory/memory.py"]
```

The extension's `memory` tool supports `add`, `read`, `list`, and `remove`. It has a
different storage and inclusion policy from native neuralnets: enabling the
extension allows its saved index to contribute prompt content. The native
`/memory` command opens neuralnets. See the
[extension reference](../extensions/memory/README.md).

## Task notes and recaps

Use `state` for task notes in the current conversation. Local recaps display saved
excerpts without requesting model inference. Neither a recap nor a context
manifest verifies that the underlying work was completed. See [Sessions](sessions.md)
for recovery and context edits.
