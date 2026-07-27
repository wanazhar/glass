---
id: authoring-025-003
scope: semantic workflow recorder evidence
status: completed
depends-on: [authoring-025-002]
---

# Objective

Keep semantic recording reviewable and non-authorizing while preserving enough
bounded evidence to compile a workflow intent step.

# Delivered

- Recorder drafts retain intent, resolution state, confidence, ambiguity,
  revision, semantic region kind, postcondition, transaction class, and route
  context.
- Current target and frame handles are hashed; route query strings and
  fragments are removed; target fingerprints are digest-only.
- Typed values are represented by input placeholders, never retained as
  literals.
- `glass workflow record` imports an explicit versioned semantic-event
  envelope from stdin or `--input` and writes a reviewable draft.
- Ambiguous or unselected results never become replay targets.
