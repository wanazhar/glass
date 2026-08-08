# glass-dev

The `glass` executable for Glass's terminal-native development runtime and
local browser workspace.

Install it with:

```console
cargo install glass-dev --locked
```

The reusable browser API remains available from the `glass-browser` crate.

Narrow SSH and Mosh terminals automatically receive the single-pane mobile
workspace. Use `glass --tui-layout mobile` to force it. Herdr is the
recommended optional multiplexer for persistent agent panes. Use `live on` for
an adaptive Herdr, Kitty, or ANSI terminal view; the default remains semantic.
The TUI command `safari` explains the stable full-fidelity path through a
private SSH local port forward. See the repository's
[mobile and remote guide](../../docs/mobile-remote.md).
