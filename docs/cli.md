# CLI reference

Run `glass --help` or `glass COMMAND --help` for the authoritative reference
for the installed version.

## Global session options

| Option | Default | Purpose |
|---|---:|---|
| `--profile NAME` | `default` | Persistent browser profile. |
| `--incognito` | off | Disposable browser session. |
| `--attach` | off | Connect to an existing CDP endpoint. |
| `--target-id ID` | automatic | Select a page on an attached endpoint. |
| `--frame-id ID` | main frame | Select a frame for this CLI invocation. |
| `--port PORT` | `9222` | CDP debugging port. |
| `--headed` | off | Display the Chrome window. |
| `--interaction human|fast` | `human` | Smooth or direct pointer events. |
| `--trace-on-error` | off | Emit one bounded JSON failure trace on stderr when an operation fails. |
| `--chrome-path PATH` | discovered | Explicit Chrome/Chromium executable. |
| `--knowledge-store PATH` | profile-scoped | Select the persistent knowledge snapshot. |
| `--mcp` | off | Run the MCP stdio server. |

Global options can be written before or after a subcommand.

Set `GLASS_CONFIG_HOME` to place Glass configuration and named profiles under
an explicit root on every platform. When unset, Glass uses the operating
system's standard configuration directory.

Chrome's process sandbox remains enabled by default. In a container or CI
kernel that cannot provide Chrome's sandbox, set `GLASS_DISABLE_CHROME_SANDBOX=1`
only inside that already-isolated environment; do not use it for ordinary
browsing.

## Browser commands

```text
navigate URL [--timeout-ms MILLISECONDS] [--expected-revision REVISION]
click TARGET [--expected-revision REVISION]
click-expect-popup TARGET [--expected-revision REVISION]
double-click TARGET [--expected-revision REVISION]
hover TARGET
drag SOURCE DESTINATION [--expected-revision REVISION]
type TEXT [--target TARGET] [--expected-revision REVISION]
key KEY [--expected-revision REVISION]
key-down KEY [--expected-revision REVISION]
key-up KEY [--expected-revision REVISION]
shortcut SHORTCUT [--expected-revision REVISION]
clear TARGET [--expected-revision REVISION]
check TARGET [--expected-revision REVISION]
uncheck TARGET [--expected-revision REVISION]
select TARGET VALUE [--expected-revision REVISION]
upload TARGET FILE... [--expected-revision REVISION]
fill-form --fields JSON [--expected-revision REVISION]
screenshot [-o|--output FILE]
text
dom
observe [--deep-dom] [--screenshot] [--form-values]
observe --level summary|interactive|structured|detailed|raw [--region REGION_ID]
scroll [--dx PIXELS] [--dy PIXELS] [--expected-revision REVISION]
wait CONDITION [--timeout-ms MILLISECONDS]
diagnostics [--duration-ms MILLISECONDS]
accept-dialog
dismiss-dialog
download DIRECTORY [--timeout-ms MILLISECONDS]
targets
new-target URL
select-target ID
close-target ID
frames
select-frame ID
evaluate EXPRESSION
batch [JSON_FILE] [--atomic] [--mode fixed|chain|unguarded] [--expected-revision REVISION]
workflow [JSON_FILE]
workflow compile|format|validate|lint|preview SOURCE
workflow diff BEFORE AFTER
workflow-resume WORKFLOW_JSON CHECKPOINT_JSON [--inputs INPUTS_JSON]
certify plan --scenario SCENARIO_JSON --fixture FIXTURE_JSON
certify replay --scenario SCENARIO_JSON --input REPLAY_JSON
certify replay-diff --scenario SCENARIO_JSON --before REPLAY_JSON --after REPLAY_JSON
certify release --version VERSION --scenarios SCENARIOS_JSON --observations OBSERVATIONS_JSON [--replays REPLAYS_JSON]
verify PREDICATE_JSON [--timeout-ms MILLISECONDS]
resolve-intent [JSON_FILE]
execute-intent [JSON_FILE]
knowledge list|show|explain|stats|export|import|invalidate|purge
```

`screenshot` defaults to `screenshot.png`. `scroll` defaults to `dx=0` and
`dy=600`. `dom` and `observe --deep-dom` are explicit deep-inspection actions;
`wait` defaults to a 10-second bounded deadline and accepts the typed condition
forms documented in the browser architecture (`lifecycle=`, `url=`,
`url-prefix=`, target states, `text=`, `js=`, and `network-quiet=`);
normal observations do not collect the full DOM. Likewise, screenshots are
only captured by `screenshot` or `observe --screenshot`.

`observe --level` returns the versioned semantic observation contract. Use
`--region` for a revision-checked scoped expansion. Semantic options cannot be
combined with deep DOM, screenshots, or form values; see
[semantic observations](semantic-observation.md) for payload levels and
revision behavior.

`resolve-intent` reads a versioned request and returns bounded candidates,
evidence, confidence, and the policy decision without dispatching an action.
`execute-intent` reads a request containing the selected `candidateId`, repeats
resolution against a fresh observation, and dispatches only when the selected
policy permits it. See [intent resolution](intent-resolution.md) for the JSON
shapes and supported action classes.

