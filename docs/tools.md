# Native tools and background jobs

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Compact native tools

`read`, `edit`, and `write` expose the bounded native file engine directly.
Reads return line numbers and revisions; changes require those revisions and support
previews and syntax checks. `bash` handles search, pipelines, builds, tests and
processes. `state` preserves task notes and discovers deeper workflows on demand.

The CLI and tool interfaces share implementation, confinement and file semantics.
Schemas stay compact; performance checks cover startup, input, redraw and history.

## Execution checks

The host validates calls, records results, and applies the configured execution
policy around the model's requests:

- Shell results include exit status. Native operations return structured outcomes.
- Native mutations require the expected revision and support previews and
  supported syntax checks.
- The shell read guard detects supported overwrite patterns and requires a prior
  read. Shell parsing and write detection are best-effort; use native file tools
  when revision checks matter.
- Independent tool calls can run concurrently. Writes with the same keyed path
  are serialized, and results return in request order.
- Eligible transient provider failures retry with bounded backoff. Partial streams
  do not execute incomplete tool calls. Completed tools retain their results for
  recovery.

These checks do not establish that an agent's change is correct. Inspect the diff
and run the project's relevant checks before accepting it.

## Native file tools

Prefer `read` for file inspection, `edit` for surgical changes, and `write` for new
files or deliberate replacement. Existing-file changes require a previous read and
its whole-file `expected` revision. `expected:null` is create-only. `preview:true`
shows a proposed diff without writing; `validate:true` requires a passing supported
syntax check. Reads are bounded and return continuation instructions when truncated.
Native calls keep input responsive and remain ordered with adjacent shell calls.
Planning mode remains inspect-only; read-only permissions block native mutations.

## Native files and owned background jobs

The native `read`, `edit`, and `write` tools share their file engine with
`gc file`. Bash children receive `GC_BIN`, the absolute path of the running
executable, for access to CLI workflows and their help.

```bash
"$GC_BIN" file read src/main.rs 1 120
"$GC_BIN" file --json read src/main.rs
"$GC_BIN" file check settings.json
```

`file read` returns numbered UTF-8 text and a whole-file SHA-256 revision.
Reads show at most 24 KiB and 1,000 lines (200 by default), with explicit partial
read metadata. Files are capped at 8 MiB; binary files and symlink paths are
rejected. Use ordinary shell commands for larger/binary files.

`file edit` and `file write` take JSON on stdin. Editing supports unique exact
replacements or original-file line ranges, so the model need not reproduce an
entire old block. Every operation validates before a batch commits:

```json
{"path":"src/main.rs","expected":"revision from file read","edits":[
  {"start":12,"end":14,"text":"replacement lines"},
  {"old":"unique exact text","new":"replacement"}
]}
```

For insertion use `end=start-1`. Line ranges preserve the file's CRLF convention,
BOM and final-newline behavior; exact replacements use explicit supplied text.
`file write` accepts `path`, `expected`, and `text`; `expected:null` means
create-only. Existing files require the current revision. At most 128 edit
operations and one million indexed lines are accepted; exact edits need no line
index. Unchanged writes avoid replacing the file.

Mutations use a same-directory temporary file, atomic replacement, file syncing,
and cooperative parent-directory locking. Stale revisions, overlapping edits,
symlink traversal, special files and hard-linked targets are refused. Ordinary
rwx bits are retained; special mode bits are dropped. External writers that do
not share the lock can still race the final POSIX rename; this is not a claim
of filesystem-wide compare-and-swap.

Native previews color added/removed lines and syntax status. JSON checks are
in process. `file check` or `validate:true` explicitly runs syntax-only checks
for JSON, Python or JavaScript; Python/Node checks have a three-second deadline.
Unavailable and unsupported checks are reported distinctly. Validation failure
leaves the original unchanged; ordinary Python/JavaScript writes do not spawn
checkers. Use project commands for compiler/type/test diagnostics. `--json`
provides structured file and diagnostic results for headless callers.

Bash accepts a `job` object. Each line below is a separate tool call:

```jsonl
{"command":"npm run dev","job":{"action":"start"}}
{"job":{"action":"poll","id":1,"offset":0}}
{"job":{"action":"input","id":1,"text":"hello\n"}}
{"job":{"action":"input","id":1,"eof":true}}
{"job":{"action":"stop","id":1}}
```

Poll using the returned `next_offset` to avoid replaying old output. Metadata
includes state, exit code, captured/dropped bytes and whether pipes are still
draining. Input reports accepted bytes; retry only the remaining bytes when
stdin applies backpressure. Poll limits may extend by up to three bytes to
finish a UTF-8 character; incomplete live characters stay at the same cursor.
Wait for both `state: exited` and `draining: false` before treating output as
complete. Input is capped at 8 KiB; polling at 24 KiB plus that UTF-8 boundary; each
job log at 4 MiB. Further output is drained and counted, not accumulated.
These are pipe-based jobs, not a terminal emulator; use unbuffered output when
the underlying program needs it.

`/bg`, `/jobs`, and model jobs share the current session's registry. IDs are local
to a session. Switching sessions retains jobs under their original owner; exiting
the application stops owned process groups. Stop sends TERM, allows 250 ms for
shutdown, then sends KILL and reaps the child. No detached persistence is promised;
programs that deliberately escape their process group are outside job ownership.
Shell permissions and explicit command approvals still apply.

## Tool call repair

GC validates completed model tool calls before execution. Malformed JSON,
nonobject arguments, invalid fields and unknown tools produce an instructive
error and a visible `Tool repair 1/2` notice. The model gets up to two correction
requests within the normal turn budget. A batch containing an invalid call runs
no tools; corrected calls still pass permissions and read-before-write guards.
Shell failures remain ordinary tool results; GC never guesses a command or
replays completed actions as part of repair. Incomplete provider streams remain
fatal, and interrupted partial calls never execute.

Raw rejected arguments stay in the local session audit. Provider replay uses
an empty-object placeholder for malformed input, paired with an explanatory
error result. Anthropic receives `is_error` for standard error results.
Validation covers JSON types, required/properties/items, alternatives, enums,
constants, numeric and length bounds. This is not a full JSON Schema engine:
extensions remain responsible for complex constraints such as `$ref`, regex
patterns and semantic validation. Repair adds no default tool or successful-turn
network request. Escape/Ctrl+C and follow-up prompts remain responsive.

## Background commands

```text
/bg cargo run
/jobs
/job 1
/job 1 stop
```

Background commands run in the configured workspace while the agent remains
available. `/jobs` shows IDs, process status, command, and log path; `/job <id>`
prints the last 60KB of output. `/job <id> stop` stops the command's process group.
Live jobs are stopped on normal terminal exit. Logs stay under session data in
`jobs/<session>/` for inspection. Jobs belong to the running terminal process and
are not reattached on resume.
