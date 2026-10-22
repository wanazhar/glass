# Remote workspace

[Documentation index](INDEX.md) · [Getting started](getting-started.md)

GC Connect opens your existing GC threads in an installable browser workspace.
The agent and its tools run on your host. Closing the browser, locking your phone,
or stopping remote access does not cancel a running task.

## Start

Install Tailscale on the host and phone, sign in to the same tailnet, and connect
both devices. The host must be a user-owned node with a resolvable Tailscale login.
Tagged service nodes do not have the owner identity this initial mode requires.

From a source checkout, include the optional web assets:

```sh
GC_INSTALL_WEB=1 ./scripts/install.sh
gc connect
# Or launch the web workspace with an explicit restriction:
gc --restricted connect
```

The installer needs Node 22.16 or newer to build missing web assets. The installed
application needs neither Node nor a frontend build tool. Alternatively, run
`npm run build:web` and set `GC_WEB_ASSETS` to the resulting `web/dist` directory.
Binary distributions can provide the web assets separately; point that variable
to their extracted directory.

GC prints a private HTTPS URL. Open it in Safari on your iPhone, then choose
**Share → Add to Home Screen**. First-time Tailscale Serve setup may require
enabling HTTPS in the tailnet. GC does not enable Funnel or publish a public URL.

```sh
gc connect status
gc connect stop
```

An existing Serve listener on port 443 is preserved. GC selects a free port from
443, 8443, 8444, or 8445. A stale route recorded for this gateway can be reused;
unrelated routes are not replaced. Stopping removes GC's route only if it still
points to GC's recorded loopback endpoint. It does not reset Tailscale Serve or
stop session workers. Use `gc stop THREAD_ID` separately to stop an idle worker.

For direct HTTP access by Tailscale IP, use:

```sh
gc connect stop # only if the gateway is already running
gc connect --http
```

This prints `http://TAILSCALE_IP:PORT` and keeps the HTTPS endpoint when available. Both use
Tailscale owner identity checks. The HTTP listener uses an available port from
8080 through 8083. It stays inside the tailnet; it is not a public listener.
Plain HTTP supports the online workspace, but browsers require HTTPS for service
worker caching and some clipboard features. Use HTTPS for the full PWA experience.
`gc connect stop` removes both GC routes while preserving session workers.
Tailscale's HTTP router normalizes IP hosts with the tailnet DNS suffix. GC adds
and removes that exact alias through the local daemon API using version checks,
so unrelated or concurrently changed routes are preserved. The default daemon
socket is `/var/run/tailscale/tailscaled.sock`; set `GC_TAILSCALE_SOCKET` when the
daemon uses another Unix socket path.

For local development without Tailscale:

```sh
gc connect --local
```

The private link has a secret in its fragment. The browser exchanges it for an
HttpOnly, SameSite cookie and removes the fragment. Do not share that link.
Remote Tailscale access uses the host owner's verified Tailscale identity instead
of a separate GC account or password. Browser host and origin checks remain
active. Other tailnet users are rejected even if network rules let them reach
the listener. Tailnet rules still determine network access.

## Shared threads and input

The sidebar lists normal GC threads, grouped by workspace. Selecting a saved
thread starts or reuses its served owner. Its ordinary JSONL log remains canonical;
no separate web conversation database is created. `gc resume THREAD_ID` attaches
to a live served owner, including when selected through the resume picker.

A standalone TUI that already owns a thread keeps its writer lock. Close that
TUI before opening the thread in the browser. GC does not interrupt it, steal its
lock, or silently change its execution policy. For simultaneous access, use the
served thread through `gc resume THREAD_ID` or `gc attach --socket PATH`.

One attached client owns input. Browser tabs use the gateway's shared client.
Use **Release control** before moving from web to terminal, or release it in the
terminal before choosing **Take control** in the browser. During active work,
choose **Steer task** to apply a correction before further actions, or **Queue
follow-up** to run another prompt afterward. The stop button cancels the current
turn. Queued follow-ups are saved before GC acknowledges them. On restart, prompts
that never started can resume; an accepted request with unknown effects is not
replayed automatically. Unsent text drafts are stored locally in this browser, separately for each thread.
The store retains at most 24 drafts for 30 days, with a 60,000-character limit
per draft. Storage failures are reported; saving a draft never submits it.

