---
id: ship-reliability-001
status: in-progress
scope: ship-next-20 items 1-6
---

Implement contracts 1-6 in ../ship-next-20.md. Own session.rs, agent.rs,
partial.rs, serve.rs, ext/host.rs, ext/mod.rs and required integration call sites.
Add deterministic error/crash/lock/restart regressions and smoke-reliability.py.
Do not edit README, docs/INDEX or unrelated styling. Preserve default behavior
except refusing effects when intent cannot be recorded. All new capabilities
must be integrated across interactive, headless, serve and ACP where applicable.
