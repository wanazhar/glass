# CLI reference
Status: Current 0.3.14 source behavior (including current-source work in this checkout)


Run `glass --help` or `glass COMMAND --help` for the exact syntax for the
installed version.

The checked-in [CLI inventory](cli-inventory.json) is the generated schema
authority used by documentation drift validation. This reference follows the
current source checkout, including development-product routes; live `--help`
remains the authority for installed flags, defaults, and positional arguments.

## Global options

| Option | Default | Function |
|---|---|---|
| `--yolo` | off | Run an unrestricted trusted Pi/Glass session: no tool approvals, all browser policy capabilities allowed, and ambient Pi resources/tools loaded. |
| `--policy development\|ci\|polite\|hardened\|untrusted-mcp` | `development` | Select the browser safety preset. |
| `--policy-allow CAPABILITY` | none | Explicitly allow a privileged capability; repeatable. |
| `--policy-confirm CAPABILITY` | none | Require a typed confirmation result; repeatable. |
| `--policy-confirm-once CAPABILITY` | none | Supply one consumable approval token; repeatable. |
| `--policy-allow-host HOST` | none | Allow an exact host in hardened mode; repeatable. |
| `--policy-deny-host HOST` | none | Deny an exact host in hardened mode; repeatable. |
| `--experimental-extensions` | off | Opt into the separately gated extension loader. |
| `--profile NAME` | `default` | Use a persistent browser profile. |
| `--incognito` | off | Use a disposable browser profile. |
| `--attach` | off | Connect to an existing CDP endpoint. |
| `--session NAME` | none | Attach browser operations to a named persistent local session; resolves its verified loopback port. |
| `--target-id ID` | automatic | Select a page target (required when the endpoint exposes more than one page target). |
| `--frame-id ID` | main frame | Select a frame. |
| `--port PORT` | `9222` | Set the local CDP port. |
| `--headed` | off | Show the Chrome window. |
| `--viewport WIDTHxHEIGHT` | browser default | Set CSS viewport dimensions before navigation. |
| `--interaction human\|fast` | `human` | Select pointer event mode. |
| `--audit` | off | Record bounded high-risk operation metadata. |
| `--trace-on-error` | off | Write one bounded failure trace to stderr. |
| `--chrome-path PATH` | discovered | Select the browser executable. |
| `--knowledge-store PATH` | profile-scoped | Select the knowledge store. |
| `--response-mode minimal\|normal\|diagnostic` | `minimal` | Select the bounded agent-facing result projection. |
| `--mcp` | off | Start the MCP stdio server. |
| `--tui-layout auto\|desktop\|compact\|mobile` | `auto` | Select geometry-only terminal layout. |
| `--tui-transport auto\|local\|remote-fast\|remote-constrained\|mosh\|unknown-remote` | `auto` | Override independently measured transport policy. |
| `--tui-graphics auto\|kitty\|sixel\|i-term-inline\|ansi\|semantic-only` | `auto` | Override graphics capability; auto requires active evidence. |
| `--tui-rtt-ms MS` | unknown | Supply measured round-trip latency. |
| `--tui-throughput-mbps MBPS` | unknown | Supply measured terminal-link throughput. |
| `--tui-live off\|auto\|on` | `off` | Keep continuous pixels off, require an available selected backend, or allow the default ANSI fallback. |
| `--tui-live-backend auto\|herdr\|kitty\|ansi` | `auto` | Select Herdr, Kitty terminal graphics, or bounded ANSI rendering; `auto` prefers Herdr and otherwise falls back to ANSI only for `live on`. |
| `--tui-live-quality data\|balanced\|smooth` | `balanced` | Select the adaptive capture size and target frame rate. |
| `--tui-live-fit contain\|cover\|actual` | `contain` | Select ANSI sampling; Kitty and other native image backends use contain. |
The TUI policy options are independent: layout uses terminal geometry; transport
uses the measured link classification; and graphics uses active protocol
evidence. `--tui-live off` keeps the browser semantic-only. `auto` captures
pixels only when the selected native path is available, while `on` permits the
bounded ANSI fallback. With backend `auto`, Herdr is preferred; an explicit
unavailable `herdr` path remains semantic-only. Explicit `kitty` selects the
Kitty protocol, and explicit `ansi` selects true-color half-block rendering.
Quality profiles target approximately 3 FPS/data, 6 FPS/balanced, and 12
FPS/smooth within bounded capture sizes. `contain` is used by native image
backends; ANSI additionally supports `cover` and `actual`.

