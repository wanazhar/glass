# Native tools and background jobs

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Compact native tools

`read`, `edit`, and `write` expose the bounded native file engine directly.
Reads return line numbers and revisions; the agent retains native revisions for
changes and supports previews and syntax checks. `bash` handles search, pipelines, builds, tests and
processes. `state` preserves task notes, manages the native execution checklist,
and discovers deeper workflows on demand. See the [checklist reference](reference/checklist.md)
for batched operations, stable IDs, blockers and durable receipts. `/todo` opens
the shared task modal; no extension is required.

`web_search` uses Exa's hosted MCP search, and `web_fetch` reads an HTTP(S) page
as bounded text. Both are native Core tools; their configuration, source
handling and limits are in [Web search and fetch](web-tools.md).

The CLI and tool interfaces share implementation, confinement and file semantics.
Schemas stay compact; performance checks cover startup, input, redraw and history.
Interactive agents also receive `ask_user_question` for decisions that require
user input. It supports choices, multiple selections, and custom written answers.
See [Approvals and user questions](reference/interaction.md) for availability and
turn-pausing behavior.

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

Foreground shell results retain 16 KiB inline per stream and up to 4 MiB of
overflow. Excess output is drained, with discarded bytes and a final tail
reported explicitly. Completed overflow is stored once in content-addressed
blobs. See [Runtime storage](reference/storage.md) for capture and retention
bounds. Explicit file redirection is required for complete larger logs.

## Native file tools

Prefer `read` for file inspection, `edit` for surgical changes, and `write` for new
files or deliberate replacement. Existing-file changes require a previous read. Model calls may omit `expected`:
GC supplies the last successful native read/write revision retained by the active
agent, never a fresh revision sampled from disk. External changes still produce
a stale-revision error and require another read. Explicit `expected` values take
precedence; `expected:null` is always create-only. Without a retained revision,
an omitted revision on `write` is create-only and `edit` asks for a native read.
The standalone CLI has no agent revision state and still requires `expected`.
Create-only writes create missing parent directories without following symlinks;
previews never create directories. `preview:true`
shows a proposed diff without writing; `validate:true` requires a passing supported
syntax check. Reads are bounded and return continuation instructions when truncated. Native
results provide one `next_read` argument object; CLI text provides a command hint
instead. JSON results retain the complete continuation metadata.
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
BOM and final-newline behavior. Exact replacements first match supplied bytes;
if no exact match exists, LF/CRLF differences are tolerated with uniqueness still
required and only the replacement region converted to the file's line endings.
Unrelated bytes remain unchanged. Other whitespace and punctuation stay exact.
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

`bash` executes `/bin/bash` with `-c` in foreground and background paths, including
Bash arrays, `[[ … ]]`, and process substitution. Bash accepts a `job` object. Each line below is a separate tool call:

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
After stop or shell exit, capture drains for at most 100 ms before closing its pipes,
so a detached writer cannot keep the UI waiting indefinitely.
Execution policy applies to all operations. In confined interactive sessions,
models can request a one-call exception on `bash`, `edit`, or `write`; the
requested operation waits for the human decision. See [Approvals and user questions](reference/interaction.md).

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
prints the last 60 KB of output. `/job <id> stop` stops the command's process group.
Live jobs are stopped on normal terminal exit. Logs stay under session data in
`jobs/<session>/` for inspection. Jobs belong to the running terminal process and
are not reattached on resume.

For foreground bash calls, `cwd` selects the directory used by execution, read-before-write checks, and verification. Failed reads do not authorize later writes. Job control rejects `cwd` rather than silently ignoring it; use explicit paths in background commands.

## Image sharing and viewing

`display_image {"path":"screenshots/result.png"}` shares a local image in web
chat without sending pixels to the model. It returns dimensions, file size and
Markdown; the browser loads the file separately. The model cannot infer its
contents from this metadata. PNG, JPEG, GIF and WebP files up to 8 MiB and
32 million pixels are supported, using the same file permission boundaries as
`view_image`. Header validation does not decode or re-encode pixels or create
vision artifacts. `gc file display PATH` provides the same metadata locally.

For a local preview and artifact metadata, run `gc file image PATH` or
`gc file --json image PATH`. This prepares pixels without a model request.

