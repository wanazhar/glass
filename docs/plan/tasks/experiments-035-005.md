# experiments-035-005 — automatic measured evidence

Status: Complete locally on 2026-08-11. No remote mutation performed.

Experiments now own resident child workspaces and collect build/test, Git,
process, LSP, DAP, workflow, and semantic evidence where providers are
available. Missing visual/performance/browser providers are explicit
unavailable records. Each metric carries producer, time, revisions, run ID,
measured/manual status, availability, and details.

Manual evidence is forcibly labeled manual. Ranking remains deterministic;
bounded custom weights require trusted workspace authority. Router and TUI
operations expose create, collect, compare, select, list, and cleanup.

The real Git-worktree test creates three competing implementations, assigns a
native Pi worker to each when Pi is installed, executes configured Rust
toolchain build/test probes for all three, collects measured provenance and
different changed-file counts, ranks only that automatic evidence, verifies
unavailable browser truth, then proves later manual evidence cannot claim
measured provenance.
