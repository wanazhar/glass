# Codemode extension

Run a JavaScript program that combines host tool calls and returns selected output.
The extension requires Node and is disabled by default.

```toml
extensions = ["node /path/to/glass-core/extensions/codemode/codemode.js"]
```

## Interface

The `codemode` tool accepts a `code` string. It provides `tools.bash(command)`,
`tools.state(items)`, and helpers for lines, filtering, unique values, counting,
joining, and emitted text. Top-level `await` is accepted; host calls are synchronous.
The script cannot request model inference or recursively delegate through another
model call.

```javascript
const output = await tools.bash("git status --short");
return output;
```

Use a script when filtering several results would otherwise require repeated
model requests. A single ordinary shell command does not need this extension.
Intermediate values remain in the script unless returned or emitted.

## Output and limits

An optional first line configures output and timeout:

```javascript
// @options: {"max_output_tokens": 10000, "timeout_ms": 60000}
```

Timeouts are capped at 120 seconds and output allowances at 100,000 tokens.
Unknown options produce a warning. `tools.text(value)` and the returned value
become labeled output items. Console messages appear in a trailing
`<console_output>` block. Output exceeding its allowance uses a bounded head/tail
preview with a path to the saved full text. A timed-out script returns an error;
the extension remains available for later calls.

## Access

The JavaScript context omits direct `require`, `process`, and timer access.
It is not an OS security boundary. `tools.bash` has the host access permitted by
the applicable execution policy, and the extension itself is executable local
code. Configure confinement separately; see [Permissions](../../docs/security.md).
Image output and tool-catalogue discovery are not implemented in this extension.
