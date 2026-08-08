id: tui-recovery-033-002
scope: Glass v0.3.3 responsive TUI and browser lifecycle
status: completed
depends-on: [presentation-033-001]

## objective

Implement the accepted phone/compact/wide information architecture, printable
navigation and filtered palette, long-lived browser connection controller,
port classification/automatic launch, in-place recovery and target selection.

## context

- `docs/plan/analysis/release-033.md`
- `docs/architecture/browser-connection.md`
- `docs/architecture/mobile-cockpit.md`
- `docs/architecture/development-tui.md`

## path

- browser endpoint/startup contracts
- `crates/glass-browser/src/tui/`
- CLI browser-session configuration
- TUI snapshots and integration tests
- affected docs/design assets

## verification

- arbitrary-port compatible/unrelated/unknown/free fixtures
- desktop and phone recovery/target-picker snapshots
- disconnect/reconnect and workspace-survival tests
- all essential actions reachable without function keys

## result

Completed locally on 2026-08-08. The responsive six-view shell, printable
navigation, palette, long-lived controller, endpoint classification, target
selection, and phone/desktop recovery paths are integrated and tested.