Approvals pause the agent and show the exact operation and justification. Only
the input owner can allow it once or deny it. Questions support choices, multiple
selection, and custom answers. Cancellation is never approval. An unanswered
interaction expires after 30 minutes. Detached workers inherit the launch policy;
GC refuses to reuse a worker with a different policy. Administrator requirements
continue to apply.

## Reading and previews

The conversation distinguishes user messages, model responses, tool activity,
and failures. Consecutive tool calls form open groups of collapsed activity rows.
Each row shows its action, target, status, and browser-observed elapsed time for
live calls. Expand a row to inspect input/target and result separately, copy the
result, or open its full preview. Saved calls do not invent durations. Complete
arguments are available when replaying saved calls; live starts currently expose
only their command or target.

The `›_` toolbar button opens **Agent activity**, a read-only execution log built
from existing tool events. Steps have separate status, command/target, and result
sections, with All/Running/Failed filters, expand/collapse controls, and a line-wrap
toggle. Result columns stay aligned by default and scroll horizontally. Output uses
plain, high-contrast text in the active theme, independent of Markdown code colors. It updates at start/completion boundaries, follows new
output only when already at the bottom, and offers a snapshot download. This is
not an interactive shell or byte-by-byte terminal stream. Log snapshots retain
up to 128K characters of recent output. The structured view shows at most 48
recent steps, with an 8K-character result preview per step; individual conversation
steps retain their existing fuller previews.

Public reasoning appears as collapsed Thinking/Thoughts rows between steps.
Their 64K-character previews render Markdown only when expanded. The Reasoning tab
retains its existing fuller reading windows. Private provider continuation data
remains excluded. Activity, context, and checklist information also live in the
details drawer. Teammate transcripts load when requested.

Questions and approvals appear inside conversation flow, with native radio or
checkbox choices, descriptions, and a written-answer field. Pending submissions
are disabled immediately to prevent duplicate answers. Standalone local links to
documents such as PDFs, Markdown, and spreadsheets appear as file cards. Cards
reuse host file previews and downloads, fetch no metadata ahead of a click, and
do not claim that a linked file has been verified. Diagrams keep their existing
lazy rendering, source view, and expanded viewer. No extra model request or new
frontend dependency is required for these presentations.

Previews include:

- Markdown, headings, lists, tables, and quotes.
- Syntax-colored code and unified diffs, with copy and expanded source views.
- Mermaid charts rendered when visible and settled, with source switching, an expanded view, and zoom controls. Large charts remain on demand.
- Inline and display math through KaTeX; unsupported input retains readable source.
- Workspace Markdown, JSON trees, text, and image files.
- HTML and SVG source previews in a script-disabled sandbox. These cannot submit
  forms, run scripts, load external resources, or access the surrounding app.
- PDF documents, rendered one page at a time with bounded canvas dimensions.

Workspace file previews stay inside that thread's workspace and use descriptor-
relative file access. External Markdown images are not loaded automatically.
Opening an external link is a separate browser action. No preview sends content
to a model provider.

Scrolling up pauses following. Returning to the bottom resumes it. Expanding a
preview does not change the thread's stored source. Browser suspension does not
pause the host; reopening reloads bounded history and the current live state.

## Files and working directories

Open the folder button or the **Files** tab to browse the host. Select a folder,
enter a path, or go to its parent. File rows include allocated-size indicators.
**Measure** scans disk usage on request; a lower-bound marker means the scan
reached its time or entry limit. Scans do not follow symbolic links or cross
filesystem boundaries. Explicit navigation can resolve a symbolic link when the
result remains permitted by the execution policy.

**New thread here** creates a normal GC thread bound to the selected directory.
It appears in the shared thread list and can be resumed from the TUI. Restricted
profiles and active sandboxes keep browsing and directory selection inside the
launch workspace. With the unrestricted profile and sandbox off, the authenticated
host owner can select another directory. Preview content is not sent to a model.

Directory pages contain at most 256 entries, sorted within each page. Pagination
reads the live directory and is not a filesystem snapshot. Usage scans stop after
100 ms, 4,096 entries, 256 queued directories, or 16 levels. They run only when
requested. Filesystem operations themselves can still block on a slow or unavailable
mount; these are traversal budgets, not kernel I/O deadlines.

The Glass logo in the thread sidebar returns to the home screen. This does not
create a thread or stop work in an existing thread.

## Phone and home-screen layout

