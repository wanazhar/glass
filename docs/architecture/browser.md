# Browser data plane

Status: Accepted

## Purpose

Define the lowest-cost correct browser contract shared by CLI, MCP, and TUI.

## Observation contract

`observe()` returns a compact `PageContext`:

- page URL, title, and ready state;
- visible text capped to a UTF-8-safe 16 KiB byte budget;
- accessibility roots and interactive controls with a snapshot revision, bounded to 128 nodes,
  32 controls, and 4 KiB of UTF-8-safe AX label text (roles are capped at 64 bytes);
- no `dom` field and no screenshot by default.

`observe_with_dom()` explicitly adds the full DOM. `observe_with_screenshot()` explicitly adds pixels. Combining both is allowed only through an explicitly named method/tool option.

The cache stores compact context only. Deep DOM and screenshot data are never cached as the default page state.

## Agent frontend contract

CLI `observe` and MCP `observe` serialize `PageContext` as compact, single-line
JSON. MCP uses `includeDom` and `includeScreenshot`; CLI uses `--deep-dom` and
`--screenshot`. Both flags default to `false`, and a supplied non-boolean value
is rejected rather than silently treated as `false`.

`getDOM`/`dom` is an explicit deep-DOM operation and returns a serialized
`DomNode`, not an unbounded accessibility snapshot. `getText`/`text` returns
the same UTF-8-safe 16 KiB-bounded visible text used by compact observation.
Input actions serialize `ActionOutcome`; navigation serializes `PageInfo`.
Screenshots remain a distinct image response in MCP and an explicit file or
structured-context request in CLI.

## Element references

Interactive controls expose `r<revision>:b<backend-node-id>` references, and
compact accessibility snapshots publish the matching revision. A reference from
a prior revision must fail with a stale-reference error instead of selecting a
newly numbered control.

String targets use explicit locator forms: `ref=<reference>`, `name=<accessible
name>`, `role=<role>;name=<accessible name>`, `text=<text>`, `css=<selector>`,
and `ordinal=<one-based index>`. A bare revisioned reference remains accepted as
the fast agent path, and a bare string remains an exact accessible-name lookup
for command-line compatibility. Role-only and substring-name lookup are not
accepted. Every strategy resolves to exactly one element or returns a bounded
ambiguous/not-found diagnostic; CSS and text lookup never select the first DOM
match silently.

Text lookup normalizes whitespace and matches exact rendered `innerText` only.
Hidden or zero-area elements are excluded, nested text is promoted to its
nearest interactive owner, and duplicate visible owners are ambiguous. CSS and
text discovery return at most the bounded diagnostic prefix across CDP even
when the page contains more matches.

## Action contract

Fast actions avoid implicit waits. A click resolves a target, asks Chrome to
scroll it into view only when required, dispatches the configured pointer mode,
invalidates the snapshot revision, and returns a serializable action outcome
with action kind, target, and resulting revision. Navigation or state waiting
is an explicit operation. Double-click uses the same contract; drag remains an
internal mouse-engine primitive until it has an equally reliable target/viewport
contract.

Human mode keeps the existing Bézier pointer path and dwell timing. Fast mode keeps direct CDP input events and does not sleep between movement samples.

Immediately before pointer dispatch, Glass samples target geometry, rejects
active element animations, and verifies that the node is connected, rendered,
enabled, inside the viewport, and owns the center hit point (a descendant may
own the point). Detached, moving, disabled, off-viewport, or overlaid targets
fail without sending pointer input. This check adds no hidden frame delay to
the fast interaction mode.

After human or fast pointer movement, Glass revalidates the same node and
center immediately before `mousePressed`. A hover-triggered overlay, detach, or
geometry change therefore fails before button input. Once pressed, Glass
releases the button at the dispatched point. A drop guard issues a best-effort
release if cancellation interrupts the press/dwell/release sequence so Chrome
is not left with a stuck button.

## CDP data path

- `DOM.getDocument(depth: -1)` is deep-DOM-only.
- CSS selector lookup fetches only the document root.
- Default CDP event broadcasts are method-only; payload delivery requires an explicit subscription.
- Unused CDP domains are not enabled.
- Network domain activation is not a default observation requirement.

## Browser lifecycle and persistence

| Mode | Chrome flags/data | Cleanup |
|---|---|---|
| named profile | `--user-data-dir=<Glass profile dir>` | retained until delete-profile |
| incognito | `--incognito` plus unique temporary `--user-data-dir` | removed when Glass-owned Chrome exits |
| attach | `--attach`; no launch; explicit endpoint/target | Glass closes only the CDP connection |

`profiles list`, create, and delete operate on profile directories. Deletion removes the profile directory and any legacy metadata together.

Library callers must invoke `BrowserSession::close()` when they finish an
owned incognito session. That explicit asynchronous path stops Chrome before
removing its disposable directory. Implicit `Drop` initiates process shutdown
as a best effort only; on platforms that lock open browser files it can leave
cleanup for a later pass. Improving abnormal-shutdown cleanup is tracked in
the delivery backlog.

For an owned session, `close()` first makes a bounded best-effort `Browser.close`
request so Chrome can flush named-profile state. Glass then waits briefly for
the child to exit and falls back to process termination if it does not. Attached
sessions only close their CDP connection.

`--attach` is intentionally narrow: it may not be combined with `--incognito`,
`--chrome-path`, `--headed`, or a non-default `--profile`. The default profile
value is ignored for compatibility because an attached Chrome instance owns its
own profile. Chrome resolution for owned sessions is explicit path first, then
the build installed by `install-chromium`, then system Chrome/Chromium.

## Errors and fallbacks

- An occupied CDP port without explicit attach is an error.
- Multiple page targets in any mode without an explicit target ID are an error.
- Missing required CDP fields, invalid element references, and stale references are explicit errors.
- MCP tool arguments with an invalid type are explicit errors and do not start a
  browser session.
- No operation silently falls back to a different browser profile or page target.

## Tests

Required coverage includes compact-vs-deep observation, bounded text, selector root lookup, stale references, fast and human motion, explicit attachment, incognito isolation, named-profile persistence/deletion, and managed Chromium resolution.