`--mcp` is a top-level option (not inherited by subcommands); it starts the
stdio server and reserves stdout for protocol frames.

Place global options before or after the subcommand.
Compatibility spellings are limited to the aliases defined by Clap:
`--chrome` for `--chrome-path` and `--semantic-level` for `observe --level`.
The command names in this reference are canonical; TUI palette words are not
CLI aliases.

Set `GLASS_CONFIG_HOME` to select the configuration and profile root. If it is
not set, Glass uses the operating system configuration directory.

Chrome sandboxing is enabled by default. Set
`GLASS_DISABLE_CHROME_SANDBOX=1` only in an already isolated container or CI
environment.

## Browser commands

The complete browser command inventory is:

```text
navigate URL
click TARGET
preflight TARGET
click-at X Y
click-expect-popup TARGET
double-click TARGET
hover TARGET
drag SOURCE DESTINATION
type TEXT [--target TARGET]
key KEY
key-down KEY
key-up KEY
shortcut SHORTCUT
clear TARGET
check TARGET
uncheck TARGET
select TARGET VALUE
upload TARGET FILE...
fill-form --fields JSON
screenshot
text
dom
observe
observe-delta
inspect-page
find-target
act-and-verify
extract-structured
recover-run
scroll
wait CONDITION
diagnostics
accept-dialog
dismiss-dialog
dismiss-consent
download DIRECTORY
archive-targets [--output FILE]
targets
new-target URL
select-target ID
close-target ID
frames
select-frame ID
evaluate EXPRESSION
cookies
export-cookies FILE
import-cookies FILE
pdf FILE
batch [JSON_FILE|-]
smoke-sites MANIFEST
workflow [JSON_FILE]
workflow-resume WORKFLOW CHECKPOINT
task SUBCOMMAND
ir SUBCOMMAND
verify PREDICATE_JSON
resolve-intent [JSON_FILE]
execute-intent [JSON_FILE]
reconcile-refs INPUT
checkpoint SUBCOMMAND
snapshot SUBCOMMAND
clipboard-read
clipboard-write TEXT
```

Use `glass --help` for options and defaults for each command.

Important defaults:

- `screenshot` writes `screenshot.png`;
- `scroll` uses `dx=0` and `dy=600`;
- `wait` uses a bounded 10-second deadline;
- `upload` accepts 1 to 16 regular files; and
- `diagnostics` and `download` use a maximum 30-second duration.

Glass does not collect deep DOM, screenshots, or form values during a normal
observation. Request those operations explicitly.

## Command families

These top-level families are browser-free unless a row explicitly says
otherwise. Run `glass FAMILY SUBCOMMAND --help` for required files, bounds,
confirmation flags, and output schemas.

| Family | Subcommands |
|---|---|
| `certify` | `run`, `plan`, `release`, `replay`, `replay-diff` |
| `workspace` | `list`, `inspect`, `suspend`, `resume`, `delete` |
| `project` | `inspect`, `files`, `search`, `read`, `edit`, `mkdir`, `rename`, `delete`, `diagnostics`, `run`, `test`, `lint`, `process`, `diff`, `link`, `graph`, `breakpoint`, `timeline`, `replay`, `neovim`, `experiment`, `attach` |
| `agent` | `doctor`, `setup`, `status`, `delegate`, `hello`, `prompt`, `steer`, `follow-up`, `models`, `set-model`, `thinking`, `abort`, `new-session` |
| `harness` | `list`, `start` |
| `memory` | `status`, `inspect`, `explain`, `forget`, `export`, `prune`, `reindex` |
| `browser` | `tui` (or omit the subcommand to launch the browser workspace) |
| `session` | `start`, `status`, `open`, `stop` |
| `surfaces` | `inspect`, `coverage` |
| `backend` | `status`, `capabilities`, `test` |
| `daemon` | `start`, `status`, `stop`, `doctor`, `logs`, `acknowledge-recovery` |
| `replay` | `inspect`, `diff`, `attach` |
| `profiles` | `list`, `create`, `delete` (subcommand optional; no subcommand lists) |
| `knowledge` | `list`, `show`, `explain`, `stats`, `export`, `import`, `invalidate`, `purge` |
| `result` | `show`, `purge` |
| `workflow` | run a workflow, or `compile`, `format`, `preview`, `diff`, `record`, `validate`, `lint`, `templates`, `init` |
| `task` | `validate`, `compile`, `execute` (execution starts/uses a browser) |
| `ir` | `validate`, `inspect`, `diff`, `continuity`, `canonical` |
| `checkpoint` | `export`, `import` |
| `snapshot` | `create` (browser-backed), `list`, `inspect`, `diff`, `purge` |

