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
| `--chrome-path PATH` | discovered | Explicit Chrome/Chromium executable. |
| `--mcp` | off | Run the MCP stdio server. |

Global options can be written before or after a subcommand.

Set `GLASS_CONFIG_HOME` to place Glass configuration and named profiles under
an explicit root on every platform. When unset, Glass uses the operating
system's standard configuration directory.

## Browser commands

```text
navigate URL [--timeout-ms MILLISECONDS]
click TARGET
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
screenshot [-o|--output FILE]
text
dom
observe [--deep-dom] [--screenshot]
scroll [--dx PIXELS] [--dy PIXELS]
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
```

`screenshot` defaults to `screenshot.png`. `scroll` defaults to `dx=0` and
`dy=600`. `dom` and `observe --deep-dom` are explicit deep-inspection actions;
`wait` defaults to a 10-second bounded deadline and accepts the typed condition
forms documented in the browser architecture (`lifecycle=`, `url=`,
`url-prefix=`, target states, `text=`, `js=`, and `network-quiet=`);
normal observations do not collect the full DOM. Likewise, screenshots are
only captured by `screenshot` or `observe --screenshot`.

`diagnostics` explicitly leases console and network domains for at most 30
seconds and returns bounded, secret-redacted metadata. Dialog commands act only
on an already open dialog. `download` serializes one browser-global download
scope, restricts its destination to an existing directory under the authorized
working root, and restores download denial on success, error, or cancellation.

`targets` and `frames` are bounded topology queries. Creating a target or
discovering a popup never selects it. In a one-shot CLI workflow, discover IDs
first and pass `--target-id` and `--frame-id` on the action invocation. MCP and
library sessions can also use `select-target` and `select-frame` without
restarting. Closing the active target leaves no implicit replacement.

Navigation, action, observation, DOM, scroll, and evaluation results are
compact JSON on stdout. `text` emits plain text. `screenshot` writes a PNG and
prints its destination.

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

Prefer revisioned references for agent workflows. They let Glass reject a
reference after page state changes instead of acting on a stale element.
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

## One-shot prompts

Glass recognizes a small set of convenience prompts, including `navigate to`,
`go to`, `open`, `click`, `double click`, `type`, `screenshot`, `text`, `dom`,
and `observe`:

```console
glass "navigate to https://example.com"
glass "click Sign in"
```

This is command parsing, not a natural-language model. Unrecognized prompt
text is evaluated as JavaScript in the current page; scripts should prefer
explicit subcommands to avoid ambiguity.
