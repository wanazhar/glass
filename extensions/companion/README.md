# Companion helper

`gc companion SOCKET [PORT]` starts a loopback browser interface for a running
served session. This Python helper is invoked explicitly; it is not an extension
started through the normal tool handshake.

## Behavior and limits

The private URL authorizes access. The client starts as an observer and requires
an explicit input claim. Reconnection returns to observer mode. Use an explicit
SSH tunnel for access from another device and protect the access URL.

See the [user reference](../../docs/reference/workbench.md#automations-and-optional-clients) and the implementation in this directory.