Nested development inventories are exact:

| Path | Public subcommands |
|---|---|
| `project process` | `list`, `start`, `stop`, `restart`, `remove`, `input`, `resize`, `output` |
| `project graph` | `discover`, `entity`, `source` |
| `project neovim` | `probe`, `start` |
| `project experiment` | `create` |

Project mutations remain confined to the canonical root. Certification and
replay inspect evidence; they do not replay browser input as an unguarded
command stream. Hidden implementation commands are not public CLI contracts.

### Exact nested syntax

The following forms are the public Clap routes. `FILE`, `PATH`, `ID`, and
`NAME` are positional values unless shown as options. Run the corresponding
`--help` for the full inherited global-option list.

| Route | Syntax and defaults |
|---|---|
| `certify` | `run --scenario FILE --fixture FILE --url URL [--workflow-root DIR] [--inputs FILE] [-o FILE]`; `plan --scenario FILE --fixture FILE`; `release --version VERSION --scenarios FILE --observations FILE [--replays FILE]`; `replay --scenario FILE --input FILE`; `replay-diff --scenario FILE --before FILE --after FILE` |
| `workspace` | `list`; `inspect ID`; `suspend ID`; `resume ID`; `delete ID` |
| `project` | `inspect|files [--root .]`; `search QUERY [--limit 64] [--root .]`; `read PATH [--root .]`; `edit PATH (--content TEXT \| --input FILE) [--root .]`; `mkdir PATH [--root .]`; `rename FROM TO [--root .]`; `delete PATH [--yes] [--root .]`; `diagnostics PATH [--root .]`; `run NAME [--command COMMAND] [--wait] [--root .]`; `test|lint [--root .]`; `process [--root .] SUBCOMMAND`; `diff|timeline [--root .]`; `link ENTITY PATH --start-line N --end-line N [--provenance explicit-marker] [--confidence 1.0] [--detail TEXT] [--root .]`; `graph [--root .] SUBCOMMAND`; `breakpoint KIND ENTITY BEFORE AFTER [--root .]`; `replay [--start 0] [--limit 64] [--root .]`; `neovim [--root .] SUBCOMMAND`; `experiment [--root .] create NAME --port PORT`; `attach ACTOR [--root .]` |
| `project process` | `list`; `start NAME COMMAND [--wait]`; `stop|restart|remove NAME`; `input NAME INPUT`; `resize NAME COLS ROWS`; `output NAME` |
| `project graph` | `discover`; `entity ENTITY`; `source PATH [--line N]` |
| `replay` | `inspect SCENARIO INPUT`; `diff SCENARIO BEFORE AFTER`; `attach SCENARIO INPUT` |
| `agent` | `doctor`; `setup [--sdk-entry FILE] [--agent-dir DIR] [--update] [--login]` (`--agent-dir` requires `--sdk-entry`); `status`; `delegate HARNESS PROMPT [--root .] [--sandbox read-only\|workspace-write] [--timeout-secs 600] [--allow-mutation] [--yes]`; `hello [--root .] [--harness local\|pi]`; `prompt TEXT [--root .] [--harness local\|pi]`; `steer TEXT [--root .] [--harness pi]`; `follow-up TEXT [--root .]`; `models [--root .]`; `set-model PROVIDER MODEL_ID [--root .]`; `thinking LEVEL [--root .]`; `abort|new-session [--root .]` |
| `harness` | `list`; `start NAME [--root .]` |
| `memory` | `status`; `inspect|explain|forget RECORD_ID`; `export [PATH]`; `prune`; `reindex` |
| `surfaces` | `inspect|coverage FILE` |
| `backend` | `status|capabilities|test FILE` |
| `profiles` | `list`; `create NAME`; `delete NAME`; omitting the subcommand lists profiles |
| `knowledge` | `list`; `show|explain RECORD_ID`; `stats`; `export [PATH]`; `import FILE`; `invalidate RECORD_ID stale\|contradicted\|quarantined [--reason TEXT] [--observed-at RFC3339]`; `purge ORIGIN` |
| `result` | `show RESULT_ID [--section NAME]`; `purge --older-than AGE` (for example `7d` or `24h`) |
| `workflow` | `FILE` executes; `compile|format FILE [-o FILE]`; `preview|validate FILE`; `diff BEFORE AFTER`; `record [--input FILE] [-o FILE]`; `lint FILE [--warnings-as-errors]`; `templates [NAME] [-o FILE]`; `init NAME [-o FILE]` |
| `task` | `validate FILE`; `compile FILE IR [-o FILE] [--explain]`; `execute FILE --expected-revision N [--confirm]` |
| `ir` | `validate|inspect|canonical FILE`; `diff BEFORE AFTER [--summary]`; `continuity BEFORE AFTER ENTITY_ID` |
| `daemon` | `start|status|stop|doctor [--socket PATH] [--status PATH]`; `logs [--status PATH]`; `acknowledge-recovery --request-id ID... [--status PATH]` |
| `checkpoint` | `export`; `import [FILE]` (stdin when omitted) |
| `snapshot` | `create`; `list`; `inspect SNAPSHOT_ID`; `diff FROM TO`; `purge` |

