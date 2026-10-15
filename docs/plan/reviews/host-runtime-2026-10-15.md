# Host runtime delivery

Date: 2026-10-15. Scope: GC's first host-neutral runtime interface.

## Delivered

- Opt-in `gc runtime` over bounded JSON-RPC stdio.
- Capability discovery and explicit session storage.
- Exclusive host tools, with no fallback to GC built-ins or extensions.
- Correlated tool requests and results using actual provider call IDs.
- Host-controlled approval waits and question tools with custom answers.
- Streaming text, exposed reasoning, usage, errors, and turn settlement.
- Steering, queued follow-ups, cancellation, and disconnect handling.
- Session create/list/resume/inspect/configure/close, fork, context-only rewind,
  and manual compaction.
- A reusable Rust subprocess client and a standard-library Python reference host.
- Protocol documentation, CLI help, and CI coverage.

Standalone GC retains its tool schemas, UI, permission policy, and prompt
construction. Hosted instructions and tool ownership are explicit. The Rust
client is a separate crate; GC does not depend on it.

## Verification

The offline runtime fixture uses the actual release binary and a local mock
provider. It checks initialization order, exclusive schemas, real tool IDs,
custom answers reaching the next model request, denial returned as a recoverable
tool error, no local shell execution for a host tool named bash, long escaped
output chunking, queued follow-ups, steering, cancellation, EOF during a tool
wait, oversized input, private manifests, resume/fork/rewind, model changes,
ownership-change refusal, and shutdown.

Release PTY fixtures verify streaming, continued tools, cancellation, retained
drafts, exact scroll cell/style round trips, and resize down to 20×8. Tooling
foundation checks 15 file CLI edges and PTYs. The existing ACP fixture passes.

One GPT-6 Luna medium reviewer checked the implementation. Its oversized-event
finding was fixed with bounded text chunks and explicit failures before dispatch.
The optional isError field is documented as defaulting to false; nonboolean
values are rejected. No remaining reviewer blocker was reported.

942 Rust tests passed; one existing doctest remains ignored. Clippy passed with
warnings denied. A parallel test exposed a writer-lock lifetime race caused by
duplicated descriptors. Sessions now explicitly release their owned lock on
close; a regression verifies reopening while a duplicate descriptor remains
alive, without releasing the new writer when that old descriptor closes. The
reviewer checked this fix and found no blocker.
Local evidence is retained in /tmp/gc-runtime-*.log.

## Performance boundary

The release binary grew from 6,231,400 to 6,362,472 bytes, an increase of 131,072
bytes (2.1%). No new dependency was added to the GC executable. Runtime transport
queues, tool result bodies, transcript pages, and event chunks are bounded.
The runtime creates no files unless explicitly invoked and initialized.
Standalone system instructions and tool schemas are unchanged.

All local timing budgets passed on the final release. Worst terminal-size
medians were 8.44 ms to the interactive prompt, 0.205 ms for a typed key,
0.299 ms for redraw, and 113.80 ms for a reader frame with 12,000 records.
These fixtures exclude provider latency and do not establish performance on
other machines.

## Remaining integration work

This delivery does not switch Glass Dev's resident agents from Pi. Glass Dev
still needs an adapter mapping its authoritative tool router, approvals,
context, worker lifecycle, and session controls to this protocol. Its browser
leases, trust, revisions, and mutation authority must remain enforced by Glass.

Runtime v1 does not advertise FIM completion or image prompts. ACP remains a
separate adapter over the same engine and retains its existing capabilities.
Production Glass adoption requires a separate end-to-end acceptance trial;
mock-provider success does not establish that integration.