The interface respects the notch, home indicator, landscape insets, and visible
keyboard area. Touch text fields use at least 16px to avoid Safari focus zoom.
Pinch zoom remains available. Threads and details use separate dismissible drawers;
only one overlay opens at a time. Reduced-motion settings disable interface animation.

Streaming preserves completed Markdown blocks and batches updates. Reasoning has
its own follow position. Touch, wheel, and keyboard scrolling pause following in the reasoning pane; Latest
in the fixed header restores live output at the bottom. Following resumes only
when Latest is selected or the Reasoning tab is entered. Completed reasoning
remains freely scrollable.
Expanded source wraps long lines and commands to the available width.
Closing an expanded preview returns to its
parent preview and preserves its scroll position. Escape follows the same sequence.
Copy and expanded previews retain source text. Oversized
live output shows a limit notice rather than repeatedly rebuilding unchanged content.

## Resource limits and storage

The initial application script is roughly 26 KiB compressed. Syntax, math,
diagrams, PDF rendering, and fonts load separately. The web assets live outside
the Rust executable. Streaming updates are batched; offscreen messages use browser
content visibility, and completed messages are not reparsed on every token.

The gateway accepts at most 32 HTTP connections and 16 connected thread bridges.
Idle, unobserved bridges can be released when that limit is reached. Each bridge
retains at most 1 MiB of recent transport events and a 512 KiB current-response
preview. History pages contain at most 60 records from a 512 KiB file window.
Oversized records are reported; use `gc export` for their full source.

Requests are capped at 64 KiB. Text file previews are capped at 512 KiB; supported
images and PDFs at 8 MiB. Image dimensions are limited to 16 million pixels. PDF previews accept at most 500 pages and render a single
canvas capped at four million pixels. Diagram input is capped at 64 KiB and 200
edges. These limits bound presentation, not the task's model or tool-call budget.

The PWA caches static application assets only. Thread content, credentials,
previews, and API responses are not written to its service-worker cache. The host
stores a private `connect.json`, a process lock, and a small gateway log. A thread with queued follow-ups also has a private
`serve-queue.json` sidecar, capped at 32 prompts and 2 MiB. Session
workers keep the same session, tool, and retention records as normal served GC.

## Development

```sh
npm run build:web
cargo test --workspace
cargo build --release -p glass-core --bin glass-core
GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core" python3 scripts/smoke-connect.py
```

The browser smoke script uses Playwright Core and an installed Chromium. Set
`GC_BROWSER_BINARY` to its executable, or install Playwright's Chromium through
`web/node_modules/.bin/playwright-core install chromium`. It uses isolated local
storage and a mock provider, and does not change Tailscale routes.

## Browser commands

Type `/` in the composer to open the command palette. Choose Conversations,
Work, or Settings, or search by command name, alias, or description. Arrow keys
move the selection and Enter chooses it. Escape returns to the root from a
section; Escape at the root closes the palette. Touch selection is supported.
On touch devices, browsing a category leaves the keyboard closed; tap search to
type. Typing `/` opens search with typing focus. The command list retains its
scrolling element while the keyboard changes the visible viewport.
The catalogue comes from the native action registry and is fetched only when
first opened.

The browser exposes the native command catalogue, including arguments and
extension commands. Commands open a focused form or a local view. File, Git,
workspace task, checklist, model, profile, memory, and agent operations run on
the host through the same native handlers. Confirmation fields protect actions
such as restores, publication, and configuration changes. Credential fields
are masked and are not added to the conversation or sent as model prompts.

Only the command-selection menus, Git/Changes, and Work & teammates use rounded
floating panels. Other command views, readers, editors and workflow surfaces
are square, borderless full-screen views on all screen sizes.

Command controls stay above the results while data loads. A compact inline
status beside the action buttons reports progress and errors without a footer. The initial result
area reserves space with static placeholders; refresh keeps the current results
visible and marks them busy until the replacement arrives.

All native palette result views share labeled rows, readable empty states and
compact sections. Lists retain item IDs and provide a local filter for larger
results; multiline output and diffs use the full reading width. Quote sources
can be selected into the form for review before attaching. Command forms show
only fields relevant to the chosen action. Opening a command does not start a
model request; submit its form to run it.

`/guidance` provides a searchable command list; `/usage` summarizes tokens, cache reporting and known
cost, with diagnostics under More details. Missing usage and price remain
explicitly unreported. `/status` puts connection and model details first.

