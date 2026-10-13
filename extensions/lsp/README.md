# Language-server extension

Enable `lsp` and configure language-server argument arrays in
`~/.config/glass-core/lsp.json`, or `GC_LSP_CONFIG`. Servers start lazily and
remain available for the session. `/lsp` reports their state.

## Behavior and limits

Diagnostics report introduced and resolved problems with bounded per-file output.
Rename and code actions produce previews with target revisions. Applying supported
text edits checks every affected revision; unsupported server commands are refused.
External writers can still race a final replacement.

See the [user reference](../../docs/reference/workspace.md) and the implementation in this directory.