### Top-level utility syntax

| Route | Syntax and defaults |
|---|---|
| `update` | `[--dry-run] [--version REQUIREMENT] [--force] [--registry NAME]` |
| `install-chromium` | `[--update]` |
| `capabilities` | no positional arguments |
| `doctor` | `[--json]` |
| `mcp-config` | `[--client generic\|claude-code\|codex]` (default `generic`), `[--print]` |
| `delete-profile` | `NAME` |
| `browser` | optional `tui` |
| `session` | `start|status|stop|open [NAME]` (default `default`) |
| `help` | `[TOPIC]` |
| `tui` | no positional arguments |

The browser command surface is the flat route list above. Common guarded
forms include `navigate URL [--timeout-ms 20000]`, `observe [--level
summary|interactive|structured|detailed|raw] [--region ID]`, `screenshot
[-o screenshot.png] [--format png|jpeg|webp] [--quality 0..100] [--scale
0.1..4.0]`, `batch [JSON_FILE|-] [--atomic] [--mode unguarded|fixed|chain]
[--expected-revision N]`, and `fill-form --fields JSON
[--expected-revision N]`. `upload TARGET FILE...` requires 1–16 regular files.
`scroll` defaults to `--dx 0 --dy 600`; `wait CONDITION` defaults to
`--timeout-ms 10000`; `diagnostics` defaults to `--duration-ms 1000`, and
`download DIRECTORY` defaults to `--timeout-ms 30000`. `screenshot` rejects
PNG quality and enforces its format/target/clip conflicts.

The browser workspace routes are exactly `glass browser` and `glass browser
tui`. `browser start`, `browser targets`, `browser remote-open`, and similar
names are TUI command-palette routes, not Clap commands; see
[the development TUI guide](architecture/development-tui.md). The
`glass-browser` executable is the browser-only product: its help hides the development
`project` and `agent` routes. The shared parser still recognizes `harness` so
the command can report a product-boundary error, but external harness launch
is supported only by `glass`. `glass` exposes the complete development tree.
The checked-in [CLI inventory](cli-inventory.json) is generated schema
authority; live `--help` wins if an installed binary differs.

### Family contracts

