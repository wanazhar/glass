---
id: visual-polish-001
scope: terminal theme, activity motion, composer and modal presentation
status: done
depends-on: []
---
# Visual polish

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Implement the user's request for more colors, animations and a prettier interface, following ../visual-polish.md. Preserve the delivered readability and security contracts.

## Paths

Theme, Markdown, editor and palette helpers in crates/glass-tui; Painter/event pump, modal and theme/profile integration in crates/glass; PTY scripts, README and CI.

## Integration

Event pump -> Painter activity tick -> bounded owned-row output. Theme semantics -> Markdown/composer/modals. Default active motion -> both main and modal animation. The user explicitly declined reduced-motion controls. Original session records -> exact copies/headless outputs remain independent of presentation.

## Verification

Workspace tests, Clippy, all existing PTY suites, dedicated color/motion PTY and independent review. Root installs only after source stable; verifier checks installed binary. No commits or remote publication.
