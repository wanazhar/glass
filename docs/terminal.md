# Terminal interface

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Keys

| Key or command | Action |
| --- | --- |
| `ctrl-g` | live transcript: scroll, collapse, follow; works during tools |
| `ctrl-t` | live provider reasoning or public summary |
| `esc` in a viewer | close it and restore the draft and main view |
| `/` on an empty prompt or `ctrl-p` | command palette |
| `ctrl-c` | interrupt |
| `ctrl-d` | open exit confirmation; Enter exits, Esc stays |
| `up` / `down` | prompt history (line motion in a multi-line draft) |
| `enter` | steer the running turn |
| `alt-enter` | queue for after the turn |
| `/help` | searchable help modal |
| `/keys` | contextual keyboard reference, including active-turn controls |
| `/model` | choose provider/account, then an available model |
| `/model <name>` | use a custom model ID directly |
| `/login` | guided provider or ChatGPT sign-in |
| `/logout` | remove a saved connection and its credentials |
| `/status` | connection, model, and session status |
| `/clear` | start a fresh session in this terminal |
| `/extensions` | discover, inspect, enable, or disable extensions |
| `/todo` | inspect the native checklist; edit when idle, read during a turn |
| `/plan` | inspect the project and review a plan before execution |
| `/init` | draft AGENTS.md for this repository in a read-only run |
| `/git` | review changes, stage files or hunks, confirm a commit |
| `/doctor` | provider diagnostics and optional streaming/tool probe |
| `/profiles` | workspace provider, model, instructions, and add-ons |
| `/responses` | formatted responses, code blocks, and exact copy |
| `/skills` | loaded skills |

## Command palette

Type `/` at an empty prompt or press `ctrl-p` to open the command modal.
The empty palette shows **Conversations**, **Work**, and **Settings**. Search any action by name, legacy alias, or intent (for example, `copy code` or `change model`); type `*` to browse every action. Work and Settings show the effect of each action before selection. Existing slash shortcuts remain available.

The top-level palette has no parent menu: Esc closes it and restores the draft.
Esc in a nested menu returns to its parent, including sign-in and custom-provider setup. Esc at the first screen closes the palette.
Canceling an API-key setup discards the staged key. A completed ChatGPT sign-in
saves its credentials before model selection; returning from that model menu
does not undo the completed sign-in.

Use `up`/`down` to choose, and press `enter` to select. Commands
open selectors or input fields inside a modal. `/model` chooses a provider or
account and then fetches its available models; `/help` and `/keys` are searchable. `tab`
completes a command for editing inside the palette. `esc` returns one menu level
or closes the top-level palette. A draft opened with
`ctrl-p` is restored when you cancel. Loaded extensions' slash commands appear
alongside the built-ins.

The palette and its command dialogs draw inside the app-owned screen, without
nested alternate screens. `/outputs`, `/responses`, and `/details` share one output inspector, filtered to all outputs, replies, or tools respectively. In the inspector, `c` copies original text, `b` selects fenced code, `r` toggles raw display, `l` loads a saved output range (64 KiB), and `k` copies a shell command. Display caps never alter copied content.

Ordinary typing uses the terminal's cursor and flushes
every frame, keeping the first and last typed characters visible immediately.

## Questions and approval dialogs

Models can ask questions with choices, multiple selections, or a custom written
answer. Every question offers **Write an answer…**. F2 opens the full question
when the preview is too short. Cancelling a question stops the current turn.

Approval dialogs show the exact requested operation and justification. Press
**a** to allow once, or **d**, Enter, or Esc to deny. The turn waits without
another provider request while either dialog is open. Answers to ordinary
questions never grant permissions. See
[Approvals and user questions](reference/interaction.md).

## Appearance and output

The renderer supports colored headings, emphasis, inline code, nested lists,
task checks, code blocks, and a bounded Markdown table format. Wide tables use
columns with aligned cells and continuous dividers; narrow tables use labeled
fields. Standard Markdown alignment markers control left, centered, and right
alignment. Horizontal rules share the reading column. Unsupported markup remains readable
source text. Prose wraps within a reading column capped at 88 cells.

Streaming renders completed display segments and keeps the last row provisional.
Segments are bounded to 8 KiB. Incomplete markup is displayed literally at
completion or the segment bound. Original text stays in the log and exact copies.
Provider text is sanitized before terminal rendering.

Tool cards separate outcome, duration, command, and a bounded output preview.
`/details` opens full recorded information. Failed statuses use the error color;
active work shows an animated glyph and elapsed time. `/tools` changes card
presentation without removing recorded output.

`/animation` previews Dot, MiniDot, Pulse, Spark, Bounce, Orbit, Grow, and Blink.
Charm's Dot animation is the default: eight fixed-width frames, updated at the existing activity cadence
(target 100 ms). The chooser makes no model requests. A saved workspace profile
retains the selected animation; without one, the selection lasts for the session.

Ctrl+D opens an exit confirmation. Enter exits; Esc returns with the draft and
menu state preserved. Confirming during a turn cancels the request before exit.
Conversation history remains saved; unsent drafts are discarded.