Opening the palette and inspecting local state do not use model tokens.
Commands that request model work, such as compaction, planning, and code review,
run only after submission and provide cancellation. Mutations submitted during
a running turn wait for a safe boundary; inspection remains available.
`/help` opens a local help page with keyboard shortcuts and a searchable command
reference. Entries open their command views directly.
Unknown slash commands produce a local notice and are never sent as chat text.

Models receive an explicit teammate CLI recipe when their session has a managed
teammate runtime. The recipe also refreshes after a resumed system checkpoint.
An older worker needs a runtime restart to receive updated model guidance.

Older thread workers require an explicit restart before using native browser
commands. The browser offers a confirmation step, waits for the idle worker to
exit, and reconnects with the current binary. Conversation history is preserved;
worker-owned background jobs and teammates stop during this restart. Active
work is not automatically interrupted.

Command results are bounded for transport. Export downloads the full transcript
separately. Read-only observers can inspect state but cannot submit mutations or
model work. Browser presentation uses forms and result panels rather than
reproducing terminal-specific controls.

The interface bundles its sans serif and monospace fonts locally. The theme
follows the device setting by default. The theme button switches directly between
light and dark based on the currently visible theme. Reloading restores device
theme following. Code and mathematical notation retain their own fonts.

### Command transport

The browser reads command names and form descriptors from `/api/commands`.
Thread replay can replace this catalogue with registered extension commands.
Local inspection uses authenticated `POST /api/command`; session operations
use the existing `/api/send` bridge with a `command` frame containing `name`,
`values`, and a unique `request_id` of at most 128 bytes. Values are limited to
64 KiB. Responses arrive as `command_result` with either `result` or `error`.
Model operations also emit `command_start` and `command_done`.

`cancel_command` targets a request ID. Cancellation is cooperative: completed
side effects remain completed. Mutation receipts prevent automatic replay after
a disconnect or worker restart. If a response is lost, inspect the affected
state before submitting a new request. Receipts contain IDs and status, not
form values or credentials.

`POST /api/upgrade` requires thread control, an idle runtime, and explicit
confirmation. It uses graceful shutdown and never kills an active process.
`GET /api/export?id=THREAD` returns a Markdown attachment; transcript downloads
are limited to source logs of 64 MiB.

The queue view distinguishes live request IDs from saved terminal follow-ups.
Use `saved-*` actions with the displayed one-based index to edit, reorder,
promote, or remove a saved item. `/queue-run` checks provider readiness before
committing delivery of the next saved item. A delivery receipt prevents a
second user message from being appended after a worker crash.

`/handoff` previews the current conversation, saves a new host file, or imports
a reviewed handoff into a new thread. Import requires a current preview
fingerprint and confirmation; it does not execute tools or contact a model.
File previews require thread control. Browser `/prompts` shows only prompts
from the current conversation.

### Reading outputs on mobile

The command palette uses a compact, scrollable overlay. Closing it clears a
standalone slash from the composer and preserves longer drafts. Temporary
notices expire; their close button dismisses them immediately. Output resuming
also clears a stale provider cooldown notice.

The folder button opens Files. The activity button opens the Activity tab.
File names use compact UI text. Path inputs display at 14 px while retaining a
16 px input size to avoid Safari focus zoom. Long thread titles stay within the
header. Tables wrap cell content by default and scroll horizontally when needed.
Table headers remain visible while scrolling. Tables offer compact mode, CSV copy,
and a full-screen expanded view. Expanded tables retain the chat styling and wrap
all cells within the screen width. Expanded headers scroll with their rows. Working threads show animated dots in the
sidebar; reduced-motion preferences disable the animation.

Mermaid loads on demand, serializes rendering, and caches at most 32 results.
The cache distinguishes light and dark themes. Each displayed copy receives
unique SVG identifiers without changing label text. Source is retained on
failure, with an explanation next to the diagram. Preview limits remain 64 KiB
of source and 500 edges; unsupported syntax is not silently rewritten.

