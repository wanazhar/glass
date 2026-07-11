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

## Element references

Interactive controls expose `r<revision>:b<backend-node-id>` references, and
compact accessibility snapshots publish the matching revision. A reference from
a prior revision must fail with a stale-reference error instead of selecting a
newly numbered control. Accessible-name and CSS-selector lookup remain
convenience paths.

## Action contract

Fast actions avoid implicit waits. A click resolves a target, asks Chrome to
scroll it into view only when required, dispatches the configured pointer mode,
invalidates the snapshot revision, and returns a serializable action outcome
with action kind, target, and resulting revision. Navigation or state waiting
is an explicit operation. Double-click uses the same contract; drag remains an
internal mouse-engine primitive until it has an equally reliable target/viewport
contract.

Human mode keeps the existing Bézier pointer path and dwell timing. Fast mode keeps direct CDP input events and does not sleep between movement samples.

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

`--attach` is intentionally narrow: it may not be combined with `--incognito`,
`--chrome-path`, `--headed`, or a non-default `--profile`. The default profile
value is ignored for compatibility because an attached Chrome instance owns its
own profile. Chrome resolution for owned sessions is explicit path first, then
the build installed by `install-chromium`, then system Chrome/Chromium.

## Errors and fallbacks

- An occupied CDP port without explicit attach is an error.
- Multiple page targets in any mode without an explicit target ID are an error.
- Missing required CDP fields, invalid element references, and stale references are explicit errors.
- No operation silently falls back to a different browser profile or page target.

## Tests

Required coverage includes compact-vs-deep observation, bounded text, selector root lookup, stale references, fast and human motion, explicit attachment, incognito persistence, named profile deletion, and managed Chromium resolution.
