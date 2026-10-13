# Python structure helper

`gc structure` invokes optional Python AST previews and revision-checked edits.
The helper supports selected identifiers and uniquely identified expressions
while retaining unrelated spelling and comments.

## Behavior and limits

Inputs must be regular UTF-8 files within the workspace and at most 512 KiB.
Ambiguous expressions require a location. Syntax and revision checks run before
replacement. This helper is not a general language refactoring engine.

See the [user reference](../../docs/reference/workbench.md) and the implementation in this directory.