`view_image {"path":".gc-artifacts/browser-123/viewport.png"}` supplies image
pixels to the model. It shares the native file engine's workspace boundary,
regular-file requirement, symlink refusal and worker dispatch. It supports PNG,
JPEG, GIF and WebP; animated formats use the first frame. The same decoder and
size limits apply to user attachments. Transparency is composited over white.

The tool card shows the source path, original and transmitted dimensions, and
encoded size. Base64 is never printed in tool text. A content hash identifies a
private JPEG artifact under the GC data directory's `images` folder. Replay adds
pixels after the complete tool-result batch, preserving call/result ordering in
Chat Completions, Responses and Anthropic transports. Missing or corrupt artifacts
produce an explicit message asking the model to call `view_image` again.

Preparation and encoded-image caches each retain at most eight entries. Decoding
runs only on demand, outside the terminal input thread for model tool calls; there
is no startup scan or background image processing. Durable artifacts remain until
the data directory is explicitly cleaned. Replayed image inputs still consume
provider context and may incur charges; local cache hits do not make vision free.

A compatible API does not prove model vision support. Set `image_input = false`
in the main configuration for a text-only model: foreground, served and ACP agents
omit `view_image`, reject fabricated viewing calls and replace existing pixels
with an explicit text notice before sending a request. Original images remain
in session storage. `display_image` remains available. An explicit upstream
400 rejection stating that image input is unsupported triggers one retry without
pixels for the current agent instance, preserving completed tools. The default is true so unknown compatible vision models can be
used without an allowlist. Independently configured delegated models must support
images when their tasks require them.

Enable `browser` and use `browser_verify` to capture a page, then pass its returned
screenshot path to `view_image`. Browser capture accepts screenshots up to 8 MiB;
native preparation reduces them to the model input bound. See
[Browser verification](../extensions/browser/README.md).

### Native file scope

The unrestricted permission profile with shell sandboxing off permits native
file tools to use absolute paths or relative paths outside the workspace.
Revision checks, bounded reads, regular-file requirements and symlink refusal
still apply. Workspace-edit profiles permit outside reads while confining
mutations; read-only and strict profiles retain narrower reads and block
unapproved mutations. Standalone `gc file` loads the active profile and
administrator requirements before executing an operation.

Outside-workspace native calls are labelled in live tool activity. Writes to
kernel and device controls under `/proc`, `/sys` and `/dev` are labelled as system
control writes. Bash does not require a document read before such a control
write when unrestricted with sandboxing off, or when the exact command has an
explicit approval. Ordinary file replacement still requires reading the file.
This exception does not bypass shell policy or OS permissions.

## Pi tool comparison

The tool audit compared Pi's [native tool implementation](https://github.com/earendil-works/pi/tree/910149912ca29d7ae4eaa945bb2b53eb84c68264/packages/coding-agent/src/core/tools)
with GC's engine and recent local tool failures. GC now covers the following
common workflows without an extra provider call or dependency:

| Tool | Compatibility and retained GC behavior |
| --- | --- |
| `bash` | Actual Bash syntax in foreground and background jobs. GC retains explicit exit status, cancellation, timeout bounds, and bounded overflow storage. |
| `read` | Relative/absolute paths allowed by the active permissions, numbered UTF-8 text, continuation, and stale continuation detection. GC defaults to 200 lines to limit context growth. Images use `view_image`. |
| `write` | New files in missing parent directories; deliberate whole-file replacement after a native read. The agent carries the observed revision; explicit null remains create-only. |
| `edit` | Unique exact replacements, atomic batches, and LF/CRLF matching without modifying unrelated bytes. GC also offers original-file line ranges, previews, and syntax validation. |

This is workflow compatibility, not identical schemas or unrestricted limits.
GC retains its 8 MiB text-file bound, no-follow paths, revision checks, and smaller
inline output budgets. Use Bash or owned jobs for larger/binary data. GC does not
silently normalize smart quotes, Unicode punctuation, or arbitrary whitespace in
edits; use the actual text or line ranges. Native schemas remain `text` and
`edits:[{old,new}]`, rather than Pi's `content` and `oldText/newText` fields.
CLI callers continue to pass revisions explicitly. These differences are
intentional and remain visible in each tool's advertised schema.

Provider-wire and PTY regression coverage exercises omitted revisions, preview
isolation, nested creation, explicit stale revisions, external writes followed by
stale automatic revision refusal, reread recovery, and read-only permissions.
