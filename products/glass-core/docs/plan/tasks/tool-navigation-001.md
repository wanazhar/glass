---
id: tool-navigation-001
scope: searchable tool inspection and keyboard discovery
status: in-progress
depends-on: [live-turn-ui-001]
---
# Tool navigation

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

User requested additional features to ship. Current `/details` is a static newest-first chooser and readonly output modal; `/help` lists slash commands with generic shortcut prose. Add a searchable tool inspector and a `/keys` discovery modal after live turn rendering is stable.

## Contract

- `/details` offers search over recorded tool names, commands and inline output, then a readable detail view. Include explicit actions to copy the exact original Bash command or recorded inline output. No command execution is introduced by this flow.
- Clipboard uses the existing explicit OSC52 copy path with base64 payload encoding and its 1 MB cap; previews apply terminal-control filtering. Preserve original records and exact copied bytes. Explain inline versus spilled output; do not pretend a partial recorded preview is the entire spill file. Do not automatically load unbounded files.
- `/keys` appears in the command palette and help. Present actual key bindings grouped by draft, active turn, and viewer. Explain Ctrl-G and Ctrl-T during work and close behavior, draft preservation, Ctrl-C cancellation, modal navigation, and existing Ctrl-P/external-editor bindings as implemented. No invented shortcuts.
- Phone layouts wrap descriptions and keep actions reachable at 20x8. Retain default motion, themes, NO_COLOR and headless behavior.

## Layout

```text
 Tool details
 Search: cargo
 > bash · exit 0 · cargo check...
   bash · exit 1 · cargo test...

 Command / Output
 [Read] [Copy command] [Copy recorded output]
 Esc back
```

## Verification

PTY search with multiple tools, exact command/output clipboard extraction, injected controls filtered in rendering and safely encoded when explicitly copied, no execution, empty results, modal cancellation, all phone sizes; accurate key help and seven existing suites. Independent review and installed binary gate. No publication.

## Current gate

Search, explicit exact command/output copying, `/keys`, and the managed palette are implemented. Dedicated navigation and independent navigation PTY tests passed at all five sizes before the environment restriction; offline startup, palette cancellation/draft restoration, keys scrolling and shell restoration also passed. Final full regression verification is blocked by denied localhost listeners, and default installation is blocked by filesystem restrictions. See ../reviews/tool-navigation-001-01.md. A private release installation exists at `/tmp/gc-final-install/bin/gc`; the user's existing PATH executable is unchanged.
