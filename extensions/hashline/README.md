# hashline

Content-anchored edits for glass. Disabled by default.

```toml
extensions = ["python3 /path/to/glass-core/extensions/hashline/hashline.py"]
```

## What it does

`hl_read` returns every line with a short hash:

```
1:37f9b8e4|fn main() {
2:aa5282df|    println!("hi");
3:c6f6|}
```

`hl_edit` then refers to lines by anchor instead of re-typing them:

```json
{"path": "src/a.rs", "ops": [
  {"op": "replace", "line": "2:aa5282df", "text": "    println!(\"bye\");"},
  {"op": "insert",  "after": "2:aa5282df", "text": "    // note"},
  {"op": "delete",  "line": "3:c6f6"}
]}
```

## Why

A model asked to reproduce a block of text byte-for-byte fails often enough to
matter. One published benchmark measured patch application failing 50.7% on one
model and 46.2% on another, and pass rates moved up to 64.6 percentage points
purely by changing the edit format. Another harness found a 2.4% first-call
parse-error rate produced a 21-point drop in task success.

Anchoring replaces recall with verification. The model names a line instead of
reconstructing it, so whitespace and exact spelling stop mattering. A stale
anchor is **rejected before anything is written**:

```
stale anchor on line 2: you said 2:dead, the file now says 2:aa5282df.
  line 2 is now:     println!("hi");
Re-read the file and use the current anchors. Nothing was written.
```

Edits are **all-or-nothing**: every op is validated against the pre-edit state
first, so a batch cannot half-apply because its third op had a stale anchor.
Writes go through a temp file and `os.replace`, so a reader never sees a
half-written file and a crash mid-write leaves the original intact.

## Why it is an extension, not a core tool

glass keeps two core tools on purpose. Adding `read`/`write`/`edit` would cost
tokens on every request, and the measured evidence is that a bash-only surface
scores *higher* than a typed tool catalogue while using 19–72% fewer tokens.

What the research does support is fixing *edit format reliability*. This
extension is that fix, opt-in, costing zero bytes of binary. Two tools in the
core, better edits when you want them.

## Concurrency

`hl_read` is `parallel`, `hl_edit` is `keyed`. Two edits to different files
overlap; two edits to the same file stay ordered.