`/theme` offers Aurora, Amber, Monochrome, Orchid, and Ocean. Input, Markdown,
modals, and status share the selected palette. Active-work motion uses the existing
input loop with a target interval of 100 ms. Completed text stays still.
`NO_COLOR=1` removes colors while preserving activity indicators.

`/mods` and `/extensions` manage their distinct integration types. Saved workspace
profiles retain supported appearance and integration choices. See
[Extending](extending.md) and [Permissions](security.md).

## Screen ownership and scrolling

The interactive application owns one alternate screen. Exiting restores the
shell's primary screen and scrollback. Conversation, activity, and composer use
separate regions; resizing repaints the owned surface. Modals do not open nested
alternate screens.

Inline chat, the fullscreen transcript, and provider reasoning use a shared reader
controller. Conversation search, scroll position, follow state, tool selection, and expansion
survive view switches. Reasoning opens at its latest lines on every invocation,
clearing any earlier reasoning search. The rendered viewport cache is capped at 1 MiB; durable
records remain in the private log.

PageUp or Ctrl+B moves toward older content. PageDown moves toward newer content;
Ctrl+F depends on the active view and search state. Terminals that forward mouse
events support wheel scrolling. New output preserves a paused reading position.
End or Esc returns to the latest content, subject to the active search/modal layer.
The composer remains separate from the transcript. Readers show position and
return-to-bottom hints without a vertical scrollbar; its column is available
for text. Paging, search anchors, and follow mode remain active.

Ctrl+G opens the transcript while idle or busy. Ctrl+T opens reasoning text or
public summaries exposed by the provider. Providers that expose neither show an
empty-state explanation. During an active turn, Ctrl+C cancels work even when a
reader is open, and closes the reader. Closing a view normally restores the
draft and repaints the bounded main-output cache.

The live reasoning reader owns drawing and navigation while open. The underlying
conversation does not repaint beneath it. Wider terminals use a padded reading
area; narrow terminals use the available width. A live indicator marks incoming
reasoning. New text follows the latest lines until you scroll up; End resumes
following. Closing and reopening reasoning returns to the latest text.

Detailed tool output retains the preview's semantic colors, including added and
removed file lines and diagnostics. Long tool titles and output notices stay
within the available width. Enabled Mermaid diagrams preserve surrounding prose
and other blocks. Native Mermaid rendering is enabled by default; set
`mermaid = false` to show source. Flowcharts support comments, node labels,
class suffixes, nested groups, chained connections, and solid, dotted, and thick
arrows. Simple solid chains use centered boxes joined by vertical arrows. Branching,
grouped, or cyclic flowcharts use boxed components with outgoing arrows
attached to their source; group paths appear above the components. HTML label
breaks become separate lines in each box. Very narrow views below 16 content
cells use compact references. Sequence
diagrams use a compact participant and message list when lifelines do not fit.
State, class, and entity relationship diagrams support bounded structural views.
Custom Mermaid styles use the terminal theme instead of arbitrary CSS.

Additional chart families use layouts suited to terminal cells:

| Mermaid family | Native presentation |
| --- | --- |
| `pie` | Labeled shares, values, percentages, and proportion bars |
| `xychart` / `xychart-beta` | Category/value rows, bars, and line sparklines; declared axis ranges apply |
| `timeline` | Sections and ordered period/event connections |
| `mindmap` | Indented tree with sibling and continuation rails |
| `journey` | Sections, task scores, five-point ratings, and actors |
| `quadrantChart` | Quadrant names, bounded coordinate plot, exact point legend, overlap markers |
| `gantt` | Calendar-date task spans, dependency dates, status tags, and milestone markers |
| `sankey-beta` | Weighted source/destination connections and relative flow bars |

These are semantic terminal views, rather than raster copies of browser charts.
Data charts accept at most 64 items, 256 source lines and 64 KiB. XY charts accept
four series and 64 points per series. Gantt supports `YYYY-MM-DD`, day/week
durations, prior-task `after` dependencies and zero-duration milestones.
Working-day exclusions, forward dependencies, alternate date formats, custom
class definitions and unsupported directives fall back to source. Mindmaps
support 16 levels; narrow views use explicit level labels when rails cannot fit.
Non-finite numbers, invalid dates, mismatched series, and ambiguous syntax do not
produce a partial chart.

Streaming and conversation replay use the same renderer. A Mermaid block waits
for its closing fence before rendering, with a 128 KiB and 256-line buffer limit.
Unsupported or oversized diagrams retain their source; enabled rendering shows
an explanation when a complete block cannot render. This is a terminal subset,
not a browser Mermaid engine: interactive links, full class members, entity
attributes, custom CSS, and unsupported families retain source.

GitHub-style NOTE, TIP, IMPORTANT, WARNING and CAUTION callouts use a labeled,
colored heading above their quoted detail.