Run `python3 scripts/smoke-web-diagrams.py` after building the web bundle to
check 19 chart families in both themes, duplicate renders, and malformed input.
The implementation follows the bounded render queue and cache pattern used by
[T3 Code](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/MermaidDiagram.tsx).
[Hermes WebUI](https://github.com/nesquena/hermes-webui/blob/master/static/ui.js)
also keeps diagram source on failure and provides an expanded viewer.

### Projects and machine-wide conversations

The sidebar defaults to **All projects** from the current user's GC data
storage. The directory where `gc connect` starts is the default for creating
a thread, not a filter on existing conversations. Conversations
from another OS account or another GC data directory are not merged.

Project groups show the full workspace path and thread count. Expand a group
or use the project selector. Search matches titles, paths, and thread IDs across
the complete index, including older pages. The list loads at most 512 rows per
request; **Load older threads** retrieves the next page. Counts indicate partial
lists. Transcripts load only when selected.

The active project's name remains visible in the mobile header. New threads
inherit the selected project, then the active thread's project, then the
connection's starting directory. The file browser also provides **New thread
here** for choosing a different directory.

Legacy conversations without workspace metadata appear under **Unassigned**.
They can be read without starting a worker. Conversations whose project
directory is missing also open as read-only archives. GC does not infer a
workspace or grant tool access to these archives.

Earlier activity loads one page per request. The button shows progress and ignores
repeated taps until the request finishes. Prepending history preserves the reading
position and does not replace the current reasoning, checklist, or live response.

The reasoning reader has its own history cursor. Opening a short reasoning pane
loads older reasoning automatically; Earlier reasoning loads another bounded batch.
Each batch reads at most four history pages, with a 256 KiB reasoning preview limit.
Loading chat history skips internal-only pages within the same four-page budget.
Teammate inspection opens a separate modal with loading, empty, and error states;
activity updates do not replace its contents. Standalone layout uses fixed viewport edges while the keyboard is closed.
This avoids WebKit under-reporting the installed app viewport by its safe inset. The toolbar leaves extra space below the status area; composer bottom
padding is capped at 34 px. Keyboard layout uses the visual viewport height.

For device-specific sizing problems, `/layout` opens local viewport, safe-area,
and element measurements. The report also includes the loaded shell version,
requested Apple status-bar style, page theme colors and browser user agent. These
page values do not establish which native status-bar style iPadOS applied.
Copy measurements collects local diagnostics; the command does not contact a
model or change layout settings.

Layout diagnostics dismiss the keyboard and wait for stable viewport geometry
before capturing. The report includes a `settled` flag. Keyboard detection uses
the layout viewport and last idle height because Safari can shrink both the
window and visual viewport; its offset is applied while the keyboard is open.

On iOS, the page leaves native status-bar styling to the system and gives both
its root and body the active theme background. WebKit's current guidance is to
remove the legacy Apple status-bar-style override so the system can match the
webpage color: https://www2.webkit.org/show_bug.cgi?id=317153#c2.
Browser checks verify page colors and metadata; native iPadOS appearance still
requires device confirmation. Standalone headers leave 24 px of space after the
reported top inset. With the keyboard open, the command palette includes the
visual viewport offset and keeps its header, search field, and footer from
shrinking.

Standalone shell geometry is also selected through the CSS display-mode media
query. Its first frame therefore uses the same header spacing, fixed edges, and
bottom inset as the initialized app, without waiting for JavaScript or making
an additional startup request.

Accent-button text uses the same light-theme color token before and after
JavaScript initialization, preventing a dark-to-white text flash at launch.

Fullscreen viewer headers use the same clearance below the iOS status area as
the main toolbar. The compact palette starts below that area as well. Returning
from a preview preserves the sidebar's scroll position and focus; its pane
outline is shown for keyboard navigation rather than touch interaction.

The command palette is centered in the safe viewport when the keyboard is closed
and in the visual viewport when it is open. Its command list scrolls if needed;
the header, search field, and footer remain visible.


Project groups in the thread sidebar use a disclosure chevron. Expanded groups
have a shaded heading and indented threads with a grouping line; collapsed
groups retain the project name, path, and thread count.

## Direct work controls

The composer provides file attachments, model and execution-profile selection,
active-context inspection, saved-queue management, and a Work view. These use
the native session commands and their completion receipts. Mutations require
input ownership. An older gateway or worker is identified explicitly; unsupported
workflow controls explain that an update is needed. Updating assets does not
restart an existing runtime. Runtime updates are managed on the host; there is no per-thread update button.
The host can restart idle workers on the installed binary while retaining their
transcripts. Owned background jobs and teammates stop during a runtime restart.

Choose files from the device or paste image files into the composer. Uploads
remain outside the project and become active snapshots only after the native
attachment command succeeds. Text inputs are limited to 32 KiB; image inputs to
8 MiB and pass through GC's existing image preparation. Active attachment chips
reopen Context, where snapshots can be removed from future requests. Context
usage remains unknown when the runtime does not report it.

Sent messages provide quote and edit/branch actions. Editing branches before the
selected prompt and restores its text for review; it preserves the original
conversation and does not rewind workspace files. Source fingerprints are
checked by the runtime before applying source-sensitive actions. Retry is an
explicit native request and can consume provider quota.

Changes shows actual staged and working-tree diffs, including untracked file
actions. Hunks render when their file is opened. Staging, unstaging and discarding
use the reviewed Git fingerprint; stale state is rejected. Binary, rename and
file-mode changes use whole-file actions instead of fabricated partial patches.
The browser response is limited to 2 MB.

Text file previews offer editing and attachment controls. Editing is limited to
32 KiB UTF-8 files inside the thread workspace, uses GC's atomic file writes,
and rejects a stale revision. Unsaved edits require a discard decision when
leaving; reconnecting the same thread preserves the editor.

Work shows tasks and teammates, with explicit task creation/completion and
transcript inspection. Its refresh runs only while the view is open and the
page is visible; refreshing does not replace an in-progress task form.

### Large saved output

Live reasoning remains a bounded reading window. Earlier/Newer and Latest
controls separate reading from following the stream. Read full saved reasoning
opens retained source independently of the live window. Saved activity text
loads in 128 KiB pages; the first page requires one request rather than assembling
many raw JSON chunks. Parsed record snapshots are bounded to 8 MiB per record,
16 MiB across at most 16 cached records, and expire after two minutes. Continuation
requests carry a revision fingerprint. Use transcript export for larger records.

### Interactive HTML

HTML previews start with scripts disabled. Run scripts explicitly creates a
separately served document with an opaque sandbox origin and an independent
content policy. It has no GC host/session bridge; fetches, external resource
loads, forms, workers and child frames are restricted. Frame self-navigation
means this is not an absolute no-network guarantee. Sources are limited to
512 KiB; the host retains at most 16 temporary documents / 4 MiB for ten minutes.
The normal application content policy is unchanged.

### Notifications

Notifications are opt-in and capability-detected, and require a supported secure
browser context. Connected-page events can announce completion, failure or a
request for input. Selecting an alert opens its thread. This does not provide
background push delivery when the page is suspended or disconnected, including
iPhone PWA background delivery.


## Launch presentation

Cold launches use the Glass cube and wordmark with a brief outline animation and
a single opening status. Light/dark styling follows the system before application
initialization. Reduced motion disables the animation. The screen disappears as
soon as the initial workspace is ready; there is no minimum display time, fake
progress, or delay for the animation. Reconnecting an already open page does not
replay it.

A small independent bootstrap exposes retry if the application cannot load.
Startup assets are preloaded/versioned together; supported service-worker
contexts retain the current and one previous shell cohort. Startup does not wait
for optional renderers or service-worker registration. Host status and thread-index
reads run concurrently after authentication. The primary interface font is preloaded;
optional renderers remain lazy and their hashed assets are reused from the shell cache.

Launch geometry and system theme styles are inlined in the HTML, keeping the logo
centered before JavaScript and preserving a usable retry surface when CSS fails.
Status, slow-loading and recovery messages do not move the logo. Try again refreshes
the current page's app and stylesheet explicitly before reload. Failed or offline
refreshes preserve cached assets so the recovery surface remains available.

Opening a saved conversation loads its history before attempting live attachment.
If a TUI still owns the writer lock, or the worker cannot attach, the transcript
remains readable with a History only indicator and disabled sending. Reconnect after
the live runtime becomes available; no writer lock is stolen. Linux gateways also
resolve the installed replacement executable when an update has unlinked their
running binary, so new thread workers can start after installation.

## Provider reasoning replay

Chat Completions reasoning required by subsequent tool requests is retained with
the assistant response and tool-call IDs, including across resumed sessions and
context projection. These internal records are separate from the visible
reasoning reader and excluded from ordinary history and exports. They are not
converted into Responses or Anthropic reasoning. Existing display-only reasoning
from older versions is not automatically reinterpreted as provider wire data.
Provider failures remain explicit; deploying this fix does not retry a model
request or replay a tool.
