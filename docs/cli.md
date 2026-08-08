# CLI reference

Run `glass --help` or `glass COMMAND --help` for the exact syntax for the
installed version.

## Global options

| Option | Default | Function |
|---|---|---|
| `--profile NAME` | `default` | Use a persistent browser profile. |
| `--incognito` | off | Use a disposable browser profile. |
| `--attach` | off | Connect to an existing CDP endpoint. |
| `--target-id ID` | automatic | Select a page target. |
| `--frame-id ID` | main frame | Select a frame. |
| `--port PORT` | `9222` | Set the local CDP port. |
| `--headed` | off | Show the Chrome window. |
| `--interaction human\|fast` | `human` | Select pointer event mode. |
| `--trace-on-error` | off | Write one bounded failure trace to stderr. |
| `--chrome-path PATH` | discovered | Select the browser executable. |
| `--knowledge-store PATH` | profile-scoped | Select the knowledge store. |
| `--mcp` | off | Start the MCP stdio server. |
| `--tui-layout auto\|desktop\|mobile` | `auto` | Select responsive terminal layout policy. |

Place global options before or after the subcommand.

Set `GLASS_CONFIG_HOME` to select the configuration and profile root. If it is
not set, Glass uses the operating system configuration directory.

Chrome sandboxing is enabled by default. Set
`GLASS_DISABLE_CHROME_SANDBOX=1` only in an already isolated container or CI
environment.

## Browser commands

The main commands are:

```text
navigate URL
click TARGET
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
scroll
wait CONDITION
diagnostics
accept-dialog
dismiss-dialog
download DIRECTORY
targets
new-target URL
select-target ID
close-target ID
frames
select-frame ID
evaluate EXPRESSION
batch [JSON_FILE|-]
smoke-sites MANIFEST
workflow [JSON_FILE]
verify PREDICATE_JSON
resolve-intent [JSON_FILE]
execute-intent [JSON_FILE]
knowledge SUBCOMMAND
capabilities
doctor
tui
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

## Profiles and files

Run:

```console
glass install-chromium
glass profiles
glass profiles create NAME
glass profiles delete NAME
glass delete-profile NAME
glass tui
```

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
stdout. `text` emits plain text. `screenshot` writes a PNG and prints its
path.

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
