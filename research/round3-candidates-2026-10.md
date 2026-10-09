# Round 3 port candidates — October 2026

Round 3 of the harness study. Round 1 produced
[harness-delta-2026-10.md](./harness-delta-2026-10.md); round 2 shipped the pi
durability set ([pi-durability-2026-10.md](./pi-durability-2026-10.md)). The
delta doc's PART 3 skip-list is fully mined, so this round took candidates from
three other pools: delta-doc items marked "later", pi features round 2
declined, and Claude Code's 2.1.x changelog. Every candidate was checked
against glass-core source before being taken; the four-line summary of what
was verified:

| Candidate | Source | Verified gap | Decision |
| --- | --- | --- | --- |
| Subagent result envelope | Claude Code changelog ("subagent output framed so it cannot pass as session instructions") | Reports land as plain tool-result bodies; framing is only the extension's own `## Task i` header (`extensions/subagent/subagent.py`) | Shipped: host wraps each report in `<subagent-report>` |
| Byte-stable tool list | Claude Code 2.1.268 | The list is rebuilt per turn (`agent.rs:tool_set`) and extension changes mutate it | Shipped: `Registry::declared_tools` freezes it; changes apply next session |
| Compaction system-prompt checkpoint | pi session format (`CompactionEntry.systemMessage`) | `Record::Compacted` stored no prompt; replay re-derived it from current config | Shipped: automatic compaction captures it; replay prefers it |
| MCP determinism + `ttlMs` | MCP 2026-07-28 CacheableResult | Bridge ignored `ttlMs`/`cacheScope` and emitted manifest tools in insertion order | Shipped: sorted manifest, TTL refresh on the deferred index |
| OSC 777 notifications | Copilot CLI (2026-10) | Only OSC 9 and the bell | Shipped: `notify = "osc777"`, auto on Ghostty/WezTerm |
| Session naming | pi `/name` + `SessionInfoEntry` | Rename existed only inside the `/sessions` browser | Shipped: `/name`, `gc list` title column |
| Serve lifecycle | Claude Code (`attach|logs|stop|respawn`) | A served session could only be stopped by SIGINT | Shipped: `gc stop/respawn/logs` |
| Extension durable entries | pi `CustomEntry` | `Record::State` is model-owned; extensions had no non-context durable store | Shipped: `Record::Custom` + `append_custom`/`read_custom`/seed |
| Skill accounting | Claude Code `/skill-doctor` | Nothing reported what skills cost | Shipped: `/skill-doctor` (size and age; usage is not tracked) |
| Diff review | Claude Code `/code-review` | `/review` summarises the task; nothing reviews the diff for bugs | Shipped: `/code-review`, read-only subagent |

## Deliberately skipped

- **Session-tree branching** (pi): medium-large; glass-core forks whole files
  and the tree would change every record. Revisit only if branch-in-place is
  actually requested.
- **Virtual models**: the fallback chain covers the failover case; a router
  layer would duplicate it.
- **tool_call/tool_result argument rewriting** (pi): `authorize` already
  refuses calls and `rewrite_tool_result` rewrites results; argument rewriting
  is the one missing hook, and it is a security-sensitive surface with no
  consumer yet.
- **MCP CIMD OAuth**: large; pre-registered clients cover the transport.
- **Voice / computer use**: the round-1 skip-list stands.

## Notes on the trade-offs taken

- **The freeze is real.** Enabling or disabling an extension mid-session no
  longer changes the declared tool list: the tools arrive next session instead.
  That is the price of a byte-stable cacheable prefix, and the transcript gets
  a meta note stating exactly what changed and when it applies. Calls to a
  since-removed tool fail visibly.
- **The checkpoint wins on replay.** A session resumed after its prompt config
  changed replays the compacted region under the prompt that shaped it, until
  the next compaction re-captures the current one. This mirrors pi; `/context`
  reports the checkpoint size so the behaviour is discoverable.
- **Usage is not tracked.** `/skill-doctor` reports size and age honestly
  instead of inventing a last-used column: the model reads skill files with
  `bash`, and glass does not instrument those reads.
