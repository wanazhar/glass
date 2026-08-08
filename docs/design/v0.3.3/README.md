# v0.3.3 remote cockpit design evidence

The issue-pinned Android and iOS JPEGs are release inputs, not decorative
mockups. Both decode in the release gate. The iOS JPEG is rendered from
`remote-ios-concept.svg`; retaining the source prevents another opaque or
truncated binary from being accepted.

The implemented hierarchy is validated twice at 40×20. The deterministic
`phone_layout_renders_at_portrait_size_and_has_no_graphics_pane` test renders
through Ratatui's cell backend and verifies the Overview and Process endpoints,
composer, and absence of a graphics pane. The Linux
`phone_tui_renders_and_leaves_a_real_terminal_cleanly` integration test then
starts the packaged executable on a real 40×20 pseudo-terminal, captures its
actual ANSI render, checks the title, Overview card and printable Command
composer, and proves terminal restoration. Adjacent tests cover printable 1–6
navigation, semantic tap priority, portrait ANSI bounds, stale pointer
rejection, and geometry-only breakpoints. The issue designs were checked
against both cell and real-terminal output.

Interaction references applied to the final design:

- [Claude Code interactive mode](https://code.claude.com/docs/en/interactive-mode):
  persistent composer, `?` help, `Ctrl-L` redraw, visible background work,
  and mouse-optional operation;
- [Codex CLI](https://developers.openai.com/codex/cli/features) and its
  [developer commands](https://developers.openai.com/codex/cli/slash-commands):
  command discovery, status composition, bounded background terminals, and
  task-safe command availability;
- [Lazygit keybindings](https://github.com/jesseduffield/lazygit/blob/master/docs/keybindings/Keybindings_en.md):
  numbered panels, contextual help, Enter/Escape predictability, and a focused
  full-width panel on constrained terminals.

Glass keeps its own identity: semantic state and revision evidence lead;
pixels are an explicit assist; recovery is a first-class Browser view; and
layout, transport, graphics, shell, and multiplexer are never collapsed into
one remote guess.