| Family | Primary input | Output/state | Important failure or recovery |
|---|---|---|---|
| `certify` | scenario, manifest, or replay evidence | bounded certification evidence | missing observations and forbidden outcomes block release status |
| `workspace` | workspace ID and expected lifecycle revision | persisted identity, ownership, attachments, and lease state | stale revision or active ownership fails closed |
| `project` | canonical root, relative paths, commands, actor | files, buffers, PTYs, diagnostics, graph, timeline | path escape, edit conflict, process degradation, and missing LSP are explicit |
| `agent` | bounded text/control request and harness | attributed local/Pi harness result | unavailable adapter, stale context, absent browser, and authority denial are distinct |
| `memory` | memory selector or snapshot operation | advisory memory projection | stale/quarantined evidence cannot authorize an action |
| `surfaces` | versioned surface evidence | capability and coverage result | unknown/weak evidence fails closed for input authority |
| `backend` | backend profile/evidence | status, capabilities, proof test | an unregistered or incompatible backend cannot become active |
| `daemon` | local socket/status lifecycle | resident MCP session and recovery state | stale PID/socket is diagnosed; interrupted mutations require acknowledgement |
| `replay` | validated replay bundle | inspection, diff, or attachment metadata | replay data does not dispatch recorded input automatically |
| `profiles` | validated profile name | named Chrome user-data lifecycle | active ownership prevents deletion |
| `knowledge` | profile/workspace-scoped record selector | bounded validated knowledge store | scope, provenance, lifecycle, and sensitivity checks reject unsafe records |
| `result` | result ID or bounded age | stored diagnostic projection/purge count | invalid IDs and malformed artifacts are rejected |
| `workflow` | YAML/JSON definition or authoring input | canonical definition, preview, lint, diff, or execution | invalid bounds, unsafe retry suffix, and stale checkpoint stop execution |
| `task` | Task Protocol and Web IR JSON | validation, deterministic plan, or verified execution | ambiguous/unproven entities and unsupported capabilities fail before dispatch |
| `ir` | Glass Web IR v1 JSON | validation, summary, diff, continuity, canonical JSON | invalid graph, revision drift, and ambiguous continuity stay explicit |
| `checkpoint` | bounded workflow/checkpoint files | portable redacted checkpoint | definition/route/effect mismatch prevents resume |
| `snapshot` | profile and snapshot selector | redacted bounded session evidence | snapshot data never restores live browser authority |
| `browser` | optional `tui` subcommand | browser-first terminal workspace | browser startup/recovery stays in the workspace; use the TUI App surface for targets |
| `session` | optional session name | persistent browser owner and attach metadata | `status` is read-only; `open` prints the attach command; `stop` closes the owned browser |

## Browser workspace and persistent sessions

The top-level `browser` family launches the browser-first development
workspace. These are equivalent:

```console
glass browser
glass browser tui
```

The browser TUI owns browser startup, target selection, and recovery. Its
command palette has routes such as `:browser start`, `:browser targets
QUERY`, and `:browser remote-open`; these are not Clap subcommands. Browser
startup is lazy until an explicit launch or browser operation. A busy or
unusable endpoint is kept in the TUI recovery flow rather than silently
reused. See [the development TUI guide](architecture/development-tui.md) and
[browser connection](architecture/browser-connection.md) for that UI-owned
state machine.

Persistent browser ownership is a separate top-level family:

```console
glass session start review
glass session status review
glass session open review
glass session stop review
```

Omitting the name uses `default`. `start` launches the named owner, `status`
inspects it without starting Chrome, `open` prints its attach command, and
`stop` closes the owned browser. `--session NAME` attaches browser operations
to an already-started named session; it does not create one.

Top-level utility routes are `update`, `install-chromium`, `capabilities`,
`doctor`, `mcp-config`, `delete-profile`, `help`, and `tui`. With no command or
prompt, `glass` opens the development TUI in an interactive terminal; with
redirected stdin/stdout it prints its start-here message instead. `--mcp`
starts the MCP stdio server and must be supplied as a top-level option. The
`glass-browser` executable is the browser-only binary; use its own
`--help` for the installed subset.

## Semantic observations

Use a semantic level when you need structured page state:

```console
glass observe --level summary
glass observe --level interactive
glass observe --level structured --region REGION_ID
```

Do not combine a semantic level with `--deep-dom`, `--screenshot`, or
`--form-values`. Read [semantic observations](semantic-observation.md) for
levels, revisions, regions, and diffs.

## Targets and revisions

Target forms are:

- `ref=r7:b42` or `r7:b42`;
- `name=Save`;
- `role=button;name=Save`;
- `text=Continue`;
- `css=button.primary`; and
- `ordinal=2`.

