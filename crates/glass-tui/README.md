# glass-tui

`glass-tui` provides the shared terminal rendering and input layer used by
Glass Core. It contains editor, layout, transcript, Markdown, syntax, and
terminal components; application state and product workflows remain with the
host application.

The crate is part of the root Cargo workspace and is not a separate Glass
product. Its source is the API reference for this internal integration
boundary. See the [terminal UI architecture](../../docs/architecture/tui.md)
and [Glass Core product documentation](../../products/glass-core/docs/INDEX.md)
for the user-facing context.
