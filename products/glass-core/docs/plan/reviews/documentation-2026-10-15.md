# Documentation audit — 2026-10-15

## Scope

Three GPT-6 Luna agents at medium reasoning audited the user guides, native tools,
extensions, SDK, documentation structure, and historical record routing. The
primary agent edited the documentation. One separate Luna reviewer checked the
result against the source and reviewed the corrections.

## Changes

The current guides now explain the unrestricted default, restricted launch mode,
platform-dependent network enforcement, custom question answers, model guidance,
and the distinction between one-call approvals and manual session grants. Terminal
documentation describes the complete animation list, exit confirmation, nested
menu navigation, and cancellation from an active reasoning reader.

The README retains the product overview and links to detailed contracts. Repeated
terminal instructions were removed from configuration documentation. Current
references and recent review records are indexed in their corresponding sections.
The architecture overview describes stable subsystems. SDK prose uses consistent
project naming. Dated research and verification records retain their original
scope and evidence.

## Verification

Local Markdown targets and heading fragments passed across the tracked Markdown
inventory, including this record. `git diff --check` passed. The final reviewer
reported no blocking findings after two inaccurate statements were corrected:
unrestricted execution bypasses approval dialogs, and Ctrl+C cancels an active
turn even when the reasoning reader is open.

This change affects documentation only. No application build, provider request,
or runtime test was needed for these edits.
