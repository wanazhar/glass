# Canonical Browser Workspace

Status: Implemented for 0.3.6

## Purpose and boundary

`BrowserWorkspace` is the single human-facing browser presentation and
interaction model used by the standalone `glass-browser` TUI and the embedded
Glass Dev `App` surface. It lives in `glass-browser`; `glass-dev` depends on it
one way and supplies its resident browser service as an execution backend.

The workspace does not replace `BrowserSession`, weaken revision guards, make
screenshots implicit automation context, or move development state into the
browser crate.

```text
BrowserSession / Glass Dev BrowserService
                  │
                  v
       BrowserWorkspaceController
        state · actions · selection
                  │
                  v
         BrowserWorkspaceView
             ┌────┴────┐
             │         │
       glass-browser  glass App
```

## State ownership

The controller owns bounded presentation state:

- connection phase, generation, ownership, endpoint summary, and recovery;
- page title, redacted URL, loading state, target summaries, and selected target;
- current browser and geometry revisions;
- current semantic projection, selected entity, semantic delta, and invalidation;
- newest visual frame metadata and presentation quality, never frame history;
- workflow summary and explicit evidence references;
- input owner (`Glass`, `Human`, or `Agent`) and takeover/reconcile state;
- focus, scroll, palette, address composer, sidecar, and transient errors.

An action created from a visible entity carries the displayed browser revision
automatically. A stale result invalidates the selection and requests a fresh
observation. Manual revisions and raw references remain expert/API inputs only.

## Desktop layout

```text
┌ GLASS BROWSER / APP ─ title ─ rev ─ visual quality ─ authority ───────────┐
│ ←  →  ↻  ■   address................................  ● Connected  Targets │
├────────────────────────────────────────────────────┬───────────────────────┤
│                                                    │ UNDERSTANDING         │
│ LIVE BROWSER                                       │ ◆ selected entity     │
│ newest terminal-native frame or explicit fallback │ ○ actionable entity   │
│                                                    │ forms · frames · diff │
├────────────────────────────────────────────────────┴───────────────────────┤
│ Inspect · Semantic · Workflow · Screenshot · Remote View       : Commands │
└────────────────────────────────────────────────────────────────────────────┘
```

The visual pane uses Herdr, probed Kitty/Sixel where implemented, bounded ANSI,
or a semantic-only fallback. The status always names the selected path and its
degradation reason. Pixels use a latest-frame-only mailbox and cannot block
semantic, control, process, or agent events.

## Phone layout

```text
┌ APP ─ title ─ rev ─ semantic ──────────┐
│ [O] Overview [S] Semantic [V] Visual   │
│ ◆ Place Order                         │
│   button · actionable · confirmed     │
│ ○ Promo code                          │
│   textbox                             │
│                                       │
│ [N] navigate [R] reconnect [T] targets│
├───────────────────────────────────────┤
│ Agent    Code    App    Tasks    More │
└───────────────────────────────────────┘
```

Phone defaults to semantic presentation. Visual mode is a deliberate bounded
burst or supported local stream, never an assumption based only on width or
`TERM`.

## Interaction contract

| Input | Scope | Behavior |
|---|---|---|
| `:` | workspace | open searchable contextual command palette |
| `n` / address action | controls | edit and submit navigation URL |
| `Alt-Left` / `Alt-Right` | controls | back / forward |
| `Ctrl-R` | controls | reload |
| `Tab` / `Shift-Tab` | workspace | move focus between controls, visual pane, sidecar, and footer |
| arrows or `j`/`k` | sidecar | move semantic selection and keep it visible |
| `Enter` | selected entity | execute revision-bound default action or open details |
| mouse click | owned pane | select semantic region or forward a revision-bound pointer action |
| wheel / page keys | owned pane | scroll the focused Glass list or browser, never change global surfaces |
| printable keys | human browser ownership | forward intentional keyboard input |
| `Esc` | browser ownership/overlay | return input to Glass or close top overlay |
| paste | composer/address | insert one bounded normalized paste event |

Mouse capture is enabled only while events are processed. Human and agent
mutation ownership are mutually exclusive. Taking control pauses agent browser
mutation until a checkpoint is reconciled and control is explicitly returned.

## Operation parity

Both shells route the same typed action inventory: start, stop, reconnect,
state, observe, snapshot, semantic, diff, targets, select target, navigate,
back, forward, reload, stop loading, click, type, scroll, screenshot, workflow
list/run/pause/resume/cancel/verify, and Remote View open/status/revoke when
available. Unsupported backend capabilities are visible disabled actions with
a reason; a shell must not silently omit them.

The standalone command area exposes `targets`, `select ID`, `state`,
`reconnect`, `attach PORT`, `launch auto`, `launch PORT`, `stop`, `screenshot`,
and `live on|off`. Embedded App routes the same operations through the resident
browser service, including loopback Remote View open/status/revoke. Remote View
normalizes coordinates against the live viewport and rejects stale revision
input before CDP mutation.

## Recovery

Connection failure never exits the TUI or destroys project, agent, editor,
process, or task state. Port collisions distinguish compatible DevTools,
unrelated listeners, and unknown endpoints. The recovery sheet offers bounded
probe, attach, automatic free-port launch, explicit port selection,
semantic-only continuation, and retry.

## Tests

- Run one reducer/behavior suite against standalone and embedded adapters.
- Render desktop, compact, 48x18, 64x24, and 80x24 layouts.
- Exercise focus, scrolling, palette, address editing, selection visibility,
  automatic revision propagation, stale invalidation, takeover, and recovery.
- PTY smoke terminal entry/cleanup, key input, mouse input, paste, resize, and
  `Ctrl-C` where the host supports them.
- Live Chromium scenarios certify pixels, semantic selection, target changes,
  actions, workflow, reconnect, and port collision without broadening remote or
  iOS evidence claims.