Every locator must resolve one target. An ambiguous locator fails with bounded
candidate data.

Prefer a revisioned reference for automation. Pass the revision from
`observe`:

```console
glass click r7:b42 --expected-revision 7
glass type 'hello' --target r7:b43 --expected-revision 7
```

Glass rejects a stale revision before it sends the browser action. The result
contains typed status, previous and current revisions, an execution ID, and
bounded verification evidence.

`glass archive-targets` exports the current page-target inventory without
selecting or closing a target:

```console
glass archive-targets
glass archive-targets --output targets.json
```

The archive is bounded to 64 KiB and contains redacted target metadata only;
it does not include cookies, form values, or page content. An output path is
accepted only when the active policy permits it.

The revision option is available on navigation, actions, scrolling, keyboard,
drag, upload, popup, and form-fill commands. Existing calls without the option
remain compatible.

For conservative recovery, `glass recover-run EXECUTION_ID` inspects a possibly
indeterminate execution without dispatching a new mutation. An `indeterminate`
result means the browser may have accepted input: observe current state,
reconcile references, or resume only a validated workflow checkpoint. Do not
blindly repeat the action. `checkpoint export` and `checkpoint import [FILE]`
operate on bounded redacted workflow checkpoints; import reads stdin when FILE
is omitted.

## Workflow and authoring

`glass workflow FILE` validates and runs a bounded workflow.

These commands are offline. They do not start Chrome:

```console
glass workflow compile FILE
glass workflow format FILE
glass workflow validate FILE
glass workflow lint FILE
glass workflow preview FILE
glass workflow diff BEFORE AFTER
glass workflow record
```

`glass workflow-resume` reconciles a checkpoint and runs only the safe
pending suffix. It refuses post-dispatch ambiguity, route changes, definition
mismatches, and completed checkpoints.

Read [workflows](workflows.md) and [workflow authoring](workflow-authoring.md).

## Intent and knowledge

Resolve an intent without dispatch:

```console
glass resolve-intent request.json
glass execute-intent execution.json
```

The execute command observes and resolves again before it acts.

Knowledge commands do not start Chrome:

```console
glass knowledge list
glass knowledge show RECORD_ID
glass knowledge explain RECORD_ID
glass knowledge stats
glass knowledge export [PATH]
glass knowledge import SNAPSHOT.json
glass knowledge invalidate RECORD_ID stale
glass knowledge purge ORIGIN
```

Read [intent resolution](intent-resolution.md) and [persistent knowledge](knowledge.md).

## Daemon and diagnostics

Run:

```console
glass capabilities
glass daemon start
glass daemon status
glass daemon doctor
glass daemon logs
glass daemon stop
glass doctor
```

`capabilities` prints the negotiated capability manifest without starting
Chrome. `doctor` prints bounded browser, daemon, profile, policy, store, and
extension-loader status. It does not start Chrome or load extensions.

The daemon is local-only. Read [Local daemon](daemon.md).

## Project development runtime

The `project` command family is browser-free and operates on the selected
workspace root. Paths are confined to that root, file writes are atomic, PTY
output is bounded, and mutations are recorded with an actor and timeline
event.

```console
glass project inspect --root .
glass project files --root .
glass project read README.md --root .
glass project edit notes.txt --content "hello\n" --root .
glass project search checkout --root .
glass project run check --command "cargo check" --wait --root .
glass project test --root .
glass project lint --root .
glass project diagnostics src/main.rs --root .
glass project graph discover --root .
glass project replay --root .
glass project neovim probe --root .
glass project experiment create alternative --port 3101 --root .
glass project diff --root .
glass project timeline --root .
```

One-shot CLI PTY operations require `--wait`; use the TUI for persistent
interactive dev servers, input, resize, restart, and live output. Use
`glass project link` only for explicit source/runtime evidence. Glass does not
infer framework source maps or confirm live updates without browser revision
evidence.

The deterministic local harness is available through `agent hello` and
`agent prompt`. `--harness local` uses Glass's local deterministic adapter.
`--harness pi` selects the one-shot legacy `PiHarness` RPC adapter; it starts
Pi with `--mode rpc --no-approve --no-builtin-tools` for this request. These
commands are not the resident native Pi AgentSession. The resident
`AgentRegistry`/Pi session is owned by the development TUI and is controlled
with its agent routes; see [the runtime guide](development-runtime.md).

