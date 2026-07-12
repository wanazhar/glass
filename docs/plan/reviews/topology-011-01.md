# Review: topology-011 browser targets and frames

Reviewed commit: `9b8fb4f` (`feat: add explicit browser topology`).

Conclusion: **blocked**. The bounded registry and explicit selection commands
are a useful scaffold, but routing identity is mutable global client state,
frame pointer actions are not coordinate-correct, and the required lifecycle
and frontend integrations are incomplete. The task appropriately remains
`in-progress`.

## Findings

### P1 — blocking: routing identity is global mutable state, not carried by operations

`CdpClient::send` reads one process-wide `active_session`; evaluation and
element queries separately read `active_context`/`active_frame`
(`src/browser/cdp.rs:265-288`, `327-346`, `374-383`, `476-493`, `601-620`).
`select_target` and `select_frame` mutate those globals while all
`BrowserSession` methods take `&self` and remain concurrently callable
(`src/browser/session.rs:740-788`, `831-852`). An in-flight wait, observation,
locator, or action can therefore issue its first CDP command to one target and
its next command to a newly selected target/frame.

The three fields are not changed atomically. `set_active_session` stores the
new session and then clears context/frame through two more locks. Evaluation
can combine a context captured from the old frame with a session captured from
the new target. Conversely, selection rollback restores only the session via
`set_active_session(old_session)`, which clears the old frame/context while the
registry still says that frame is selected (`src/browser/session.rs:759-770`).

This violates the contract that commands *carry* explicit target/frame
identity. Capture one immutable route handle at operation start and pass it to
every routed CDP request, or serialize selection and all routed operations
behind a route owner. Add adversarial select-during-wait/observe/action tests
and atomic rollback tests. MCP's outer mutex happens to serialize its calls but
does not repair the public library contract.

### P1 — blocking: selected-frame pointer actions use child coordinates as top-level coordinates

Frame evaluation executes in the selected frame context, so
`getBoundingClientRect()` and `document.elementFromPoint()` in the hit-test
function produce coordinates relative to that frame's viewport. Input dispatch
still sends those coordinates through the selected *page target* session
(`src/browser/session.rs:831-852`; pointer path at `src/browser/session.rs:1180-1270`).
For an iframe not located at top-left, Glass clicks the corresponding top-page
coordinate rather than translating through the frame owner chain. Cross-origin
and nested offsets compound the error.

The real-Chrome topology test only evaluates text in the cross-origin frame; it
never locates or clicks a frame control (`tests/browser_smoke.rs:1295-1310`).
Implement coordinate translation through each ancestor frame or route input
through the correct flattened OOPIF session with documented coordinate
semantics. Test click/type in offset same-origin, nested, and cross-origin
frames and assert no top-page element receives the action.

### P1 — blocking: flattened OOPIF routing is not modeled

Startup and target selection call `Target.attachToTarget(flatten: true)` only
for page targets (`src/browser/session.rs:599-620`, `748-758`). Glass never
enables `Target.setAutoAttach` with flattened child sessions and retains no
frame-to-session mapping. `select_frame` always sends
`Page.createIsolatedWorld` through the active page session and stores only an
execution-context ID (`src/browser/session.rs:831-851`). This is not the
documented cross-origin routing model for out-of-process iframes; an OOPIF can
have its own target session and execution-context namespace.

The test's successful cross-origin evaluation on the current Chrome/process
layout does not prove OOPIF routing and may pass when site isolation does not
place that fixture in a separate process. Enable/discover flattened child
sessions, associate frame IDs with session IDs, handle attach/detach events,
and force an actual OOPIF in integration coverage.

### P1 — blocking: operation results do not expose target/frame identity

The architecture requires every primitive outcome to contain target/frame
identity, but existing `PageInfo`, `PageContext`, `ActionOutcome`, wait results,
and evaluation results were not extended. Selection only mutates hidden global
state. After an event or concurrent selection, an agent cannot verify which
page/frame produced an observation or received an action. Revision references
also contain only page revision/backend-node ID, so references from different
targets can collide semantically.

Add stable target/frame handles to observations and action/wait/navigation
outcomes and scope revisions/references to that route. Reject a reference whose
route differs from the operation route.

### P1 — blocking: CLI selection is not persistent across commands

The CLI exposes `select-target` and `select-frame` as standalone subcommands,
but normal CLI dispatch creates one `BrowserSession`, runs exactly one command,
and exits. A later `glass evaluate`, `observe`, or `click` invocation starts a
new session and loses the selection. Therefore the documented promise that
selection routes “subsequent” CLI commands is not usable in the ordinary CLI
surface (`docs/cli.md:49-54`, `src/cli/runner.rs:129-143`).

