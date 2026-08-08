# Mobile cockpit delivery analysis

The requested post-issue-32 work is one serial integration chain because MCP,
the TUI, client fixtures, and development exports share central dispatch files.

1. `development/cockpit.rs` owns resident workspace identity, capacity, idle
   expiry, capsules, attention items, and verification cards.
2. MCP creates one registry per server lifetime and routes stateful `project.*`
   operations through it. Browser-free persisted reads stay outside the lock.
3. The TUI projects inbox, semantic tap, card, capsule, and adaptive-live state
   into the existing responsive phone layout.
4. TypeScript and Python compose the MCP primitives into cancellable workflows.
5. Real MCP smoke tests and TUI reducer tests verify the complete call chain.

The implementation order is runtime, MCP, TUI, SDK, then release validation.
