id: tui-debug-020
scope: TUI action debugger activity
status: done
depends-on: [effects-020]

## objective

Make the existing TUI activity pane useful for bounded action debugging by
showing each action execution ID, resulting revision, and observed browser
effects without exposing page content.

## path

- `src/tui/app.rs`
- GitHub issue #20

## verification

- TUI action activity includes the execution identity and observed effects.
- Existing TUI parser and PTY smoke tests remain valid.
- No remote push, tag, or publication occurs.