Provide a persistent CLI session/batch handle, accept explicit target/frame IDs
on every routed CLI command, or clearly limit topology selection to MCP/TUI.
Add an end-to-end CLI discovery-select-frame-action chain as required by the
task.

### P2 — blocking: frame lifecycle invalidates all frame state indiscriminately

Any `frameAttached`, `frameNavigated`, or `frameDetached` event in the active
session clears the entire cached frame tree and selected frame
(`src/browser/session.rs:2260-2280`). A sibling iframe navigation silently
deselects an unrelated active frame. The contract instead says navigation
invalidates only the affected frame subtree. The event path also records no
parent/session/context mapping, so it cannot distinguish selected-frame loss
from unrelated lifecycle changes.

Update the bounded tree incrementally or resync while preserving selection when
the selected frame still exists. Clear selection only when its frame/subtree is
detached or invalidated, and test sibling navigation plus nested subtree
detach.

### P2 — blocking: target-session detach lifecycle is unhandled

The registry handles target created/info/destroyed/crashed and page frame
events, but not `Target.detachedFromTarget`, `Target.attachedToTarget`, browser
process loss, or session-level crash/detach (`src/browser/session.rs:2208-2283`).
An active session can become invalid while `active_target_id` remains selected
and commands continue routing to a dead session. Detach errors from old
sessions during selection are ignored (`src/browser/session.rs:772-780`),
potentially retaining duplicate attached sessions and event traffic.

Track flattened attach/detach ownership, clear or resync active routing on
session loss, and cover crash/detach recovery. The required real crash scenario
is absent.

### P2 — blocking: retained IDs are not consistently bounded

Target IDs are validated, but `opener_id` is copied directly into retained
target entries without validation/truncation in list, event, and resync paths
(`src/browser/session.rs:707-712`, `2219-2227`, `2310-2317`). Rejected oversized
IDs are also retained in lifecycle summaries through
`bounded_topology_text(..., 1024 bytes)` rather than the documented 256-byte ID
cap (`src/browser/session.rs:2188-2205`). These violate the stated bound on every
retained ID. Apply `validate_topology_id` to opener/parent/session IDs and use a
separate 256-byte bounded/redacted event ID representation.

### P2 — non-blocking: topology errors are untyped and hidden by MCP

The architecture promises typed topology errors, but all topology failures are
string errors. MCP's hardened error surface converts them to generic
`browser tool failed`, so agents cannot distinguish no selection, stale target,
budget rejection, frame not found, crash, or routing loss. Introduce a bounded
`TopologyError` kind that MCP can serialize without echoing untrusted URLs or
raw oversized IDs.

### P3 — non-blocking: verification and resource evidence are far below the task gate

The only real-Chrome test covers popup discovery, manual target selection,
cross-origin evaluation, and active close. It does not cover `create_target`,
nested frames, same/cross-origin frame action, MCP or CLI chains, target crash,
session detach, event lag/resync, registry overflow, or one-page resource cost
(`tests/browser_smoke.rs:1235-1324`). Unit tests cover only popup non-selection,
active crash clearing, and a two-node frame tree. No latency, RSS, payload,
round-trip, or binary-size evidence is recorded in the task, whose status is
still in progress.

## Positive observations

- Page/frame/event collections have explicit numeric caps, and URL/title text
  uses UTF-8-safe truncation (`src/browser/session.rs:34-41`, `2170-2205`).
- Popup target discovery does not change active selection, and closing/crashing
  the active target clears selection instead of guessing another target
  (`src/browser/session.rs:795-816`, `2208-2255`).
- Browser-scoped commands use an explicit unrouted path, preventing
  `Browser.close`, target discovery, and target management from accidentally
  inheriting a page session (`src/browser/cdp.rs:265-288`, `729-733`).
- Frame-tree collection preserves parent IDs and rejects a 129th frame
  (`src/browser/session.rs:2150-2186`).

## Focused verification

Independent inexpensive checks passed:

- `cargo test browser::session::tests --lib` — 19 passed;
- `cargo test browser::cdp::tests --lib` — 8 passed;
- `cargo test mcp::server::tests --lib` — 14 passed;
- `cargo test cli::args::tests --lib` — 9 passed;
- `git diff --check 9b8fb4f^ 9b8fb4f`.

No implementation changes were made during this review.
