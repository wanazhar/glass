# Oracle review extension

Enable `oracle` for an explicit `/oracle` review using an alternate model.
Configure `model` and optional `prompt` in `~/.config/glass-core/oracle.json`,
or select a file through `GC_ORACLE_CONFIG`. `GC_ORACLE_MODEL` overrides the model.

## Behavior and limits

The extension sends a bounded working diff through the read-only delegate path.
Requests consume provider quota and use session accounting. The configured
model must be available on the active connection. Local review views remain separate.

See the [user reference](../../docs/agents.md#model-assisted-review) and the implementation in this directory.
