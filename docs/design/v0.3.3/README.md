# v0.3.3 remote cockpit design evidence

The issue-pinned Android and iOS JPEGs are release inputs, not decorative
mockups. Both decode in the release gate. The iOS JPEG is rendered from
`remote-ios-concept.svg`; retaining the source prevents another opaque or
truncated binary from being accepted.

The implemented hierarchy is validated at both constrained and full portrait
sizes. The deterministic
`phone_overview_matches_remote_mock_hierarchy_at_ios_portrait` test renders a
populated 46×50 Ratatui buffer and verifies the mock ordering, rounded card
separation, status chips, preview inset, colored agent surface, panel
background, composer, and navigation. A 40×20 test verifies that the Overview
collapses into a page-bounded priority window and that later semantic/process
cards remain reachable. The Linux
`phone_tui_renders_and_leaves_a_real_terminal_cleanly` integration test then
starts the packaged executable on a real 40×20 pseudo-terminal, captures its
actual ANSI render, checks the title, Overview surface and printable Command
composer, and proves terminal restoration. Adjacent tests cover all six
reconnect keys, current/legacy capsule aliases, printable 1–6 navigation,
semantic tap priority, portrait ANSI bounds, stale pointer rejection, and
geometry-only breakpoints. The issue designs are therefore checked against
both styled cells and a real terminal lifecycle, not by testing title text
alone.

The cards are an interaction contract as well as a visual hierarchy. Portrait
tests click Browser, Agent, Understanding and Process hit regions, route the
nested Remote View control through the real command parser, distinguish an
actual received frame from a merely connected browser, reject browser readiness
as semantic-confidence evidence, prevent diagnostics from completing an agent
task, and build the Overview thumbnail from a Kitty-backed frame. The PTY test
also proves mouse, focus, and bracketed-paste reporting are enabled on entry and
disabled on restoration.

The native interaction layer is covered separately from the visual contract.
`phone_action_dock_palette_send_and_confirmation_are_real_controls` exercises
touch hit regions for the contextual dock and composer, searchable action-sheet
dispatch, busy-operation cancellation, and the abort confirmation boundary.
`phone_command_history_is_bounded_process_only_state` verifies recall, the
32-command memory bound, and exclusion from reconnect capsules. Focus loss
suspends live acquisition; focus gain queues a fresh structured observation;
retained pixels are visibly marked paused or stale until that evidence is
current. The capsule round-trip test preserves only the bounded mobile scroll
position alongside the existing target, revision, attention, and live-mode
metadata.

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
