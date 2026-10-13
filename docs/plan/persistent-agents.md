# Persistent teammates

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

Implementation is followed by independent review. The change adds no default model tools or dependencies.

## Contract

Each owning main session has an authenticated local control socket in a private
session directory, a bounded roster (32 agents), and append-only interactions.
Agents have stable IDs, their own durable sessions and compactable context.
Idle agents hold no provider request or worker thread. At most two active agents
per owner; further launches fail explicitly. Reviewers default to read-only.
Workers cannot recursively delegate or control the parent's agents.

`gc agent --help` exposes create/list/send/inspect/wait/transcript/interactions/
interrupt/resume/archive. Shells receive GC_AGENT_OWNER; explicit --session is
available to humans. Main receives bounded completion/intervention notifications
at request boundaries. Full transcripts never enter parent context implicitly.
Human messages and main messages are attributed separately. Interrupt preserves
history; resume needs an explicit message. Shutdown parks agents, never silently
replays a possibly completed tool. Cross-process restart restores parked records.

Tokens are provider-reported, not inferred. Each agent has a cumulative token
budget checked before each request (one in-flight request may overshoot), bounded
output and tool rounds. Shared parent dollar ledger remains authoritative.
Unknown price remains unavailable. Working context uses existing compaction;
full audit logs remain intact. No periodic model polling or automatic review loops.

## UI

```
conversation
◐ review · checking parser · /agents
─────────────────────────────────
› composer

Agents | Interactions | Transcript
› review · working · 1.2k tokens
  main → review  Check parser races
  review → main Found one issue
```

/agents opens a live roster; Enter inspects; Tab cycles views; m messages;
x interrupts; r resumes; a archives. Narrow screens use one view, never split.
Esc closes and restores the managed surface. Long text wraps. Transcript tails
are bounded and read by file offset; unchanged data is cached. Animation is
bounded to 8Hz during active work; idle UI redraws only on changed data/input.
The existing status row is reused for the dock so viewport geometry is stable.

## Verification

Unit contracts, deterministic provider integration, actual release PTYs at
80x24, 40x12, 20x8; live messages, interruption, resume, restart, permissions,
concurrency/budget limits, headless and non-Git use. Existing scrolling and
palette regressions plus performance budget gate. Independent review before commit.

## Implemented interfaces and bounds

- `gc agent create NAME TASK [--model ID] [--tokens N] [--write] [--worktree]`
- `list`, `inspect ID`, `send ID MESSAGE`, `wait ID [SECONDS]` (at most 60s)
- `interrupt ID`, `resume ID MESSAGE`, `archive ID`, `budget ID TOKENS`
- `discard ID` explicitly clears undelivered/uncertain mailbox entries; audit remains.
- `transcript ID [--before BYTE]`, `interactions [--before BYTE]` return bounded
  pages with `before`, `end`, and `has_more`. PageUp at the beginning loads older
  history; End restores live following. Oversized JSONL records appear as
  fragments instead of being silently omitted. Human and main messages are
  separately attributed. `/agents` and Alt+A work while main is idle or busy.

Messages are at most 8 KiB; eight unacknowledged messages per agent. The pending
mailbox is persisted in metadata and remains inspectable after restart. Restart
parks running agents and does not replay uncertain messages automatically; send
an explicit new instruction or discard the pending mailbox after review.

Known usage is provider-reported. Requests without final usage retain a separate
conservative reservation based on estimated input plus bounded generation, and
show usage/cost unavailable. Reservations are not reported token counts. Immediate
request failures release their reservation so retries do not consume phantom
budget. A request already in flight can exceed the remaining budget; steering
may briefly overlap two requests. Dollar spend uses the shared session ledger,
with per-agent prices shown only when a rate card and usage are known.

Interruption stops tool execution and preserves context promptly. The blocking
HTTP library cannot retract a request already accepted by a provider: network-only
streams may drain until their existing transport deadline. They cannot execute
tools, are marked as transport active, and cannot retain session ownership or
write stale agent metadata after shutdown. Outstanding teammate transports are
bounded to two per agent, four per hub, and eight per process. Tool-capable worker
threads are joined before the owner hub closes. Idle agents hold no worker thread.

The hub is currently available in the native interactive and `run` paths on Unix;
ACP/serve continue using their existing delegation interfaces. Workers inherit
the selected connection/model unless `--model` is supplied, and are read-only by
default. No automatic provider/model escalation or recursive delegation occurs.

Offline `list`, `inspect`, `transcript`, and `interactions` work without a running
owner. Mutations require the owner socket. Agent models are pinned when the owner
opens (or explicitly selected with `--model`), so changing the main model cannot
silently change a worker's model. Exact per-agent pricing comes from the metered
provider itself, including provider-specific ChatGPT/unknown-price rules.
Unchanged transcript/idle ticks skip snapshotting and page work entirely; active
roster animation is capped at 8Hz. Changed pages are still bounded to 48 KiB.
