# Glass iOS · round 4

**Design proposal, not shipped behavior.** Direction in progress pending the owner's visual references.

**Choose Lens v1:** a circular orb on a bent horizon, with a teal rim. It keeps its silhouette at 16 pt and is the same mark on the icon, splash and empty chat. **Lens v5** is the alternate: its asymmetric shape is distinctive at icon scale but less legible when small. Earlier cut, prism and fold studies were explored and set aside because they do not read as lenses.

**Choose splash v4:** a Metal ripple bends the horizon lines behind the icon for 1.55 s. It starts at the orb, restarts at a tap, and can be skipped immediately; Reduce Motion bypasses the ripple. The striped **v1** treatment is an alternate motion field, not a second identity. Keep glass on controls and navigation; keep the transcript and its data on neutral, solid surfaces.

Open `Prototype/GlassRound4.xcodeproj` in Xcode 26 or later and run the GlassRound4 scheme on iOS 26+. Launch without arguments for the plate index or pass `--plate <state>` (for example, `screen-working`). There is no host dependency. Review the [icon studies](evidence/light/marks.png), [v4 storyboard](evidence/light/storyboard.png), [v4 recording](evidence/splash-ripple.mp4), and [v1 alternate](evidence/light/splash-v1.png). The [light](evidence/light) and [dark](evidence/dark) directories contain the visual system, components and all six iPhone 17 Pro mockups; see the [iPad conversation](evidence/ipad) for the split layout.

## Screen decisions

Each reference cell names what to take → what to leave out. The owner's light and dark sheets set the visual direction; native safe-area bars, SF type, Dynamic Type, and 44 pt controls replace the sheets' approximate geometry. The composer retains + and the model control, and the drawer uses a single card per section.

| Screen | ChatGPT | Amp | T3 Code | OpenCode | Owner direction and self-critique |
|---|---|---|---|---|---|
| Empty | Composer-first entry → promotional prompts | Quiet branded arrival → replaying motion in chat | Immediate task entry → project setup at entry | Low visual noise → terminal prompt | Keep the orb, one short prompt and two suggestions. The lens has room to read; suggestion wrapping needs another look at accessibility sizes. |
| Active | Gray user bubble and unbubbled answer → per-reply chrome | Folded work summary → trace as the first thing you read | Syntax and copy action → editor-sized code pane | Clean, scannable rows → terminal grid | Keep the single toolbar, clean table and one teal Send. The answer leads on phone; longer examples need horizontal room. On iPad the sidebar owns New Chat, leaving the long title to the detail toolbar; the wide transcript deliberately caps line length. |
| Working | Replace Send with Stop → separate progress banner | Live step and connected trace → expose every event | Name the current file/action → separate log pane | Distinct queued/done markers → dense event stream | Follow the light reference in both appearances: one vertical trace with a live spinner and one Stop. The live step is the focal point; the trace is a short sample, not an exhaustive log. |
| Failure | A clear next action → generic error toast | Resume in the thread → collapse the cause | Failed command and exit code → terminal-first recovery | Stderr on demand → raw output as the headline | Extend the active sheet rather than invent a new color system. Continue leads; the stderr sheet keeps the transcript quiet but costs a tap. |
| Drawer | Search and recency → undifferentiated history | Active-thread status → badges on every row | Quick thread switching → project suffixes | Jump by group → flat terminal list | Follow the dark panel in both appearances, with grouped sections, a jump index and 336 threads. Settings stays anchored; search or the index matters once month groups fill up. |
| Settings | Familiar grouped preferences → profile as the hero | Surface host health → diagnostics by default | Make defaults findable → expose every project flag | Keep device state explicit → config-file terminology | Carry the drawer's grouping and glass navigation into settings. The connected host leads; diagnostics stay one level deeper. |

## Round 3 defects → round 4 design

| # | Resolution and review target |
|---|---|
| 1 | Put the glass toolbar in the scroll view's top safe-area bar; apply the native scroll-edge effect. Text recedes under the toolbar instead of competing with the title. |
| 2 | Mount the same composer in new chat and in a thread; the empty state is a draft, not a dead end. |
| 3 | Symmetric 8 pt outer insets and 44 pt circular controls; center the send glyph. Keep the prompt on focus and use “Steer this reply” while writing. |
| 4 | Put model and live status on one slim rail inside the composer. Stop takes the send position; no separate working banners. |
| 5 | Send, Continue and Reconnect use white on deep teal in light (5.59:1) and deep ink on pale teal in dark (9.93:1). Teal text on the canvas is 5.21:1 light / 12.53:1 dark; on gray inset, 4.67:1 / 9.16:1. |
| 6 | Use a solid Jump to latest control with no sampled text. Only show it away from the bottom; give it its own slot above the composer. |
| 7 | The same Lens v1 mark appears on the icon, splash v4 and new chat. |
| 8 | State the cause and a single count (“1 command failed”); include the full command, exit status, red stderr detail and a distinct Continue action. |
| 9 | Independent old/new file-line gutters; neutral hunk metadata; no raw patch headers in the review. Show +N −M and size the sheet to its content. |
| 10 | Tinted trace rail, live spinner, expandable reasoning summary and file delta chips. Use “2m 14s”. |
| 11 | Code scrolls with 16 pt padding on both sides; distinguish comments, literals, symbols and keywords on a fixed solid terminal surface. |
| 12 | Three heading levels, serif inline math with a full-size fraction, and benchmark bars in teal and neutral with explicit labels. |
| 13 | Group models by provider, mark the selection and hide the custom ID field under a disclosure. |
| 14 | Keep full history behind a persistent, solid connection/ownership banner; use “Reconnect” or “Take over here” and explain what happens. |
| 15 | Search the thread collection, show status independently of titles and group by Today, Yesterday, Previous 7 Days, Previous 30 Days, then month. Pin one glass Settings bar. |
| 16 | In the iPad split view, one owner for sidebar toggle and one for New Chat. A wide detail title wraps instead of truncating. |
| 17 | Put context and estimated cost above the first scroll at the medium detent; call the control “Let another device type”. |
| 18 | Scale body, headings, code and math from native text styles; keep table cells whole and let the grid scroll at XL. |
| 19 | Capture one distinct state per file using native simulator resolution, appearance and fixed status-bar settings. Keep phone exports at the same 3× scale; iPad keeps its native 2× scale. |

Per-screen reference choices and visual self-critique will accompany the final captures after reference review.
