# CLI reference

Run `glass --help` or `glass COMMAND --help` for the exact syntax for the
installed version.

The checked-in [CLI inventory](cli-inventory.json) records the exact public
top-level and nested command tree for documentation drift validation. Live
`--help` remains the authority for flags, defaults, and positional arguments.

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
| `--target-id ID` | automatic | Select a page target. |
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
| `--tui-live off\|auto\|on` | `off` | Keep continuous pixels off, require a detected native backend, or allow ANSI fallback. |
| `--tui-live-backend auto\|herdr\|ansi` | `auto` | Select the terminal-native renderer; `kitty` is accepted and currently rendered through the ANSI path. |
| `--tui-live-quality data\|balanced\|smooth` | `balanced` | Select the adaptive capture size and target frame rate. |
| `--tui-live-fit contain\|cover\|actual` | `contain` | Select ANSI sampling; native image backends use contain. |

Place global options before or after the subcommand.

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
| `agent` | `hello`, `prompt`, `steer`, `follow-up`, `models`, `set-model`, `thinking`, `abort`, `new-session` |
| `memory` | `status`, `inspect`, `explain`, `forget`, `export`, `prune`, `reindex` |
| `surfaces` | `inspect`, `coverage` |
| `backend` | `status`, `capabilities`, `test` |
| `daemon` | `start`, `status`, `stop`, `doctor`, `logs`, `acknowledge-recovery` |
| `replay` | `inspect`, `diff`, `attach` |
| `profiles` | `list`, `create`, `delete` |
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

Standalone utility commands are `update`, `install-chromium`, `capabilities`,
`doctor`, `mcp-config`, `delete-profile`, and `tui`. With no command or prompt, `glass`
opens the TUI. The `glass-browser` executable exposes the browser-control
subset; use its own `--help` as the installed-version authority.

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

The revision option is available on navigation, actions, scrolling, keyboard,
drag, upload, popup, and form-fill commands. Existing calls without the option
remain compatible.

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

The deterministic local harness is available through:

```console
glass agent hello --root .
glass agent prompt "read README.md" --root .
glass agent hello --harness pi --root .
glass agent models --root .
glass agent prompt "Explain @diagnostic" --harness pi --root .
glass agent steer "focus on the failing test" --root .
```

The Pi path uses Glass's embedded system prompt and a single bounded SDK gateway
rather than Pi's raw filesystem/shell implementation. Run `glass agent doctor`,
`glass agent setup [--login]`, or `glass agent status` to install or inspect
Node, SDK, provider/auth, and session
readiness; run `glass agent setup` for an explicit pinned install, or
`glass agent setup --login` for Pi's provider login flow. A one-shot Pi prompt
(`doctor`, `setup`, and `status` are the lifecycle subcommands.)
waits for `agent_settled`; steer, follow-up, and abort remain useful in the
resident TUI where the same SDK session stays active. A mutation
freezes and privately serializes its exact arguments, then pauses on a Glass
approval sheet. `Y`/Enter approves once; `N`/Esc denies. The approval expires
after 120 seconds and cannot authorize a retry or reshaped call. Exact edits
must still match uniquely and are applied atomically. One-shot CLI Pi requests
have no interactive approval host and therefore deny mutations.

The standard tool behavior is explicit and bounded. `read` accepts one-based
`offset` and `limit`; `ls` accepts a path prefix and limit; `grep` is a literal
UTF-8 search with optional path prefix, `*`/`?` glob, case folding, context, and
limit; `find` matches project paths with `*` and `?`; `edit` applies one atomic
set of unique exact replacements; and `bash` has a caller-selected timeout
capped at 300 seconds.

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
`project pi models` and model selection. The default uses the cached catalog,
ephemeral session, and only the Glass-owned extension. Set
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
`--version`, `--force`, and `--registry` values pass through as bounded Cargo
install choices. It fails instead of guessing for unmanaged binaries,
ambiguous owners, or an implicit source-channel change. See
[Installation and operations](installation.md#update-a-cargo-installation) for
the complete ownership, provenance, custom-root, and Windows contracts.

`delete-profile` remains an alias for profile deletion.

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

Diagnostics use stderr. With `--trace-on-error`, Glass writes a bounded
failure trace to stderr. The trace includes compact observation and target and
frame state. It does not include raw page data or secret values.

## Live-site smoke tests

Use `smoke-sites MANIFEST` for a bounded, read-only compatibility probe. It
performs navigation, compact observation, structured inspection, safe target
preflight, and a post-observation revision check. See
[Live-site smoke testing](site-smoke.md) for the manifest and result contract.

`batch` reads a JSON file, stdin when no path is supplied, or stdin explicitly
with `-`. Inline JSON is not a positional argument.
