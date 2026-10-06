---
id: issue60-p1-daemon-lifecycle-001
scope: glass-dev/daemon-lifecycle-and-transport
status: complete
depends-on: [issue60-p0-daemon-result-race-001]
---

# Issue #60 P1: recover daemon lifecycle and transport failures

## Objective

Resolve daemon findings #52, #53, #54, #56, and #59: operation lock poisoning
must not strand records, startup timeout must not leak the child, a worker
panic must not permanently lose its workspace actor, close must wait for
shutdown confirmation, and an oversized result must not end the client stream.

## Context

- `crates/glass-dev/src/daemon.rs`
- `docs/daemon.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), P1 daemon lifecycle
  and transport findings
- [Source report #52](https://github.com/wanazhar/glass/issues/52)
- [Source report #53](https://github.com/wanazhar/glass/issues/53)
- [Source report #54](https://github.com/wanazhar/glass/issues/54)
- [Source report #56](https://github.com/wanazhar/glass/issues/56)
- [Source report #59](https://github.com/wanazhar/glass/issues/59)

## Contract

- When the operation registry mutex is poisoned, all queued or running records
  become `indeterminate` under the recovered guard; future registry calls can
  inspect and mutate the registry normally.
- Startup accepts readiness only from its own child process. An early child
  exit is reported, and a readiness timeout terminates and reaps the child.
- A worker panic marks its operation `indeterminate`, rebuilds the workspace
  from its durable root and trust store, restores resident broker settings,
  and leaves the actor available for subsequent calls. Direct tool worker
  panics also rebuild the actor before it accepts more commands.
- A workspace remains in the registry until its actor acknowledges shutdown
  after dropping the owned workspace. Concurrent close requests are
  serialized, and an unconfirmed shutdown leaves the registry entry available
  for retry or reconciliation.
- A response over 1 MiB is replaced with a bounded error tied to its valid
  request ID; the stream continues to the next request.

## Path

- Daemon operation registry locking and workspace actor completion in
  `crates/glass-dev/src/daemon.rs`
- Unix and Windows daemon startup paths and stream response encoding
- Daemon lifecycle and recovery contracts in `docs/daemon.md`

## Verification

- Poison a registry while an operation is running and verify terminal
  indeterminate state, event publication, and successful subsequent locking.
- Use a child that never publishes status and verify startup timeout kills and
  reaps it.
- Inject an operation worker panic and verify the operation is indeterminate
  while a subsequent workspace tool succeeds.
- Delay and drop workspace shutdown acknowledgements to verify registry
  retention, confirmed removal, and duplicate-close serialization.
- Send an oversized list response followed by `ping` on one stream; verify a
  bounded request-scoped error followed by the successful ping response.

## Verification evidence

Passed on Linux:

- `cargo test -p glass-dev --lib --locked daemon::tests:: -- --nocapture` —
  18 passed, 0 failed, 411 filtered.
- `cargo check -p glass-dev --lib --bins --locked` — passed.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- Independent review — PASS; no blockers against the five contracts.

The Windows startup branch was not executed locally. A Windows target check was
attempted, but stopped in `ring` before compiling `glass-dev` because this Linux
host has no MSVC cross compiler (`cc` rejected the MSVC target flags). The
timeout cleanup test is platform-gated for Unix and Windows and ran on Linux.
