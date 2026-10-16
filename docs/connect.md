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
replayed automatically. Unsent drafts stay in the open page; they are not stored in browser storage.

Approvals pause the agent and show the exact operation and justification. Only
the input owner can allow it once or deny it. Questions support choices, multiple
selection, and custom answers. Cancellation is never approval. An unanswered
interaction expires after 30 minutes. Detached workers inherit the launch policy;
GC refuses to reuse a worker with a different policy. Administrator requirements
continue to apply.

## Reading and previews

The conversation distinguishes user messages, model responses, tool activity,
and failures. Consecutive tool calls form expandable groups. Reasoning exposed
by the provider, activity, context, and checklist information live in a details
drawer. Teammate transcripts load when requested.

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

## Phone and home-screen layout

The interface respects the notch, home indicator, landscape insets, and visible
keyboard area. Touch text fields use at least 16px to avoid Safari focus zoom.
Pinch zoom remains available. Threads and details use separate dismissible drawers;
only one overlay opens at a time. Reduced-motion settings disable interface animation.

Streaming preserves completed Markdown blocks and batches updates. Reasoning has
its own follow position. Copy and expanded previews retain source text. Oversized
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
The catalogue comes from the native action registry and is fetched only when
first opened.

Browser commands open conversation navigation, activity, reasoning, context,
checklists, and theme controls. Other commands are marked **Terminal only**.
Their native handlers have not yet been exposed through the browser protocol.
Arguments are not currently supported. Unknown or unsupported slash commands
produce a local notice and are never submitted to the model. Opening the palette
does not use model tokens and remains available during a running turn.

The interface bundles its sans serif and monospace fonts locally. The theme
follows the device setting by default; the theme button cycles through light,
dark, and system modes. Code and mathematical notation retain their own fonts.