```console
glass agent hello --harness local --root .
glass agent prompt "read README.md" --harness local --root .
glass agent hello --harness pi --root .
glass agent prompt "Explain @diagnostic" --harness pi --root .
```

Pi readiness is separate from the legacy request adapter. Run `glass agent
doctor` or `glass agent status` to inspect Node, the pinned managed SDK,
provider/authentication, and session readiness. `glass agent setup` installs
the managed `@earendil-works/pi-coding-agent` SDK at the exact pinned version
`0.84.3`; `glass agent setup --update` reinstalls it. Use `--sdk-entry FILE`
to select an existing SDK entry and `--agent-dir DIR` with it to select an
existing credential/model directory. `--login` opens Pi's provider login flow
after setup. One-shot Pi requests wait for `agent_settled`; extension UI
requests are denied because a one-shot caller has no safe approval host.

The standard Glass tool behavior is explicit and bounded. `read` accepts
one-based `offset` and `limit`; `ls` accepts a path prefix and limit; `grep`
is a literal UTF-8 search with optional path prefix, `*`/`?` glob, case
folding, context, and limit; `find` matches project paths with `*` and `?`;
`edit` applies one atomic set of unique exact replacements; and `bash` has a
caller-selected timeout capped at 300 seconds.

The collaborative native editor is TUI-owned. Its palette routes include
`:editor open PATH`, `:editor edit`, `:editor selection PATH`,
`:editor replace PATH OLD NEW`, `:editor replace-selection TEXT`,
`:editor save PATH`, `:editor undo PATH`, `:editor redo PATH`,
`:editor comments [PATH]`, `:editor comment PATH START END TEXT`,
`:editor comment-selection TEXT`, `:editor comment-resolve ID`,
`:editor proposals`, `:editor propose PATH SUMMARY TEXT`,
`:editor accept ID`, `:editor reject ID`, `:editor checkpoints`,
`:editor checkpoint NAME`, `:editor restore CHECKPOINT_ID`, and
`:editor search QUERY`. These are command-palette routes, not Clap aliases;
they update the shared editor projection and use the TUI's confirmation,
selection, and recovery state. The CLI `project edit PATH` remains the
single-file native-buffer mutation with `--content` or `--input`.

### External harness parity

The CLI and TUI share one fixed PATH-discovered harness catalog:

```console
glass harness list
glass harness start codex --root .
```

The equivalent TUI commands are `:harness list` and `:harness start codex`.
Both launch the selected installed program in the project root and return to
the client after it exits.

### Temporary external agents

Glass can make a bounded one-shot delegation to an installed Codex CLI,
Claude Code, or OpenCode process without registering that process as a resident
Glass Agent:

```console
glass agent delegate codex "inspect the failing test and explain the smallest fix" --root .
glass agent delegate claude "review the current diff for regressions" --root .
glass agent delegate opencode "summarize the project entrypoints" --root .
```

The TUI equivalent is `:harness delegate NAME PROMPT`, with optional
`--sandbox`, `--timeout-secs`, `--allow-mutation`, and `--yes` tokens. The
prompt can be `-` to read it from stdin in the CLI. `NAME` must be `codex`,
`claude`, or `opencode`; each installed executable is resolved from `PATH`.
The default sandbox is `read-only`; `--sandbox workspace-write` requires both
`--allow-mutation` and `--yes` (or the process-scoped `--yolo` flag). Timeout
defaults to 600 seconds and must be 1..3600 seconds. Prompts are limited to
64 KiB; captured stdout is limited to 256 KiB and stderr to 64 KiB, with
truncation flags in the JSON result. Glass passes the canonical workspace
root as a process argument, never through a shell.

Delegation uses the selected harness's structured output mode and returns JSON
with exit status, timeout, transport, sandbox, duration, output, stderr, and
truncation fields. A nonzero child exit or timeout prints the result before
returning a failing CLI status. The child is temporary and is never
registered as a resident Glass Agent.

The resident Glass Agent exposes a separate governed `delegate` tool /
`glass.agent.delegate` route. It remains approval-gated, bounded, and
ephemeral; it is not the legacy one-shot Pi adapter or an external harness
process.

