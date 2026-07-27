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

Compact collection snapshots the immutable target/frame route and page
revision at entry. It collects page state and accessibility concurrently,
checks the revision again, and retries once when they differ. A second race is
returned explicitly as `consistent: false` with the starting and ending
revisions; Glass never labels a mixed snapshot consistent. Each attempt is
bounded by a one-second internal collection deadline.

Every compact result includes target/frame identity and a bounded list of
incompleteness reasons. Reasons distinguish visible-text, accessibility-node,
accessibility-label, control, shadow-boundary, frame-boundary, canvas, and
mutation-race limits. Shadow roots and child frames are represented by bounded
boundary summaries rather than recursively expanding unrelated documents.

Visible text remains UTF-8 byte bounded. Replacing `innerText` with a lower
allocation traversal is allowed only after browser tests prove equivalent
rendered-text semantics for hidden, clipped, generated, and whitespace-heavy
content; until then Glass reports truncation and avoids a semantically weaker
optimization.

The compact cache owns one serialized-equivalent set of strings and bounded
AX vectors for one route/revision. Callers receive clones; deep DOM, boundary
descendants, screenshots, and source CDP payloads are not retained.

## Frontend contract

CLI `observe` and MCP `observe` serialize `PageContext` as compact, single-line
JSON. MCP uses `includeDom` and `includeScreenshot`; CLI uses `--deep-dom` and
`--screenshot`. Both flags default to `false`, and a supplied non-boolean value
is rejected rather than silently treated as `false`.

`getDOM`/`dom` is an explicit deep-DOM operation and returns a serialized
`DomNode`, not an unbounded accessibility snapshot. `getText`/`text` returns
the same UTF-8-safe 16 KiB-bounded visible text used by compact observation.
Input actions serialize `ActionOutcome`; unguarded navigation serializes
`PageInfo`, while revision-guarded navigation returns `NavigationOutcome`.
Guarded actions include status, previous/current revisions, and bounded
verification evidence.
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
the fast path, and a bare string remains an exact accessible-name lookup
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
with status, action kind, target, previous/current revisions, and bounded
verification evidence. The optional expected revision is checked before the
action and produces a typed stale-revision failure without browser mutation.
Navigation or state waiting is an explicit operation. Double-click uses the
same contract. Hover moves without button events. Drag resolves and
revalidates both endpoints, presses only after source validation, revalidates
the destination before release, and uses the configured human or fast motion
path. Cancellation after press keeps the existing best-effort release guard.

Keyboard primitives use `Input.dispatchKeyEvent`: key-down and key-up are
independent operations, key press emits down/optional character/up ordering,
and shortcuts carry explicit modifier bits. Key names, shortcut length, and
text are bounded before CDP. `type` remains efficient text insertion;
contenteditable and browser-shortcut workflows use keyboard primitives.

`clear` focuses a unique actionable target and sends select-all plus Backspace.
Check/uncheck are idempotent only after verifying checkbox/radio state; select
matches one option value exactly and verifies the result. File upload accepts
a bounded list of canonical regular files, resolves one file input, uses
`DOM.setFileInputFiles`, and returns only file count—never paths or contents.
Paths are rejected unless they remain under the canonical process workspace
root; later safety profiles may narrow that boundary but cannot widen it
implicitly. Form operations revalidate immediately before the side effect and
verify post-state.

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

## Wait contract

`BrowserSession::wait` accepts one typed condition and an explicit deadline.
Public string forms are `lifecycle=<load|domcontentloaded|complete>`,
`url=<exact URL>`, `url-prefix=<prefix>`, `target-attached=<locator>`,
`target-visible=<locator>`, `target-hidden=<locator>`,
`target-enabled=<locator>`, `target-stable=<locator>`, `text=<visible substring>`,
`js=<predicate>`, and `network-quiet=<milliseconds>`. JavaScript predicates
must return a boolean. Deadlines are positive milliseconds and never default
to an unbounded wait.

Lifecycle and URL waits consume Page events before polling current state.
DOM-dependent conditions use bounded 50 ms polling because CDP does not expose
a reliable semantic event for every state transition. Text checks execute an
untruncated page predicate. Network quiet enables the Network domain only for
the first overlapping wait, tracks requests observed from activation onward in
a bounded set, fails conservatively on event lag/overflow, and disables the
domain after the last lease. Timeout results contain a typed condition,
deadline, bounded last state, and reason; dropping the wait cancels it without
leaving subscriptions or enabled domains behind.

## CDP data path

- `DOM.getDocument(depth: -1)` is deep-DOM-only.
- CSS selector lookup fetches only the document root.
- Default CDP event broadcasts are method-only; payload delivery requires an explicit subscription.
- Unused CDP domains are not enabled.
- Network domain activation is not a default observation requirement.

## Browser lifecycle and persistence

## Target and frame topology

A session owns one bounded topology registry: at most 32 page targets, 128
frames across all targets, and 64 recent lifecycle summaries. Each page is
identified by Chrome's target ID and each frame by its frame ID; IDs are capped
at 256 bytes before retention. The registry records opener, URL, title, active
state, parent frame, and last lifecycle state without retaining page content.

`list_targets`, `create_target`, `select_target`, and `close_target` are
explicit operations. Popup discovery adds a registry entry but never changes
the active target. Closing or crashing the active target clears selection and
requires an explicit subsequent selection; Glass never guesses another tab.

`list_frames` and `select_frame` are scoped to the selected target. The main
frame is explicit rather than represented by an absent value. Commands carry
the selected target/frame routing identity through the CDP actor. Same-process
and out-of-process cross-origin frames use flattened Target sessions and
execution-context routing rather than DOM access through a parent document.
Navigation invalidates only the affected frame subtree and its published
references.

Each public routed operation snapshots one immutable session/frame/context
route for its full async lifetime. Explicit selection atomically replaces the
route for later operations without redirecting commands already in flight.
OOPIF child sessions are auto-attached with flattened routing; pointer actions
translate frame-local hit-test coordinates through the selected frame owner.
Chrome reports that owner box in target-page coordinates, so a grandchild frame
does not accumulate already-absolute ancestor offsets; real nested and OOPIF
click tests enforce this coordinate contract.

Target/frame lifecycle tracking is activated once per BrowserSession. Registry
updates consume `Target.targetCreated`, `Target.targetDestroyed`,
`Target.targetCrashed`, `Page.frameAttached`, `Page.frameNavigated`,
`Page.frameDetached`, and popup opener metadata. Unknown, oversized, or
over-budget entries are rejected with typed topology errors; event lag forces
a bounded resynchronization through `Target.getTargets` and `Page.getFrameTree`.

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
