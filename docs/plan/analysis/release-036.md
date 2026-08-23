# Glass v0.3.6 delivery analysis

Status: Active direct implementation. Issue
[#36](https://github.com/wanazhar/glass/issues/36) is authoritative. It had no
comments when audited on 2026-08-12. The user authorized conventional commits,
push, publication, release, and issue completion only after every gate passes.
Status: Historical record; superseded by the current 0.3.12 source/release evidence. Issue #36's completed gate review is recorded in [release-036-gates.md](../reviews/release-036-gates.md), and current release evidence is recorded in [release-evidence.md](../../release-evidence.md).

## Locked decisions

- One canonical `BrowserWorkspace` lives in `glass-browser` and is consumed by
  both the standalone browser TUI and Glass Dev `App` surface.
- Structured semantic state remains the default automation context;
  screenshots and pixels are explicit presentation/evidence paths.
- Normal TUI actions carry the displayed browser revision automatically.
- Pi remains the only embedded agent SDK. Cargo never silently installs global
  Node packages; Glass owns an explicit doctor/setup/status lifecycle.
- The daemon returns operation IDs for long work and supports reconnect,
  inspection, bounded events, cancellation, and reconciliation.
- Agent settlement and verified task success remain different states.
- Desktop and phone share state but use different information architecture.
- Package ownership remains `glass-dev -> glass-browser` one way.

## Audited baseline

The post-0.3.5 `main` branch already retains and joins Pi worker handles, but
the broader epic remains reproducible:

- daemon `workspace.tool` waits behind a fixed ten-second client timeout and
  has no recoverable operation lifecycle;
- `VerificationRequirement::default()` is `Settled`;
- Pi discovery ends in an environment-variable/package error and has no
  doctor/setup/status model;
- standalone browser mouse/focus/paste capture is enabled while the event loop
  handles only keys;
- standalone browser and embedded development browser are unrelated TUI paths;
- both primarily pretty-print browser/runtime JSON;
- Glass Dev exposes eighteen equal top-level architecture surfaces;
- the editor, Git, process, test, LSP, DAP, and kernel views are diagnostic
  strings rather than interactive work surfaces;
- phone tests assert labels, not completion of usable flows;
- existing terminal graphics, browser recovery, editor, graph, and resident
  service code is not wired into one current product experience.

## Requirement matrix

| Pillar | Baseline | Delivery evidence |
|---:|---|---|
| 1 worker lifecycle | partial post-tag fix | attributed panic, terminal join semantics, repeated cleanup/leak test |
| 2 daemon operations | missing | durable operation store/API/events/cancel/reconcile and reconnect test |
| 3 verification | unsafe settle default | inferred/explicit policy and visible unverified state |
| 4 Pi setup | missing | doctor/setup/status CLI, managed runtime metadata, readiness TUI |
| 5 docs truth | stale phrases/keys | forbidden-phrase validator and exact behavior docs |
| 6 shared browser | missing | one exported controller/view consumed twice |
| 7 live visuals | unwired | latest-frame presentation and explicit quality/degradation |
| 8 desktop browser | command/JSON shell | controls, focus, input, screenshot/workflow/recovery routes |
| 9 semantic sidecar | raw observation JSON | bounded entities, selection, highlight, delta, stale state |
| 10 parity | service APIs only | shared typed capability inventory and adapter tests |
| 11 recovery | service reconnect only | in-place collision/attach/auto-port/target recovery UI |
| 12 information architecture | 18 equal views | seven desktop destinations and contextual More |
| 13 modes | absent | Build/Agent/Run-App/Debug state projection |
| 14 Agent chat | ID/status centric | default conversation, composer, tools, confirmation, history |
| 15 editor | backend exists | cursor/edit/save/undo/redo/search/diagnostic interaction |
| 16 Git | JSON | branch/files/diff/stage/commit/push confirmations |
| 17 runtime views | JSON | process/test/LSP/DAP/kernel designed projections/actions |
| 18 context pane | fixed unrelated dump | focus-sensitive selected evidence/actions |
| 19 pane input | global surface cycling | focus, local scroll, mouse, resize, paste, editor/browser input |
| 20 phone nav | 18-view cycle | Agent/Code/App/Tasks/More |
| 21 phone Agent | status text | direct conversation flow and approvals |
| 22 phone App | JSON browser | semantic-first App with deliberate visual mode |
| 23 mobile tests | label assertions | scripted reachable flows at 48x18, 64x24, 80x24 |
| 24 onboarding | dashboard dump | environment detection and direct next actions |
| 25 palette | append-only parser | fuzzy/context/history/completion/cursor/error UX |
| 26 Agent-browser | disconnected projections | shared selection/context/lease/reconcile path |
| 27 Code-App | graph backend only | known source/runtime links navigable in UI |
| 28 App mode | Browser diagnostic surface | first-class App destination using BrowserWorkspace |
| 29 projections | widespread pretty JSON | explicit cards/tables; raw behind Inspect |
| 30 visual language | partial | consistent status vocabulary in every major projection |
| 31 terminal compatibility | incorrect mouse capture | processed modes, cleanup, monochrome/fallback tests |
| 32 desktop tests | labels | interaction, focus, scroll, selection, composer, resize tests |
| 33 browser parity tests | absent | one contract suite over both shells |
| 34 PTY tests | browser cleanup only | browser and Dev input/paste/mouse/resize/cleanup smokes |
| 35 remote/mobile proof | simulation overclaimed in docs | exact local/SSH/tmux/simulation evidence boundaries |

## Serial delivery order

1. Correctness foundations: workers, daemon operations, verification, Pi
   lifecycle, and semantic documentation assertions.
2. Canonical BrowserWorkspace state/action/view contract and both adapters.
3. Live/semantic browser rendering, input ownership, workflows, targets, and
   recovery.
4. Glass Agent, editor, App, Git/runtime projections, focus, and desktop shell.
5. Purpose-built phone shell, onboarding, palette, and cross-surface context.
6. Interaction/parity/PTY/live-browser certification, synchronized 0.3.6
   packaging and exact-tag release evidence.

## Release policy

Every checkpoint receives a focused conventional commit. Final validation uses
the complete locked workspace suite, strict Clippy/rustdoc, browser smoke,
package/publish dry runs, security/dependency/fuzz gates, clean install and
upgrade tests, and exact-tag CI. `glass-browser` publishes before `glass-dev`.
The crates, signed tag, GitHub Release, remote CI, registry installs, and issue
closure occur only after the local candidate is complete.
