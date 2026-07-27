---
id: workflow-019-036
scope: typed workflow client contracts
status: completed
depends-on: [workflow-019-035]
---

# Objective

Make the TypeScript and Python workflow helpers describe the same version-one
contract as the Rust runtime.

# Delivered

The clients now expose typed aliases for inputs, budgets, steps, transactions,
outputs, checkpoints, and workflow results. They still pass the declaration
through MCP unchanged; Glass remains the single validator and executor.