Inline Markdown supports underscore emphasis, strikethrough, nested emphasis,
multi-backtick code, and links with visible destinations. Image references show
the label and path; they do not fetch or display an image. Numbered lists accept
both `1.` and `1)`, and nested quotes retain their rails. Raw HTML and reference
link definitions remain text. See the [rendering audit](plan/reviews/rendering-audit-2026-10-15.md)
for supported cases and verification.

Small terminals use shorter labels and fewer decorative rows. Commands and usage
wrap within their regions. Unicode width handling accounts for wide characters
and combining marks; complex emoji sequences may still differ between terminals.
Local PTY checks include 20x8 screens and resize transitions.

## Small terminals and editing

Modal selectors keep the current selection visible and drop decorative rows on
short screens. Press F1 or Right to read a selected item's full label. Information
views wrap and scroll with arrows, PageUp/PageDown, Home/End. Fields display an
insertion caret and mask secrets. The composer keeps a bounded viewport around
the cursor for long pasted text and clears rows when a draft shrinks. Width
calculations account for wide Unicode characters and combining marks.

Ctrl+R (or `/prompts`) searches prompt history. Ctrl+X (or `/editor`) uses
`VISUAL`, then `EDITOR`, then `vi`, preserving the draft if the editor fails.
Tab searches project file paths. Alt+Enter inserts a newline while idle;
Ctrl/Shift+Enter also do so when the terminal reports those keys. While busy,
Alt+Enter keeps its existing meaning of queueing a follow-up. Bracketed paste
preserves newlines. Ctrl+C cancels the current turn, drops queued follow-ups,
and stops owned shell process groups. Cancelling a response stops local
consumption; it does not guarantee that the provider stops processing or billing
an already submitted request. `/redraw` repaints the current terminal screen;
Ctrl+G reopens the recorded transcript.

Offline PTY checks cover 80×24, 40×12, 24×10, and 20×8 terminals, resize during
editing and modal navigation, draft restoration, multiline paste, file selection,
sessions, response recovery, and cancellation. Account-backed sign-in and inference require separate verification.

## Steering

During a turn, typing continues to edit your draft.

- **Enter** submits a steering message at an available safe boundary. The message
  becomes part of the conversation so the model can adjust the current task.
- **Alt+Enter** queues a follow-up for after the current turn.
- **Ctrl+C** cancels active work and stops owned shell process groups.

Provider streaming, tool execution, and follow-up delivery have different safe
boundaries. Cancellation does not roll back files or guarantee that a provider
stops processing an already submitted request. Completed results remain available;
inspect uncertain operations before retrying them.

The input thread remains separate from provider and tool work so these controls
remain available while output arrives. `/queue` manages saved follow-ups.

## Turn notifications

Notifications are interactive-only. A completed turn of at least five seconds,
or a failed turn, can trigger the configured terminal mechanism. Cancelled and
steered turns do not notify; queued work defers notification until its turn ends.

`notify` accepts `auto`, `osc`, `osc777`, `bell`, and `off`. Automatic selection
uses terminal detection. Notification support also depends on the SSH client and
multiplexer. Messages include duration and available cost information, with
terminal controls sanitized.

## Program status

Interactive sessions emit Program Status Protocol reports through OSC 7501:
`working`, `done`, `error`, `idle`, and `blocked` for supported trust/authentication
waits. `program_status = "off"` disables reports. Headless output omits them.

Reports use a session title or error excerpt and exclude prompts and model output.
Terminals that do not support the sequence may ignore it. No detection handshake
is required. This protocol is separate from turn notifications.

## Code and mathematical notation

Fenced snippets use native lexical highlighting for Python, JavaScript and
TypeScript, Rust, C/C++, Java, Go, shell, JSON, SQL, configuration files, and
markup, Ruby, Lua, R, PHP, PowerShell, Dockerfiles, Makefiles, GraphQL, patches,
and logs. Python triple quotes, Rust raw literals, Java text blocks, Lua long
strings/comments and HTML comments retain their multiline state. Diff additions
and removals have separate colors; logs distinguish warnings and errors.
Keywords, strings, comments, numbers, and callable names have separate
styles. Unknown language tags remain plain text. Highlighting does not run a
language server or parse the project; copying a block preserves its exact source.
Code rails use the available content width in both streaming and replay.

Inline math accepts `$...$` and `\(...\)`. Display math accepts standalone
`$$` or `\[` delimiters and `math`, `latex`, or `tex` fences. The native formatter
supports Greek symbols, operators, fractions, indexed roots, scripts, vectors,
accents, boxed expressions, binomial notation and common number-set symbols.
Standalone display fractions use a stacked rule. Matrices, arrays, cases and
aligned expressions use aligned columns and native delimiters; narrow matrices
use numbered row lists instead of clipping entries. Display geometry is bounded
to 32 rows and 16 columns. It renders Unicode text rather than typeset graphics.
Unsupported commands retain their source. Parsing is limited to 16 KiB and 16
nesting levels; TeX packages, macros, shell commands, and file access never run.

Ctrl+D opens a compact centered confirmation over the conversation. Enter exits;
Esc or Ctrl+C returns to the draft. The conversation remains saved. An unsent
draft is discarded only when exit is confirmed.