### Unrestricted Pi mode

```console
glass --yolo
glass --yolo agent prompt "Implement and verify the requested change" --harness pi --root .
```

This is a process-scoped trust decision. Glass skips its mutation approval
sheet, the Pi extension does not request approval, and any confirmation RPC
from another loaded Pi extension is accepted automatically. Ambient context,
extensions, skills, templates, themes, and registered extension tools are
enabled. Browser policy capabilities are treated as explicitly allowed, even
if a confirmation capability was also supplied on the command line.

`--yolo` does not turn correctness guards into best effort: project file tools
remain root-confined, browser revisions remain guarded, workspace/daemon
mutation leases still apply, explicit host denials still deny, and request and
result bounds remain active. The unrestricted `bash` tool and loaded extensions
can execute arbitrary commands as the current OS user, so those components can
operate outside the project root.

Pi's configured providers and `models.json` models are available through
`agent models` and `agent set-model PROVIDER MODEL_ID`; `agent thinking LEVEL`
sets the thinking level. The default uses the cached catalog, ephemeral
session, and only the Glass-owned extension. Set
`GLASS_PI_ONLINE_CATALOG=1` for catalog refresh,
`GLASS_PI_PERSIST_SESSION=1` for Pi-managed session persistence, or
`GLASS_PI_TRUSTED_RESOURCES=1` to load ambient context files, extensions,
skills, templates, and themes. Trusted resources are executable local code and
are outside Glass's per-tool authority boundary. This opt-in also removes the
extension-tool allowlist so installed extension tools can be selected by Pi;
Glass's raw built-in filesystem and shell tools remain disabled because their
names are replaced by the Glass-owned overrides.

## Profiles and files

Run:

```console
glass update --dry-run
glass update
glass install-chromium
glass profiles
glass profiles create NAME
glass profiles delete NAME
glass delete-profile NAME
glass tui
```

`update` is browser-free. It updates the Cargo package that owns the invoked
executable, preserves its detected install root, and uses `--locked`. Optional
`--version REQUIREMENT`, `--force`, and `--registry NAME` values pass through
as bounded Cargo install choices. It fails instead of guessing for unmanaged
binaries, ambiguous owners, or an implicit source-channel change. See
[Installation and operations](installation.md#update-a-cargo-installation) for
the complete ownership, provenance, custom-root, and Windows contracts.

`delete-profile NAME` is a separate top-level route retained alongside
`profiles delete NAME`; both delete one profile.

`export-cookies FILE` and `import-cookies FILE` provide explicit profile
state transfer. They require the persistent-profile capability. Imports are
limited to 512 KiB and 256 cookies.

Quote selectors and values that contain spaces or shell metacharacters:

```console
glass click 'css=button[type="submit"]'
glass type 'hello world' --target 'css=#message'
```

## Convenience prompts

Glass accepts a limited set of prompts:

```console
glass "navigate to https://example.com"
glass "click Sign in"
```

This feature parses known command forms. It is not a general language
interpreter. Use explicit subcommands in scripts.

## Output

Navigation, action, observation, DOM, scroll, and evaluation results use JSON on
stdout. `text` emits plain text. `screenshot` writes PNG by default, accepts
`--format png|jpeg|webp`, and prints the output path. JPEG/WebP accept quality
`0`–`100`; PNG rejects a quality value. Scale is finite `0.1`–`4.0`, and
`--full-page`, `--clip`, and `--target` are mutually exclusive. The output
extension is not silently rewritten, so keep it consistent with the selected
format.
All structured command results, including `diagnostics`, use compact JSON on
stdout. `text` emits plain text. Operational logs and warnings use stderr.
With `--trace-on-error`, Glass writes one bounded failure trace to stderr; it
contains compact observation, target, and frame state, not raw page data or
secret values.

## Live-site smoke tests

Use `smoke-sites MANIFEST` for a bounded, read-only compatibility probe. It
performs navigation, compact observation, structured inspection, safe target
preflight, and a post-observation revision check. See
[Live-site smoke testing](site-smoke.md) for the manifest and result contract.

`batch` reads a JSON file, stdin when no path is supplied, or stdin explicitly
with `-`. Inline JSON is not a positional argument.
