---
id: authoring-025-004
scope: workflow parameter and safety inference
status: completed
depends-on: [authoring-025-003]
---

# Objective

Keep parameter values outside workflow source and make static safety findings
deterministic before execution.

# Delivered

- Sensitive-looking names are inferred as sensitive unless explicitly denied.
- Undefined and malformed input placeholders are reported statically.
- Literal values in type/select actions and value-bearing intent steps are
  rejected; callers provide values at execution time.
- Fragile CSS, ordinal, coordinate, and revision-reference locators produce
  stable review diagnostics.
- Recorder drafts can produce value-free input declarations.
