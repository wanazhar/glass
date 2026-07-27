---
id: semantic-020-007
scope: TUI semantic explorer
status: completed
depends-on: [semantic-020-006]
---

# Objective

Make semantic observations discoverable from the terminal interface without
changing the existing compact observation command.

# Delivered

The TUI accepts `semantic`, `semantic LEVEL`, and `semantic LEVEL REGION_ID`
commands for summary, interactive, structured, detailed, raw, and scoped
observations. The observation pane renders the same bounded JSON contract and
the header follows the semantic page identity.
