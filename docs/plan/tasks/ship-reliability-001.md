---
id: ship-reliability-001
status: completed
scope: ship-next-20 items 1-6
---

Implement contracts 1-6 in ../ship-next-20.md. Own session.rs, agent.rs,
partial.rs, serve.rs, ext/host.rs, ext/mod.rs and required integration call sites.
Add deterministic error/crash/lock/restart regressions and smoke-reliability.py.
Do not edit README, docs/INDEX or unrelated styling. Preserve default behavior
except refusing effects when intent cannot be recorded. All new capabilities
must be integrated across interactive, headless, serve and ACP where applicable.

Delivery notes: raw history is indexed in memory with file identity/size/time
invalidation; read-only snapshots never repair or take the writer lock. Checked
append failures roll back and poison the session, and agent turns refuse before
provider work. Tool intents, accepted serve requests, user prompts, and extension
custom entries use checked appends. Partial snapshots use private atomic replace.
Serve duplicates persist across restart; an accepted request without completion
is reported interrupted and is never replayed.

Serve request IDs are retained for the session lifetime so a restart cannot
silently make an old identifier executable again.
