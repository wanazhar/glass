# Hashline extension

Hashline provides content-anchored line edits. It is optional; native revision-based
read, edit, and write tools remain available in the core.

```toml
extensions = ["python3 /path/to/glass-core/extensions/hashline/hashline.py"]
```

## Read and edit

`hl_read` returns line numbers and content hashes. Use the returned anchors in
`hl_edit` operations instead of reproducing the original text. For example:

```json
{"path":"src/a.rs","ops":[
  {"op":"replace","line":"2:aa5282df","text":"    println!(\"updated\");"},
  {"op":"insert","after":"2:aa5282df","text":"    // reviewed"}
]}
```

The example anchors are illustrative; obtain actual anchors from the current
`hl_read` result. Replace, insert, and delete operations validate against the
pre-edit file. A stale anchor rejects the batch before writing. Writes use a
same-directory temporary file and replacement. Untouched line endings are retained.

Anchors detect changes to the referenced lines. They are distinct from the native
file tool's whole-file revision check. Cooperating tool calls are ordered by path;
external writers can still race file replacement.

## Concurrency and access

`hl_read` declares parallel execution; `hl_edit` declares keyed execution. Calls
on different files may overlap. Calls sharing a key remain ordered.

Enable the extension only after reviewing its code and applicable permissions.
See [Native tools](../../docs/tools.md) for revision-based edits and diagnostics.
