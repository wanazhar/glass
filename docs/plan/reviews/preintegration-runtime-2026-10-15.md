# Pre-integration runtime delivery

Date: 2026-10-15. Scope: GC as an independently hosted agent runtime.

## Delivered

- Stable structured journal entries, incremental byte cursors, bounded field
  reads, session names, statistics, fork lineage, and resume by session ID.
- Persisted execution ownership and effective settings. A host cannot silently
  resume a session that executes GC's local tools.
- Typed context at user-message priority, validated inline images, and structured
  host tool results. Images follow all results in a tool batch.
- Credential readiness, explicit cached model discovery, validated settings,
  and provider-dependent reasoning hints.
- Transient host tool progress, cancellation acknowledgement, durable unknown
  execution status, explicit reconciliation, and categorized protocol errors.
- Typed Rust and TypeScript clients, a Python reference host, and an isolated,
  cancellable prefix/suffix completion lane.
- External conformance checks and subprocess client tests in CI.

Glass's source and integration remain unchanged. Hosts retain tool authorization,
side-effect ownership, approval UI, question UI, and result validation. Runtime
v1 remains additive; ACP is a separate adapter.

## Verification

The final release binary passed the external conformance suite and existing
runtime smoke suite. Coverage includes typed context priority, image/result
ordering, stable entries, incremental reads, usage reporting, resume ownership,
model catalog caching, completion isolation and cancellation, acknowledged and
unacknowledged tool cancellation, reconciliation, and progress flooding. Empty
progress is ignored; a tool accepts at most 1024 updates and emits one overflow
marker.

945 Rust tests passed. The actual-process Rust SDK test and TypeScript SDK test
passed separately. Clippy passed with warnings denied. Release PTYs covered five
screen sizes down to 20 by 8, streaming, scrolling, cancellation, drafts, and
resize. Tooling checks covered 15 CLI edge cases and three PTY sizes. ACP's
existing protocol fixture passed.

One GPT-6 Luna medium reviewer examined the sources. Its progress-event flooding
finding was fixed and covered by conformance. The reviewer checked the fix and
the final focused diff, reporting no remaining blocker.

## Live external host trial

The selected account was sw-kilo, using kilo-auto/free through the local SW proxy.
The trial invoked the actual release executable over stdio; it imported no GC
engine code. All files and runtime sessions belonged to a temporary fixture.

The final coding run completed in 17.41 seconds. It read two Python files, asked
for overflow behavior, accepted a custom answer, wrote against the exact read
revision, and ran a fixed unit test covering five boundary cases. Five transient
progress events reached the host. The host independently reran the test, drained
130 journal entries through pagination, named the session, closed it, and resumed
it by ID.

Live steering interrupted a pending host question and returned the requested
replacement answer. Live cancellation received the host's cancelled
acknowledgement and left no unresolved operation. Credential readiness and model
discovery worked. Isolated completion returned an expression in 4.39 seconds,
using 59 reported input tokens, without changing conversation entries or stats.

Earlier probes exposed a host-fixture pagination assumption, which was corrected,
and nondeterministic model behavior. One model run stopped before fixing the
fixture. Completion responses included leading blank lines; an indented Python
body insertion failed syntax validation, while an expression insertion passed.
GC preserves model output rather than silently modifying source. Hosts must
validate generated insertions before applying them. The successful trial does
not establish model reliability or vision support; image transport and ordering
were verified offline.

Local evidence: /tmp/gc-preintegration-*.log and the final performance JSON.
Live tests require explicit --live and are excluded from CI.

## Performance and operating limits

The binary is 6,493,584 bytes, up 131,112 bytes (2.1%) from the previous host
runtime build. No dependency was added to the GC executable. The TypeScript
client has no runtime package dependencies. SDK dependencies do not enter GC.

All final local performance budgets passed. Worst screen-size medians were
6.13 ms to the interactive prompt, 0.120 ms to display a key, 0.291 ms to redraw,
and 114.50 ms to display a reader frame with 12,000 records. Peak reader RSS was
4128 KiB. These figures exclude provider latency and are machine-specific.

Initialization and readiness do not call a model. Catalog reads are explicit and
cached. Progress is neither persisted nor sent to the model. Host sessions do
not implicitly load project instructions, skills, memory, or extensions.
Completion uses a small fresh request with no tools or conversation history.
Incremental reads seek directly to their cursor; previews and output queues are
bounded. Full legacy inspection and statistics may still scan the journal;
statistics are cached until the journal grows.

One runtime process executes one turn at a time. Use separate processes for
parallel agents. Context-only rewind does not undo filesystem changes. Unknown
host effects require reconciliation before the host authorizes more mutations.
Reported usage is explicitly incomplete when a provider omits accounting.

## Next integration boundary

Glass Dev can now implement its adapter in the Glass repository. Its own
permissions, browser leases, revisions, checkpoints, and mutation authority must
remain enforced by that host. Production adoption still needs Glass-specific
end-to-end acceptance tests. This delivery does not claim Pi SDK wire
compatibility or complete that integration.
