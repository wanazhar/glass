---
id: semantic-020-006
scope: typed TypeScript and Python semantic clients
status: completed
depends-on: [semantic-020-005]
---

# Objective

Keep the TypeScript and Python MCP clients aligned with the Rust semantic
observation contract.

# Delivered

Both clients now expose the five semantic levels, typed bounded observation
models, and optional region expansion helpers while retaining their existing
compact `observe` call behavior.
