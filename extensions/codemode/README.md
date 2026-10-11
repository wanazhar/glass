# codemode

Lets the model write a small JavaScript program instead of making one tool call
per action.

Disabled by default. Enable it in `~/.config/glass/config.toml`:

```toml
extensions = ["node /path/to/glass-core/extensions/codemode/codemode.js"]
```

## Why it exists

Calling `bash` twenty times to filter and aggregate twenty tool results is
wasteful: every call round-trips through the model, and every intermediate
result lands in the context window. Codemode lets the model write one script
that calls the tools it needs, filters their output, and returns only the
summary worth keeping.

This is the capability pi ships as codemode, where it costs ~10 MiB of bundled
QuickJS. Here it costs zero bytes of harness: the interpreter is whatever the
user already has on their `PATH`, and the harness only knows the extension
protocol.

## How it works

The extension contributes two tools:

- `codemode` — run a JS program. The program calls `tools.bash(cmd)`,
  `tools.state(items)` and `read(name)` synchronously. Only the value the
  program returns reaches the model.
- `codemode_available` — whether a runtime is present, so the model can check
  before committing to a script.

Because the extension is a plain subprocess, you can read exactly what the
model is allowed to do before you enable it. That is the advantage over an
in-process sandbox: the security boundary is the process, and it is auditable
in one file.

## Output protocol (pi parity, task six-wave-e-001)

- Optional first line `// @options: {"max_output_tokens": 10000, "timeout_ms": 60000}`
  tunes the script. Unknown keys warn (they are reported in the result, never
  silently ignored); `timeout_ms` is capped at 120 s, `max_output_tokens` at
  100 k.
- `tools.text(value)` and the returned value are output items: with more
  than one, each item is preceded by a `==> text N/M <==` line so the model
  can tell them apart.
- `console.log`/`console.error` collect into one trailing `<console_output>`
  block instead of vanishing.
- Over the output budget the result is a bounded head+tail preview and the
  FULL text is spilled to a temp file whose path is named — same shape the
  bash tool uses — so a follow-up `bash` call can read the rest lazily.
- A script exceeding its timeout returns `codemode error: script timed out
  after <N>ms`; the extension process survives and answers later calls.

## Limits

- No filesystem or network access from the script beyond what `tools.bash`
  allows, which is everything `bash` already allows.
- Synchronous calls only. The protocol is request/response, so there is no
  promise combinator to await.
- The script cannot call the model. Codemode orchestrates tools; it does not
  recurse.
- No `image()` output and no `image` type in the sandbox; tools that produce
  images should be driven directly, not through codemode.
- `searchTools`/`describeTool`/`store` are not ported: the tool catalogue
  here is two tools, so there is nothing to search.
