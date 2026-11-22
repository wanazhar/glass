# Browser and computer-use audit (Linux)

This audit covers the `glass-browser` crate and the computer-use (managed
desktop) code in `glass-core`, as they behave on Linux. macOS and Windows are
out of scope. It records what each tool does today, how a model reaches it,
where it falls short of strong browser and computer-use agents, and the
prioritized plan that the follow-up changes implement. Measured eval numbers
live in the [eval section](#tool-selection-eval).

Audited tree: `main` at `8c90a8a5`. Test machine: Ubuntu 22.04.5, Chrome for
Testing 156, Xvfb, XFCE 4.16, FFmpeg 4.4.2, bubblewrap 0.6.1.

## How the model reaches each surface

| Host | Browser | Desktop |
|---|---|---|
| Standalone Glass Core (`gc`, `glass-core run`) | No typed tools. The system prompt and `guidance.rs` tell the model to run the `glass-browser` CLI through `bash`, and to use `web_fetch` for static reads. | Six `desktop_*` tools, offered only while a Connect desktop manager runs for the workspace (`Agent::tool_set_for_desktop`). |
| Glass (`glass-dev`) mixed mode | 19 `glass.browser.*` host tools from `crates/glass-dev/src/tools.rs`. | Same `desktop_*` tools through the resident Core. |
| Any MCP client, including Core's optional `mcp-bridge` extension | `glass-browser --mcp` serves 101 camelCase tools over stdio. The bridge switches to `mcp_search`/`mcp_call` above 40 tools, so a model never sees these schemas directly. | Not exposed. |

The same capability therefore has three spellings (`click`, `glass.browser.act`
with `action:"click"`, `bash: glass-browser click --name …`) and none of them
uses the `desktop_*` naming that computer use already has.

## Browser tools today

### `glass-browser` runtime

* Runtimes: `native` (default, Glass-owned engine with a sandboxed content
  worker), `chromium` (CDP), `firefox` and `safari` (WebDriver BiDi/WebDriver).
* Locators (`browser/session/types.rs`): `Reference` (observation ref),
  `AccessibleName`, `RoleAndName`, `Text`, `Css`, `Ordinal`.
* Wait conditions: lifecycle, exact/prefix URL, title, target
  attached/visible/hidden/enabled/stable, text, semantic region, JavaScript,
  network quiet.
* Actions: click, click-at (viewport pixels), double-click, hover, drag, key,
  key-down/up, shortcut, clear, check/uncheck, select, upload, type, scroll.
* Observation: visible text, deep DOM, compact accessibility `observe`,
  `inspectPage`, Web IR, screenshots (PNG), form values.
* Session: navigate/back/forward/reload/stop, targets (tabs) list/create/
  select/close, frames, popups (`clickExpectPopup`), dialogs (accept/dismiss),
  downloads, cookies and storage, clipboard read/write, emulation.

### MCP catalog (`src/mcp/server.rs`)

101 tools, one per runtime operation plus workspace, memory, knowledge,
replay, Web IR and diagnostics tools. Descriptions are one sentence that say
what a tool does, never when to prefer it or what to use instead. Locators are
passed as six optional mutually exclusive properties
(`ref`/`name`/`role`/`text`/`css`/`ordinal`) without examples. Errors are
returned as plain runtime messages.

### Glass mixed mode (`glass.browser.*`)

`start`, `stop`, `attach`, `reconnect`, `state`, `observe`, `snapshot`,
`semantic`, `web_ir`, `diff`, `targets`, `target.select`, `navigate`, `act`
(`back`, `forward`, `reload`, `stopLoading`, `click`, `type`, `scroll`),
`screenshot`, `verify`, `remote-view.open|status|revoke`. There is no typed
key press, select, wait, upload, download, dialog or form-fill tool here.

### Standalone Core path (`bash` + CLI)

The prompt asks the model to discover the CLI with `command -v glass-browser`
and `--help` (60+ subcommands), start `glass-browser --browser-runtime native
session start NAME`, and repeat `--browser-runtime native --session NAME` on
every call. Each call is a separate process; bare commands are stateless.

## Computer-use tools today (`glass-core`)

| Tool | Behaviour |
|---|---|
| `desktop_status` | Session ID, display, workspace, frame freshness, lease. |
| `desktop_screenshot` | Fresh frame saved as a workspace PNG; the model must then call `view_image`. |
| `desktop_observe` | Bounded AT-SPI snapshot (apps, windows, controls) with a revision, optional `windowId`/`query`. |
| `desktop_act` | `click`, `focus`, `set_text` on a revision-bound accessibility target. |
| `desktop_input` | Raw XTest events: pointer move/click in normalized 0–1 coordinates, scroll, key, key chord, text (≤512 bytes). |
| `desktop_launch_app` | Launch an allowlisted desktop application by ID. |

Implementation: Xvfb display per session (`desktop/process.rs`), XFCE or GNOME
(`gnome-shell --x11`) inside a private D-Bus and AT-SPI bus, FFmpeg `x11grab`
frames at 15 fps (`desktop/capture.rs`), XTest loaded with `dlopen`
(`desktop/input.rs`), and an input lease shared with the human through Connect.
The six schemas total about 18.6 KB; `desktop_observe` alone carries a
paragraph-length description.

## Gaps against strong agents

Severity: **P0** breaks tasks on a stock Linux machine, **P1** costs steps or
causes wrong tool choice, **P2** is polish.

| Area | Finding | Severity |
|---|---|---|
| Linux runtime | Default `native` browser runtime fails on Ubuntu 22.04: the content worker is started with bubblewrap `--disable-userns`, which needs bubblewrap ≥ 0.8; 0.6.1 exits and the error is reported only as a generic IPC failure. Core's guidance hard-codes `--browser-runtime native`. | P0 |
| Linux desktop | Desktop capture passed `-fps_mode`, which needs FFmpeg ≥ 5.1; Ubuntu 22.04 ships 4.4, so every managed desktop failed with "Desktop capture did not produce a fresh frame". Fixed with a version probe in the eval milestone. | P0 |
| Discoverability | Standalone Core has no typed browser tools. The model must know the CLI exists, read a long `--help`, and remember session flags on every call. | P0 |
| Discoverability | Three inconsistent browser spellings and a 101-tool MCP catalog hidden behind `mcp_search`. Descriptions never say when to use or avoid a tool; no schema examples. | P1 |
| Session state | `session start NAME --browser-runtime chromium` still records the native runtime; persistence depends on the model passing `--session` every time. | P1 |
| Navigation | Present in all surfaces. Core path relaunches the browser per call unless a session is used. | P1 |
| Waiting | Rich wait conditions exist in the runtime and MCP, but the Glass `act` surface has none and the CLI requires knowing the condition grammar. | P1 |
| Accessible text and snapshots | `observe`, `text`, `inspectPage` are strong. Refs are only valid for one observation revision, which the model learns from errors. | P2 |
| Element targeting | Role/name/text/CSS/ordinal locators exist; no label-for lookup as a named locator (labels resolve through accessible name). | P2 |
| Forms | `fillForm` exists in MCP only; Glass and Core models fill field by field. | P1 |
| Uploads and downloads | Present in runtime and MCP; absent from Glass `glass.browser.*`. Download location is not reported to the model in a predictable place for Core. | P1 |
| Tabs | MCP and Glass expose targets; popups need `clickExpectPopup`, which a model rarely picks. | P1 |
| Dialogs | `acceptDialog`/`dismissDialog` only in MCP. A blocking `confirm()` stalls other tools without telling the model which tool clears it. | P1 |
| Screenshots with coordinates | Browser `clickAt` uses CSS pixels; desktop `desktop_input` uses normalized 0–1 coordinates, and screenshots do not state the scale. Models clicking from a screenshot must convert. | P1 |
| Scrolling | Browser scroll by pixels; desktop scroll by wheel steps. No "scroll until text visible". | P2 |
| Keyboard | Browser `key`/`shortcut`; desktop requires `Control`, rejecting common aliases (`Ctrl`, `Cmd`, `Super`, `Win`, `Esc`). | P1 |
| Clipboard | Browser `clipboardRead/Write` in MCP only. No desktop clipboard tool at all. | P1 |
| Error recovery | Desktop errors are good ("call desktop_observe again"). Browser errors rarely name the next tool to call. | P1 |
| Linux X11 | Fully X11: Xvfb, XTest, x11grab, `gnome-shell --x11`. Works headless once dependencies are present. | — |
| Linux Wayland | No Wayland backend. Managed sessions always run their own Xvfb, so they work on a Wayland host, but attaching to a Wayland desktop, `wtype`/`ydotool` input, or PipeWire capture are not supported and the status does not say so. | P2 |

## Prioritized plan

1. **Make Linux work out of the box (P0).** FFmpeg 4.x capture fix; headless
   `gc desktop quick|status|stop` for scripted setup; bubblewrap capability
   probe with an actionable error that names `--browser-runtime chromium`;
   honour `--browser-runtime` in `session start`.
2. **One discoverable browser tool family (P0/P1).** A curated agent profile of
   `browser_*` tools in `glass-browser --mcp --tool-profile agent`
   (navigate, snapshot, click, type, fill_form, select_option, press_key,
   scroll, wait_for, screenshot, click_at, tabs, handle_dialog, upload_file,
   download, back/close), each with when/when-not text, tight schemas with
   examples, and errors that name the next tool. Real-Chromium integration
   tests.
3. **Typed browser tools in standalone Core (P0).** Core starts the agent
   profile as a persistent child over MCP when `glass-browser` is installed and
   offers the `browser_*` tools natively, so the model needs no CLI knowledge
   and state persists across calls.
4. **Computer-use discoverability (P1).** Shorter `desktop_*` descriptions in
   the same when/when-not style, key aliases, pixel coordinates matching the
   screenshot, a `desktop_clipboard` tool, explicit X11/Wayland reporting in
   `desktop_status`, and Xvfb integration tests.
5. **Later (P2).** Scroll-until-visible, Wayland host attach (PipeWire portal
   capture, libei input), consistent `glass.browser.*` aliases in Glass.

## Tool-selection eval

`scripts/eval/browser-cu/run_eval.py` runs realistic tasks through
`glass-core run --json` against the configured provider and a local fixture
site (`fixture_server.py`). Prompts never name a tool. For each run it reads
the saved session and records:

* **right first tool**: the first tool call belongs to the task's family
  (browser or desktop) or an allowed alternative such as `web_fetch` for a
  static read;
* **ideal tool**: the specialised tool for the capability was used;
* **success**: checked from fixture-server state, the managed desktop
  (clipboard, windows), or the final answer, never from the model's claim
  alone;
* **steps**: tool calls in the run.

Tasks cover reading an article, a labelled form, delayed content, upload,
download, a new tab, a confirm dialog, infinite scroll, a canvas click from a
screenshot, a keyboard-driven combobox, and four desktop tasks (calculator,
describe screen, clipboard, text editor). Ten later tasks cover the tools added
since: finding text on a long page, extracting a table, a console error, a
failed request, a cookie banner over a form, saving a PDF, pasting long text on
the desktop, listing windows, recording the desktop, and an OSWorld-style task
that saves a file in the desktop's Downloads and copies it into the workspace.
File outputs are checked with `workspace_file` (a glob, plus optional
`starts_with` or `contains`).

Run it:

```console
cargo build -p glass-core -p glass-browser --bins --locked
cd "$EVAL_WORKSPACE" && gc desktop quick --json   # only for desktop tasks
python3 scripts/eval/browser-cu/run_eval.py \
  --glass-core target/debug/glass-core --glass-browser target/debug/glass-browser \
  --workspace "$EVAL_WORKSPACE" --data-dir "$XDG_DATA_HOME" \
  --label before --out /tmp/before.json
```

Results are recorded per milestone in the pull requests.
