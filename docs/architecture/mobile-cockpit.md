# Remote development cockpit

Status: Released in 0.3.2

## Purpose and boundaries

The cockpit makes Glass resilient and actionable in a narrow SSH terminal. It
does not turn MCP into an image transport, expose a public relay, or replace
Herdr/Mosh/SSH as the transport owner. Structured browser state remains the
default and visual capture remains explicit.

## Runtime ownership

```text
MCP/TUI/client
     │ attach canonical project root
     v
ResidentDevelopmentSessions ── bounded LRU / idle expiry
     │ owns
     ├── ProjectWorkspace (buffers, revisions, actors, timeline)
     ├── managed PTYs and process output
     └── reconnect capsule (non-sensitive control state only)
```

A resident session is keyed by canonical project root. A registry owns at most
eight sessions and expires idle sessions after 30 minutes. Eviction stops
owned processes. Event and timeline reads remain direct, read-only persisted
queries so observation never creates activity. Capsules persist only project
identity, event cursor, selected mobile view, browser target/revision, pending
approval summary, and live-view preferences. They never persist command input,
prompt text, pixels, cookies, or page secrets.

## Phone layout

```text
┌─ Glass / project ───────────────────┐
│ NEEDS YOU (2)                       │
│ ! Agent approval · review edit      │
│ ! Tests failed · cargo test         │
├─────────────────────────────────────┤
│ RUNNING                             │
│ ● dev :3000 · agent working         │
├─────────────────────────────────────┤
│ RECENT                              │
│ ✓ semantic verification · rev 83    │
├─────────────────────────────────────┤
│ 1 Home  2 Agent  3 App  4 Diff  5… │
│ command >                           │
└─────────────────────────────────────┘

Semantic tap mode replaces unreliable pixel targeting with a bounded action
overlay in the App view:

```text
┌─ Semantic actions ──────────────────┐
│ [1] Open menu                       │
│ [2] Search                          │
│ [3] Add to cart                     │
│ [4] Continue checkout               │
└─────────────────────────────────────┘
```

## Interaction contract

| Input or command | Behavior |
|---|---|
| `inbox` | Open the bounded attention summary. |
| `notify on\|off\|status` | Opt in to a deduplicated terminal bell for new attention items. |
| `tap` | Show numbered actionable semantic targets. |
| `tap N` or number in tap mode | Resolve the current revision-bound target and click it. |
| `verify card` | Show the latest compact verification outcome. |
| `capsule save\|show\|clear` | Manage the non-sensitive reconnect capsule. |
| `live quality auto` | Adapt frame rate and size from delivery pressure. |
| `Esc` | Close tap/help/error overlays before leaving the current view. |

The inbox has loading, empty, error, and bounded overflow states. Items are
classified as `needsAttention`, `running`, or `recent`; no notification body
contains prompt text, process output, source content, URLs with secret-like
query values, or browser pixels. Optional terminal notification output is off
by default.

## Verification cards

A card is bounded structured evidence with a title, outcome, checks, changed
file count, semantic revision, and explicit visual status. Visual status is
`not-captured` until an explicit screenshot or comparison supplies evidence.

## Adaptive live view

`auto` starts at the balanced profile and adjusts only within the existing
data/balanced/smooth bounds. Sustained drops or slow delivery reduce quality;
a stable window increases quality. Hidden App views suspend capture. A manual
quality selection disables adaptation until `auto` is selected again.

## SDK workflows

TypeScript and Python expose bounded helpers for event waiting, health waits,
mutation-lease scopes, edit-and-verify, cursor resume, and attention callbacks.
Every helper accepts a deadline/cancellation mechanism and delegates actions to
the existing typed tool methods; it does not create a second protocol.

## Tests

- Registry tests cover root identity, reuse, LRU capacity, expiry, and cleanup.
- Capsule tests cover bounds, redaction-by-construction, atomic persistence,
  versioning, and removal.
- Inbox/card tests cover classification, limits, and sensitive payload refusal.
- TUI reducer tests cover tap overlay, phone focus, and adaptive-quality state.
- MCP and SDK integration tests traverse real resident state and cancellation.
