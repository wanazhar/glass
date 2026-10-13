# ACP and DAP adapters

Enable the bundled `adapters` extension through `/extensions`. Configure named
ACP harness or DAP debugger commands in `~/.config/glass-core/adapters.json`.
Commands must be explicit argument arrays. Processes start on demand, not during
the extension handshake.

## Behavior and limits

External-agent execution and debugger execution require separate opt-in flags.
Reverse execution requests are refused. Long operations return tickets with
bounded event polling. Disabling the extension stops its owned adapter processes.

See the [user reference](../../docs/reference/workbench.md) and the implementation in this directory.
