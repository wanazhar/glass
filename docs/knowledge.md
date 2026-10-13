# Project knowledge and model guidance

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Neuralnets

Neuralnets stores attributed project knowledge locally. `/neuralnets` and `/memory`
open the shared review interface. The `state` tool can discover guidance and propose
learning without adding another core model tool.

Entries have pending, confirmed, or forgotten lifecycle states. Model proposals
start pending and stay out of retrieved context. Human review through the native
API is revision-checked. Learning content is reference data; it grants no
permissions and cannot override current instructions.

```sh
gc neuralnets list --json
gc neuralnets search "test setup" --json
gc neuralnets --help
gc guidance search "background jobs"
```

Detailed recipes are loaded on demand. `gc guidance` uses the same catalogue as
native discovery. Context manifests disclose supplied references; they do not
prove that a model used them.

## Reviewed facts and migration

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
Reading the legacy store does not migrate it. The old file remains as a backup;
use the current application version for later writes. Optional extension-memory
files are separate and are not migrated.

## Retrieval and storage limits

| Data | Limit |
| --- | --- |
| Retrieved reviewed facts | 16 facts / 8 KiB of escaped JSON |
| Retrieved confirmed learning | 8 entries / 2 KiB of escaped JSON |
| Neuralnets entries | 512 entries |
| Combined native store | 1,088 KiB |

Retrieval deduplicates identical included fact text. Conflicting entries remain
available for review. Pending entries are excluded. Approved or forgotten content
changes the next turn's reference snapshot; proposals alone leave it unchanged.

Writes use private atomic storage, a nonblocking lock, strict parsing, and review
revisions. Invalid stores are reported without discarding them. Review controls
protect the native model API. An unrestricted shell can still invoke CLI commands
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
different inclusion and review policy from native neuralnets: enabling the
extension allows its saved index to contribute prompt content. The native
`/memory` command opens neuralnets. See the
[extension reference](../extensions/memory/README.md).

## Task notes and recaps

Use `state` for task notes in the current conversation. Local recaps display saved
excerpts without requesting model inference. Neither a recap nor a context
manifest verifies that the underlying work was completed. See [Sessions](sessions.md)
for recovery and context edits.