Knowledge commands inspect or manage the bounded profile-scoped store without
starting Chrome. `list`, `show`, `explain`, and `stats` are read-only;
`export`, `import`, `invalidate`, and `purge` are explicit lifecycle operations.
See [persistent knowledge](knowledge.md) for scope, privacy, and freshness
rules.

`diagnostics` explicitly leases console and network domains for at most 30
seconds and returns bounded, secret-redacted metadata. Dialog commands act only
on an already open dialog. `download` serializes one browser-global download
scope, restricts its destination to an existing directory under the authorized
working root, and restores download denial on success, error, or cancellation.

`verify` accepts bounded JSON predicates such as
`{"urlEquals":"https://example.com"}`, `{"visible":"r7:b42"}`,
`{"textContains":"Ready"}`, or Boolean compositions using `all`, `any`, and
`not`. It never evaluates caller-provided JavaScript.

`workflow` reads a JSON document containing `workflow` and `inputs` objects
(or a workflow definition by itself), validates it before dispatch, and emits
the workflow result with step states, terminal proof, typed outputs, and a
deterministic trace.

The `workflow compile`, `format`, `validate`, `lint`, `preview`, and `diff`
subcommands are offline authoring operations. They parse YAML or JSON, apply
the same workflow validation as the runtime, and do not start Chrome. See
[workflow authoring](workflow-authoring.md) for diagnostics and redaction
boundaries.

`certify replay` validates one redacted reliability replay bundle against its
versioned scenario. `certify release` evaluates the complete scenario and
observation set and exits unsuccessfully when required evidence is missing or
unsafe outcomes are present. These commands do not start Chrome. See the
[reliability laboratory](reliability.md) guide for the supported contracts and
current boundaries.

`workflow-resume` reconciles the definition and checkpoint against the current
browser state, then executes only the safe pending suffix. It refuses
post-dispatch ambiguity, route changes, definition mismatches, and completed
checkpoints.

`targets` and `frames` are bounded topology queries. Creating a target or
discovering a popup never selects it. In a one-shot CLI workflow, discover IDs
first and pass `--target-id` and `--frame-id` on the action invocation. MCP and
library sessions can also use `select-target` and `select-frame` without
restarting. Closing the active target leaves no implicit replacement.

Navigation, action, observation, DOM, scroll, and evaluation results are
compact JSON on stdout. `text` emits plain text. `screenshot` writes a PNG and
prints its destination. With `--trace-on-error`, failures additionally emit a
bounded trace pack on stderr containing the last compact observation and active
target/frame topology.

Keyboard commands emit browser key events; `type` remains the efficient plain
text path. Shortcuts use `Control+A`/`Shift+Enter` syntax. Upload accepts 1–16
regular files and never includes paths or contents in its result.

## Element targets

`click`, `double-click`, and `type --target` accept explicit locator forms:

- `ref=r7:b42` (or the bare `r7:b42` fast-path reference);
- `name=Save` (a bare string is an exact accessible name for compatibility);
- `role=button;name=Save`;
- `text=Continue`, `css=button.primary`, or `ordinal=2`.

Use `click-expect-popup TARGET` when the selected element is expected to open
one popup. It returns the verified popup target without implicitly selecting
it. Ordinary `click` retains strict CDP acknowledgement semantics.

Prefer revisioned references for automation workflows. They let Glass reject a
reference after page state changes instead of acting on a stale element.
For an explicit action contract, pass the revision from `observe` with
`--expected-revision`; stale state is rejected before the action runs and the
result includes typed status plus bounded verification metadata. The flag is
available on navigation, click, double-click, type, clear, check, uncheck,
select, scroll, keyboard, drag, upload, popup, and form-fill commands. The
`fill-form --fields` value is a JSON array of `{target, value}` objects.
Existing commands without the flag remain compatible.
Every locator must resolve uniquely. Ambiguous names, text, or selectors fail
with bounded candidates instead of choosing the first match.

Quote selectors or text containing spaces or shell metacharacters:

```console
glass click 'css=button[type="submit"]'
glass type 'hello world' --target 'css=#message'
```

## Profile and utility commands

```text
install-chromium
profiles [list|create NAME|delete NAME]
delete-profile NAME
tui
```

`profiles` without an action lists profiles. `delete-profile NAME` is retained
as a direct alias for profile deletion.

`export-cookies FILE` and `import-cookies FILE` provide an explicit profile
state hand-off. Both operations require the persistent-profile capability;
imports are capped at 512 KiB and 256 cookies.

## One-shot prompts

Glass recognizes a small set of convenience prompts, including `navigate to`,
`go to`, `open`, `click`, `double click`, `type`, `screenshot`, `text`, `dom`,
and `observe`:

```console
glass "navigate to https://example.com"
glass "click Sign in"
```

This is command parsing, not a general-purpose language interpreter. Unrecognized prompt
text is evaluated as JavaScript in the current page; scripts should prefer
explicit subcommands to avoid ambiguity.
