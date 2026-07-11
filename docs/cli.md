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
| `--port PORT` | `9222` | CDP debugging port. |
| `--headed` | off | Display the Chrome window. |
| `--interaction human|fast` | `human` | Smooth or direct pointer events. |
| `--chrome-path PATH` | discovered | Explicit Chrome/Chromium executable. |
| `--mcp` | off | Run the MCP stdio server. |

Global options can be written before or after a subcommand.

## Browser commands

```text
navigate URL
click TARGET
double-click TARGET
type TEXT [--target TARGET]
screenshot [-o|--output FILE]
text
dom
observe [--deep-dom] [--screenshot]
scroll [--dx PIXELS] [--dy PIXELS]
evaluate EXPRESSION
```

`screenshot` defaults to `screenshot.png`. `scroll` defaults to `dx=0` and
`dy=600`. `dom` and `observe --deep-dom` are explicit deep-inspection actions;
normal observations do not collect the full DOM. Likewise, screenshots are
only captured by `screenshot` or `observe --screenshot`.

Navigation, action, observation, DOM, scroll, and evaluation results are
compact JSON on stdout. `text` emits plain text. `screenshot` writes a PNG and
prints its destination.

## Element targets

`click`, `double-click`, and `type --target` accept:

- a revisioned accessibility reference returned by `observe`, such as
  `r7:b42`;
- an accessible name; or
- a CSS selector.

Prefer revisioned references for agent workflows. They let Glass reject a
reference after page state changes instead of acting on a stale element.

Quote selectors or text containing spaces or shell metacharacters:

```console
glass click 'button[type="submit"]'
glass type 'hello world' --target '#message'
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
