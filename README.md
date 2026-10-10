# glass-core

A minimal coding harness. Two tools, one loop, native terminal.

```
binary   ~4.8 MB   (stripped, lto=fat, codegen-units=1)
RSS      ~3.7 MB  idle
startup  <10 ms
tools    bash + state
```

### Why two tools

Measured, not assumed. Microsoft's *Is Bash All You Need?*
([arXiv:2609.11999](https://arxiv.org/abs/2609.11999), Opus-4.8 and GPT-5.5, two
benchmarks, 1,308 paired runs) found bash alone beats a typed tool catalogue by
**+24.6pp** on TheAgentCompany and **+6.2pp** on APEX-Agents, while using
**19–72% fewer tokens**. Adding typed tools on top of bash produced no
detectable gain. Separately, frontier models pick the right tool 93.1% of the
time at K=2 versus 87.1% at K=5 — the curve slopes down with tool count.

What bash-only does cost is edit-format reliability, and that is handled in the
harness rather than with a third tool: see below.

### What the research said to build instead

The same literature is clear that the win is a *harness-side guardrail*, not a
better tool. SWE-agent measured its lint-and-revert guardrail at ~3pp, worth more
than its edit tool. So:

- **Every tool result leads with its exit code.** A model without one re-runs
  commands to find out what happened, and those retries show up as duplicated
  commands in transcripts.
- **Writes are verified.** The harness detects what a command wrote (heredoc,
  redirect, append, tee, `sed -i`), reads the region back to the model, and runs
  the language's parser if one is already required by the project. A heredoc
  that wrote broken Python now returns the `SyntaxError` with line and column,
  instead of the model finding out three turns later at build time.
- **Read-before-write.** Overwriting a file the model has not read is refused.
  A blind `sed -i` succeeds silently and the damage surfaces later, when the
  cause is no longer obvious. Creating a new file is never blocked, and any
  read counts.
- **Independent tool calls run in parallel**, with same-path writes serialized
  and results reassembled in call order. A chat-completions endpoint rejects
  out-of-order tool results, and a transcript whose results appear in completion
  order is impossible to debug.
- **Transient failures retry.** A 429, a 5xx, or a dropped connection is
  re-sent with backoff (1s, 2s, 4s) before the turn gives up. That is
  safe because a failed request produced no events: the model never saw
  it and no tool ran. A 4xx is a request problem, not a blip, and is
  never retried.

## Why two tools

`bash` is the only tool that matters. The model can read, write, search, edit,
build, test, and use git with it, and every tool we don't ship is schema the
model pays for on every single request. `state` exists because with only `bash`
there is no durable place for the model to record what it is doing, and that is
exactly what context compaction needs to survive a long task.

## Tool call repair

GC validates completed model tool calls before execution. Malformed JSON,
nonobject arguments, invalid fields and unknown tools produce an instructive
error and a visible `Tool repair 1/2` notice. The model gets up to two correction
requests within the normal turn budget. A batch containing an invalid call runs
no tools; corrected calls still pass permissions and read-before-write guards.
Shell failures remain ordinary tool results; GC never guesses a command or
replays completed actions as part of repair. Incomplete provider streams remain
fatal, and interrupted partial calls never execute.

Raw rejected arguments stay in the local session audit. Provider replay uses
an empty-object placeholder for malformed input, paired with an explanatory
error result. Anthropic receives `is_error` for standard error results.
Validation covers JSON types, required/properties/items, alternatives, enums,
constants, numeric and length bounds. This is not a full JSON Schema engine:
extensions remain responsible for complex constraints such as `$ref`, regex
patterns and semantic validation. Repair adds no default tool or successful-turn
network request. Escape/Ctrl+C and follow-up prompts remain responsive.

## Native files and owned background jobs

The default model tools remain `bash` and `state`. Bash children receive `GC_BIN`,
the absolute path of the running executable. The agent can load file helper
instructions on demand with `"$GC_BIN" file --help`; there is no file-tool schema
catalogue or always-running helper process.

```bash
"$GC_BIN" file read src/main.rs 1 120
"$GC_BIN" file --json read src/main.rs
"$GC_BIN" file check settings.json
```

`file read` returns numbered UTF-8 text and a whole-file SHA-256 revision.
Reads show at most 24 KiB and 1,000 lines (200 by default), with explicit partial
read metadata. Files are capped at 8 MiB; binary files and symlink paths are
rejected. Use ordinary shell commands for larger/binary files.

`file edit` and `file write` take JSON on stdin. Editing supports unique exact
replacements or original-file line ranges, so the model need not reproduce an
entire old block. Every operation validates before a batch commits:

```json
{"path":"src/main.rs","expected":"revision from file read","edits":[
  {"start":12,"end":14,"text":"replacement lines"},
  {"old":"unique exact text","new":"replacement"}
]}
```

For insertion use `end=start-1`. Line ranges preserve the file's CRLF convention,
BOM and final-newline behavior; exact replacements use explicit supplied text.
`file write` accepts `path`, `expected`, and `text`; `expected:null` means
create-only. Existing files require the current revision. At most 128 edit
operations and one million indexed lines are accepted; exact edits need no line
index. Unchanged writes avoid replacing the file.

Mutations use a same-directory temporary file, atomic replacement, file syncing,
and cooperative parent-directory locking. Stale revisions, overlapping edits,
symlink traversal, special files and hard-linked targets are refused. Ordinary
rwx bits are retained; special mode bits are dropped. External writers that do
not share the lock can still race the final POSIX rename; this is not a claim
of filesystem-wide compare-and-swap.

Native previews color added/removed lines and syntax status. JSON checks are
in process. `file check` or `validate:true` explicitly runs syntax-only checks
for JSON, Python or JavaScript; Python/Node checks have a three-second deadline.
Unavailable and unsupported checks are reported distinctly. Validation failure
leaves the original unchanged; ordinary Python/JavaScript writes do not spawn
checkers. Use project commands for compiler/type/test diagnostics. `--json`
provides structured file and diagnostic results for headless callers.

Bash also accepts a `job` object, without adding another model tool:

```json
{"command":"npm run dev","job":{"action":"start"}}
{"job":{"action":"poll","id":1,"offset":0}}
{"job":{"action":"input","id":1,"text":"hello\n"}}
{"job":{"action":"input","id":1,"eof":true}}
{"job":{"action":"stop","id":1}}
```

Poll using the returned `next_offset` to avoid replaying old output. Metadata
includes state, exit code, captured/dropped bytes and whether pipes are still
draining. Input reports accepted bytes; retry only the remaining bytes when
stdin applies backpressure. Poll limits may extend by up to three bytes to
finish a UTF-8 character; incomplete live characters stay at the same cursor.
Wait for both `state: exited` and `draining: false` before treating output as
complete. Input is capped at 8 KiB; polling at 24 KiB plus that UTF-8 boundary; each
job log at 4 MiB. Further output is drained and counted, not accumulated.
These are pipe-based jobs, not a terminal emulator; use unbuffered output when
the underlying program needs it.

`/bg`, `/jobs`, and model jobs share the current session's registry. IDs are local
to a session. Switching sessions retains jobs under their original owner; exiting
the application stops owned process groups. Stop sends TERM, allows 250 ms for
shutdown, then sends KILL and reaps the child. No detached persistence is promised;
programs that deliberately escape their process group are outside job ownership.
Shell permissions and explicit command approvals still apply.

Tool count alone does not establish performance. The job schema adds 547 bytes
to the compact OpenAI tool envelope (1,151 to 1,698 bytes for both tools); file
helper guidance adds a small fixed prompt paragraph and full help is on demand.
Matched model/task trials are required before claiming better coding success or
lower total token cost. Local checks: `scripts/check-performance.py` and
`scripts/check-native-performance.py`.

## Prompt caching

glass-core caches aggressively and shows the hit rate after every turn:

     12,350 input / 3 output · cache 97% · $0.041

The dollar tail is the turn's cost, from a rate card. Anthropic's
list prices are built in for the default provider; any other model
prices itself via `[pricing]` in config, and an unpriced model shows
tokens only — a wrong price is worse than no price, because it is
the number a budget is decided from. The session tail carries the
cumulative cost alongside the token total and the context fill.

Three breakpoints, matching Anthropic's documented layout:

1. **system block** — the prompt plus tool schemas. This is the
   part that survives compaction, because the cache hierarchy is
   tools → system → messages and compaction only edits messages.
2. **tail** — the newest message, so turn N+1 reuses turn N
   verbatim. Verified end-to-end: the turn-1 prefix is preserved
   byte-for-byte into turn 2.
3. **lookback anchor** — a second marker placed periodically from
   the start, because the read lookback is only 20 positions and
   a run of tool blocks counts as one. Without it the tail marker
   falls off after ~6 turns and you pay a full rewrite.

The log is append-only, so compaction rebuilds the projection on
resume rather than reverting to raw history — a resumed session
does not re-compact and re-pay for a summary it already wrote.

### Cache warmer (disabled by default)

Providers expire the cache a few minutes after the last write. On a
long task the harness then goes idle — you are reading output, thinking,
away from the keyboard — and the first turn afterwards pays to re-read
the whole prefix. `extensions/cache-warmer` refreshes the cache just
before it expires, so that turn is a hit instead of a miss.

It is off by default, and the reason is worth stating plainly: a cache
*write* costs 1.25x the prefix while a *miss* costs 1.0x, so each
refresh is a net spend of about 0.35x the prefix. The only thing it
buys is latency on the first turn after an idle gap. Enable it if that
latency is worth the spend:

```toml
extensions = ["python3 ~/.config/glass-core/ext/cache-warmer.py"]
```

The extension declares a *policy*; the host owns the *mechanism*. That
split is deliberate — a subprocess cannot warm the cache itself, because
it holds no API key and the host is not reading its stdout between
turns, which is exactly when a refresh has to happen. So the extension
says how long to wait and how big the prefix must be, and the host does
the refreshing in a background thread that a new turn cancels.

Two knobs, both optional: `refresh_after_s` (default 240, just under
the 300s TTL) and `min_prefix_bytes` (default 4000, so a short prompt
is never warmed for nothing). Both are tunable by environment variable,
and `/extensions` shows the active values.

Compare: Codex CLI — the only serious Rust harness in existence — reports ~100MB
idle RSS and ships an 80MB binary. glass-core is roughly 50x smaller because it does
not do the things that make it big: no syntax highlighting, no image codecs, no
telemetry, no MCP, no embedded JS engine, and no tokio.

## Install

```sh
./scripts/install.sh
# Both commands launch the same executable:
glass-core --version
gc --version
```

The Cargo package and executable are named `glass-core`. The installer uses
`${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin` and creates `gc` as a
relative symlink to `glass-core`. It refuses to replace an unrelated `gc` command.
The separate `glass` executable and package belong to the other Glass project.

For a build without installation:

```sh
cargo build --release --package glass-core --bin glass-core
./target/release/glass-core
```

### Build and upgrade checks

The GitHub Actions workflow builds and checks Linux and macOS independently.
It runs the Rust tests, Clippy, installer checks, and offline PTY suites (including
20×8 terminals), then performs a real clean install and repeat installation in a
disposable configuration and data directory. These checks require no provider
account. They verify that configuration, credentials, history, and session files
survive installation and that the separate `glass` executable is preserved.

Successful jobs upload a native binary archive containing `glass-core`, the
relative `gc` symlink, and this README, plus a SHA-256 checksum. These are CI
artifacts; the workflow does not publish a release or modify Git tags.

To run the same checks locally:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked --package glass-core --bin glass-core
export GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core"
python3 scripts/smoke-install.py
python3 scripts/smoke-features.py
python3 scripts/smoke-onboarding.py
python3 scripts/smoke-polish.py
python3 scripts/smoke-workflows.py
python3 scripts/smoke-mobile.py
python3 scripts/smoke-chat.py
python3 scripts/smoke-motion.py
python3 scripts/smoke-live.py
python3 scripts/smoke-navigation.py
python3 scripts/smoke-trust.py
python3 scripts/smoke-sandbox.py
python3 scripts/smoke-init.py
python3 scripts/smoke-web.py
python3 scripts/smoke-mcp.py
python3 scripts/smoke-acp.py
python3 scripts/smoke-serve.py
python3 scripts/smoke-upgrade.py
```

The PTY suites require a Unix host and Python 3.10 or newer. Real provider
authentication and model behavior require a separate account-backed trial.
Install the VT parser dependency with `python3 -m pip install pyte==0.8.2`
before running the PTY suites.

### Appearance and add-on controls

Main chat uses the same Markdown presentation as the transcript and response
viewer: colored headings, emphasis, inline code, nested lists, task checks, and
framed code blocks. Pipe-framed Markdown tables become aligned columns on wide
screens and labeled fields on narrow screens. Tables stream row by row; malformed
or unsupported table syntax remains readable source text. Prose wraps at words inside a reading column capped at 88 cells,
with two-cell margins (one on the smallest phone screens). Neutral body text
remains legible without inheriting a terminal's yellow foreground.

Streaming displays completed words and rows before the response finishes. The
last row is provisional; split Markdown delimiters wait for their closer.
Unfinished display segments are bounded to 8 KiB, with very long segments rolled
onto a new display row. Incomplete markup is shown literally at completion or
that bound. Exact source text remains in logs and clipboard copies. Resizing
changes subsequent rows; printed terminal scrollback is retained. Provider text
cannot emit terminal controls or clipboard commands through rendering.

Each completed tool call forms one block: outcome and duration, the command
(up to two wrapped rows), and an indented preview of up to three output rows.
A remaining-line count points to `/details`. Failures retain their exit code
and show a red status. Running commands appear in the animated activity row. `/tools` switches to compact cards. Full commands and
recorded output remain available in `/details` and the transcript. Cancelled or interrupted responses flush their remaining
text without carrying Markdown state into the next turn.

- `/dashboard` opens workspace shortcuts for provider/model, sessions, review,
  context, plans, Git, diagnostics, profiles, responses, mods, extensions, and security.
- `/theme` selects Aurora, Amber, Monochrome, Orchid, or Ocean for the current session. The
  composer, Markdown, command selection, rounded modals, and progress share
  coordinated accent, emphasis, code, success, warning, and error colors.
  Existing profiles retain their original theme indices. Active provider, tool,
  and modal work animate with cycling glyphs, colors, and elapsed time; completed
  text stays still. Motion runs from the existing input loop, at no more than
  eight redraws per second, and does not add lines to terminal scrollback.
  Busy multiline drafts use a single-row viewport while retaining their full
  text. `NO_COLOR=1` selects neutral presentation while activity still animates.
- `/mods` toggles built-in mods for the session. `redact` masks tool-output lines
  containing common credential markers; `noisy` remains the example mod.
  Use `mods = ["redact"]` in your configuration to enable it on future launches.
- `/extensions` shows loaded versions and tools, opens details, and lets you stop
  an extension immediately. The manager discovers bundled extensions, shows source
  and access details, accepts custom commands, and persists enable/disable choices.
  Failed startup attempts remain visible in its startup warnings view.
- `/security` or `/permissions` reviews the active permission policy, saves an
  opt-in profile for the next launch, approves exact commands for this session,
  and inspects extension capability declarations before startup.

### Conversation and recovery tools

- `/search` or Ctrl+F searches the conversation. Reading positions survive
  resize and switching conversation/thinking views. End returns to live output.
- Tool cards show the exact command, status and timing. Enter toggles the
  focused card; `L` explicitly loads up to 64 KiB of spilled output for viewing
  and search. Loaded output is marked if capped.
- `/diff` browses changed files and hunks with distinct addition/removal colors
  and horizontal scrolling. `/activity` collects tools, subagents, jobs and loops.
- `gc attach` reconnects with bounded backoff, saved replay position and durable
  submission IDs. Ctrl+C/Ctrl+D can cancel reconnecting.
- `gc doctor --json` and `/doctor` provide redacted offline diagnostics.
  Provider requests require an explicit check or `gc doctor --probe`.
- `gc plugin verify` checks managed plugin content. `gc plugin update NAME`
  reviews capability changes before replacing an install; existing plugin data
  is retained.
- MCP servers can request reviewed forms/selections or HTTPS URL consent in the
  interactive UI. Headless requests fail explicitly when user input is needed.
  Form responses are sent to the server without recording the callback values
  in the conversation log. Credential collection belongs in URL flows.

See [permission and extension confinement](docs/security-confinement.md) for
`permission_profile` and `extension_isolation`, OS support and enforced limits.

Extensions keep their existing host-tool access. They run as trusted local
programs with your filesystem and network permissions, so inspect their source
before adding executable commands to `extensions` in your configuration.
GC filters inherited environment variables whose names contain `TOKEN`, `SECRET`,
`PASSWORD`, or `API_KEY`, plus `AWS_ACCESS_KEY_ID` and `SSH_AUTH_SOCK`. This reduces
accidental credential exposure; extensions can still read files accessible to you.

An extension enforcing policy should include `"authorization_required": true`
in its handshake. In this mode missing, invalid, or negative authorization replies
block the tool. Legacy extensions retain their previous optional-hook behavior.
Extensions that fail their startup handshake are skipped with a warning; check
the loaded list before relying on an extension to enforce policy.
Extension replies have bounded size and read deadlines, and extension shutdown
cleans up its owned process group. Duplicate extension names and tools, including
attempts to replace `bash` or `state`, are rejected.

New session logs and output spills use `0600` permissions; spills live under
the glass-core data directory in `output/`. Existing logs keep their current
permissions. Streamed provider text is stripped of terminal control characters.
The `redact` mod is a best-effort output filter, not a guarantee that all secrets
are removed: original shell output can still exist in spill files.

### Project workflows

- `/plan` uses a bounded, read-only filesystem inspector. Shell commands, writes,
  extension tools, and delegation are blocked during planning. Review the proposal
  in its modal and explicitly choose execution to start a normal coding turn.
  `gc run --plan --json "describe the change"` supports SSH and headless use.
  Planning still writes private session logs; local extensions are trusted programs,
  not an operating-system sandbox.
- `/init [notes]` drafts an `AGENTS.md` for the workspace in the same
  read-only mode as `/plan` (no shell, no writes), then offers review, save,
  or regenerate. The draft is written by the harness, never by the model, and
  an existing file is replaced only after an explicit confirmation; an outer
  Markdown fence is unwrapped first if the model wrapped the document in one.
  `gc init --json` prints a draft, `gc init --write` creates the file.
- `/git` reviews staged and unstaged changes, stages individual files or hunks,
  and offers an editable local commit-message draft. Committing requires explicit
  confirmation. Rename and metadata changes use whole-file staging. Cancelling a
  commit can occur after Git has completed it; check repository status afterward.
- `/doctor` checks the model catalogue and offers an optional streaming/tool-format
  probe. The probe consumes provider quota, records usage, and respects the cost
  limit; it does not execute the requested test tool. `gc doctor --json` and
  `gc doctor --json --probe` provide headless diagnostics.
- `/profiles` saves provider/model, account references, instructions, theme, mods,
  and extensions per canonical workspace path. Saved profiles contain account IDs,
  not copied credentials. Changes to an active profile are saved automatically.
  `gc profiles --json` lists profiles without opening the UI.
- `/responses` renders Markdown and fenced code, and requests exact response or
  code-block copying with OSC52. Clipboard support depends on the terminal and
  SSH/multiplexer settings. Transcript tool collapse preserves the full content.

Bundled extensions need their listed Python or Node interpreter. Enabling them
uses embedded source and does not download third-party code. Custom commands can
use a JSON argument vector, such as `["python3", "/path with spaces/ext.py"]`.
`gc extensions --json` reports configured extensions and the bundled catalogue.
Theme and mod selections are saved when a workspace profile is active.
Required extension authorization applies to streamed Bash and mixed tool batches
as well as ordinary calls.

## Configure

`~/.config/glass-core/config.toml`. Every field is optional.
Configuration and session data use only the `glass-core` namespace; this harness
does not read or migrate the separate project's `glass` directories.

```toml
provider = "anthropic"          # "anthropic" | "openai" | "openai-responses"
base_url = "https://api.anthropic.com"
model = "claude-opus-4-5"
api_key_env = "ANTHROPIC_API_KEY"
max_tokens = 16000
cost_limit = 5.0              # optional session limit in USD
mods = []                       # "noisy" is the shipped example
notify = "auto"                 # auto | osc | osc777 | bell | off

# Optional: dollars per million tokens. All four fields are
# required — a partial card would misprice every request.
[pricing]
input = 5.0
cache_read = 0.5
cache_write = 6.25
output = 25.0
```

The API key is read from an environment variable, never stored in the file. Set
`api_key_env = ""` for local runtimes that need no key:

```toml
provider = "openai"
base_url = "http://localhost:11434/v1"
model = "qwen3-coder"
api_key_env = ""
```

Any OpenAI-compatible endpoint works, which covers GPT-5, Kimi, GLM, Qwen,
DeepSeek, Mistral, Gemini's compatibility endpoint, Ollama, vLLM, and LM Studio.
A new provider is a base URL and a model id, not a new implementation.

### Model fallback

`fallback = ["claude-sonnet-4-5"]` lists model ids to try, in order, when the
primary fails *before producing any output* — hard rate limits, overload, or a
model that vanished from the account. Every entry runs on the same connection
and endpoint; the first model that answers serves the turn, and the stream
opens with a warning naming the swap so a changed model is never silent.
Authentication failures are never routed around — a bad credential would fail
identically on every model — and when everything fails, the primary's error is
the one reported. The chain applies to delegated sub-agent tasks and their
model overrides too. A fallback to a smaller-context model is not re-derived
mid-turn: compaction keeps using the primary's window.

## Keys

| key | |
|---|---|
| `ctrl-g` | live transcript: scroll, collapse, follow; works during tools |
| `ctrl-t` | live provider reasoning or public summary |
| `esc` in a viewer | close it and restore the draft and main view |
| `/` on an empty prompt or `ctrl-p` | command palette |
| `ctrl-c` | interrupt |
| `ctrl-d` | quit |
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
| `/plan` | inspect the project and review a plan before execution |
| `/init` | draft AGENTS.md for this repository in a read-only run |
| `/git` | review changes, stage files or hunks, confirm a commit |
| `/doctor` | provider diagnostics and optional streaming/tool probe |
| `/profiles` | workspace provider, model, instructions, and add-ons |
| `/responses` | formatted responses, code blocks, and exact copy |
| `/skills` | loaded skills |

### Command palette

Type `/` at an empty prompt or press `ctrl-p` to open the command modal.
Type to filter, use `up`/`down` to choose, and press `enter` to select. Commands
open selectors or input fields inside a modal. `/model` chooses a provider or
account and then fetches its available models; `/help` and `/keys` are searchable. `tab`
completes a command for editing inside the palette; `esc` closes the modal. A draft opened with
`ctrl-p` is restored when you cancel. Loaded extensions' slash commands appear
alongside the built-ins.

The palette and its command dialogs draw inside the app-owned screen, without
nested alternate screens. Ordinary typing uses the terminal's cursor and flushes
every frame, keeping the first and last typed characters visible immediately.

### Provider onboarding

Use `/login` to connect a provider. The flow guides you through authentication
and model selection, with masked secret entry and an explicit cancel action.
Built-in choices include Anthropic, OpenAI API, Google Gemini, OpenRouter,
DeepSeek, Groq, Mistral, xAI, Cerebras, Z.AI Coding Plan, Ollama, and LM Studio.
Custom endpoints support Chat Completions, Responses, or Anthropic Messages.
Your last selected connection and model are restored at startup; the remaining
options continue to come from config.toml.
API keys can come from the provider's environment variable or private local
storage. Local runtimes can use no authentication.

**Continue with ChatGPT** uses OpenAI's [direct open-source sign-in flow](https://developers.openai.com/siwc/token-sharing-open-source/sign-in),
with PKCE, a loopback callback, a stable installation ID, validated ID-token
signatures and account identity, and explicit ChatGPT plan permission. The
browser completes authorization; remote terminals can paste the final callback
URL into a masked field. Returning sign-in reuses that account's issued client
ID. Multiple accounts are kept separately. Tokens refresh before new turns,
and revoked sessions show an instruction to reconnect with `/login`.

Credentials and saved provider/model choices live exclusively in
`~/.config/glass-core/auth.json` (or the XDG config location), written atomically
with owner-only permissions on Unix. They are never imported from Glass, pi,
Hermes, or Codex. `/logout` removes this application's saved connection; it does
not revoke the provider grant or unset shell environment variables.

Model choices come from the selected provider or account's live catalog, with a
custom-ID option for endpoints that do not expose discovery. ChatGPT uses the
public Responses API with `store=false`, rather than the legacy Codex backend.
API-key pricing and ChatGPT plan usage are different: model costs are shown only
when an applicable rate card is available; unknown costs are not guessed.

`/help`, `/status`, `/skills`, `/extensions`, `/diff`, checkpoints, and job output
stay in temporary modal screens. Budgets, branch points, job actions, and command
inputs use guided selectors or fields. Normal conversation remains in terminal
scrollback. Input fields also support Ctrl+A/E, Ctrl+U/K, and Ctrl+W.

Missing credentials fail before inference. HTTP authentication failures, SSE
errors, truncated streams, and empty replies are surfaced explicitly. Partial
streams never execute pending tool calls. Both Chat Completions and Responses
retain tool requests and matching results across turns and session resumes.

The interaction patterns were informed by [pi](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/providers.md)
and [Hermes](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/integrations/providers.md).

### Sessions, context, and task review

`/sessions` searches conversations in the current project. Choose a session to
preview, rename, resume, branch through a chosen turn, or delete with confirmation.
The current session cannot be deleted. Deletion removes its transcript, title,
attachments, and cost ledger; workspace files and checkpoint snapshots stay.
Sessions are bound to the canonical workspace path. Older logs acquire a project
binding when explicitly resumed, and then appear in that project's browser.

`/name <title>` sets the title directly; `/name` opens a modal prefilled with the
current title, and an empty field falls back to the first message. `glass-core
list` shows titles, and so does `/sessions`.

`/context` shows the last foreground model request and a browser of past requests,
plus current prompt configuration, attachments, context edits, and compaction.
Request manifests record instruction/attachment paths and byte counts, available
tools and schema sizes, history counts, and which skill descriptions were supplied.
Skill bodies are loaded on demand through tools; inspect `/details` for commands
and results. Manifests describe supplied context, not proof the model read it.
They are saved as metadata in a private `.context` sidecar beside the session log,
never added to model history or compaction. Older sessions lack these manifests.
Headless access: `glass-core context SESSION_ID --json`.

The footer labels turn **input/output** separately from **session total tokens**
accumulated during this run. Context fill uses the latest model request input,
not summed usage across tool rounds. Manifest token estimates include tool schemas
and a rough image allowance; provider-reported usage remains authoritative.
`/attach` opens a searchable project file picker; `/attach path` also works.
Text attachments are saved UTF-8 snapshots, capped at 32 KiB per file and
128 KiB in total. Images (PNG, JPEG, GIF, WebP) attach as pictures instead:
up to 1 MiB each and 2 MiB in total, relayed to the model as image blocks —
no codec decodes them, they are bytes passed through — and shown in the
transcript as `[image: png · N KB]`. They become part of the conversation
log, so a resumed session still has them. Remove attachments in `/context`;
removing one changes future prompt context, and does not remove material
already present in conversation history.

### Crash recovery

A kill mid-turn used to leave three loose ends; all three are handled on
reopen now, and the handling is invisible until it matters.

**The log repairs itself.** A killed append can leave a torn final line;
opening truncates it — the alternative was silent record loss, because the
next append would glue itself onto the fragment. A *complete* line that is
not JSON is corruption: the original log is backed up beside it, the line is
dropped, and a visible note records the repair. Lines from a newer version
are left alone: forward compatibility is not corruption.
`log_fsync = "always"` upgrades every append from process-crash consistency
to power-loss survival.

**Interrupted tools are answered, never re-run.** The tool call record is
written before the command executes, so a call with no result means the
effect may have landed. Reopening answers it with an `interrupted` tool
result — "may have partially run; check the workspace before retrying" for
`bash`, "no side effects" for `state` — so the provider transcript stays
well-formed (`tool_use` always pairs with `tool_result`) and `/retry`
cannot silently re-execute a half-landed command.

**Partial output survives.** While a turn streams, the assistant text, the
tool in flight, and any retry deadline are committed to a
`<session>.live.json` sidecar at most every `live_flush_ms` (default 250;
0 disables) — and immediately when a tool starts, when a retry begins, and
before the turn's final records. The sidecar is deleted the moment the turn
ends, however it ends, so a leftover file means the process died. The next
open shows what happened, records the partial as the assistant message it
was becoming, and lets `/retry` continue the thought instead of restarting
it.

`gc attach` takes a cursor too: a client that kept one receives only entries
newer than it (`{"t":"attach","since":N}`), with an explicit reset when the
cursor is stale, and prompt frames carry a `request_id` so a retry over a
flaky link cannot submit the same prompt twice.

### Context edits

```text
/forget 3                      # hide turn 3 from future model context
/forget                        # list turns and their edit state
/rewrite 3 condensed wording   # replace turn 3's user message for the model
/unforget 3                    # restore it
```

Turns are append-only, so these commands delete nothing: each appends an
edit that changes only what the model sees next. The transcript, exports,
and the raw log keep the original, files on disk were never affected either
way, and the latest edit for a turn wins — `/unforget` is just another edit.
This is pi's `context_edit` idea without the session tree: the log is the
escape hatch, not a rewind. `/context` lists active edits.

### Program status

glass-core tells the terminal what it is doing with the Program Status
Protocol (OSC 7501): `working` for a turn, `done`/`error` when it ends,
`idle` at the prompt and after a cancel, and `blocked` for the project-trust
modal (`permission`) and `/login` (`auth`). Reports never contain prompts
or model output — the message is the session title, or the first line of an
error. `program_status = "off"` disables them; they are emitted only on an
interactive terminal, so pipes, CI, and headless runs stay byte-clean.
Unlike pi, there is no reply handshake: the input layer reads through
crossterm and cannot observe the reply, and the spec explicitly permits
reporting without detection (terminals that do not know the sequence ignore
it).

Tool activity shows running/completion state and elapsed time. `/details` searches
recorded tool names, commands, and inline output. Its detail view can copy the
exact original command or recorded inline output; it identifies spilled or
truncated output without loading an unbounded file. Every task ends with a
compact command/file-change summary. `/review` shows the final answer, actual
shell commands and exit statuses, and the checkpoint diff. It does not infer
that tests passed from an agent's completion message.

`/export [path]` writes the raw conversation to Markdown — user and assistant
turns, commands with their exit status, and folded tool output, with fences
that content cannot break; reasoning stays out unless asked for. The raw log
is the source, so the export includes turns compaction later dropped.
`gc export <id> [--out PATH] [--thinking] [--json]` does the same headlessly,
printing to stdout when no output path is given.

Failed streams preserve partial assistant text. `/retry` offers retry, provider
and model selection, or sign-in, then resumes the existing context. During
recovery, identical tool name/arguments with an existing result reuse that result
even when the provider issues a new call ID. Completed actions are not replayed.
HTTP errors, including unavailable models and rate limits, remain visible.

### Small terminals and editing

Modal selectors keep the current selection visible and drop decorative rows on
short screens. Press F1 or Right to read a selected item's full label. Information
views wrap and scroll with arrows, PageUp/PageDown, Home/End. Fields display an
insertion caret and mask secrets. The composer keeps a bounded viewport around
the cursor for long pasted text and clears rows when a draft shrinks. Width
calculations account for wide Unicode characters and combining marks.

Ctrl+R (or `/history`) searches prompt history. Ctrl+X (or `/editor`) uses
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
sessions, response recovery, and cancellation. Real ChatGPT browser authorization
and paid account inference are intentionally left for the user's later sign-in.

### Steering

While a turn is running you can still type. The two inputs mean different
things and conflating them would be worse than having neither:

- **Enter steers.** "Stop, use Postgres not MySQL." The turn ends at the next
  safe point — *before* the next command runs, not after — and the correction
  enters the conversation so the model reacts to it.
- **Alt+Enter queues.** "Then update the docs." It runs automatically when the
  current turn finishes, so a multi-step plan does not need re-submitting.

A steer abandons the tool result it interrupted. Feeding output from a command
you just cancelled back to the model would push it toward the plan you rejected.

Both need the input thread: the loop must stay able to read keys while a model
is streaming and a command is running, which is why `crates/glass/src/input.rs`
owns stdin on its own thread and the loop drains it non-blockingly.

### Turn notifications

Long turns are meant to be walked away from. When a turn that ran at least
five seconds finishes — or any turn fails, however fast — glass-core gets
your attention. `notify = "auto"` sends OSC 777 on terminals that surface it
(Ghostty, WezTerm), OSC 9 on the rest of the terminals that do (iTerm2,
kitty, VS Code, mintty), and rings the bell everywhere else; `"osc"`,
`"osc777"` and `"bell"` force one mechanism; `"off"` silences it. Cancelled
and steered turns never notify, because you are already at the keyboard, and
a queued follow-up defers the alert until the work is really done.
Notifications are interactive-only; headless runs stay silent. The message
carries the turn duration and session cost and never includes provider text
that could smuggle terminal controls.

## Sessions

One append-only JSONL file per session under `$XDG_DATA_HOME/glass-core`. The same
file is the crash-recovery mechanism, the resume mechanism, and the source for
the transcript viewer — which is what keeps the two renderers from drifting.

```sh
glass-core list          # sessions, newest first, with date and title/first message
glass-core search <pat>  # every session whose messages match a pattern
glass-core resume
glass-core resume <id>
```

Prompt history is persisted too: `up` recalls prompts from earlier
sessions, capped at 200.

## Rendering: one owned screen

The interactive app owns one alternate screen for its lifetime. The shell's
primary screen and scrollback are restored on exit. Provider output, activity,
and composer have separate owned rows; resize repaints the app surface so
terminal reflow cannot turn draft text or decorative rules into history.

The rendered viewport cache is capped at 1 MiB. Full conversation records stay
in the private session log and are reachable through Ctrl-G or PageUp. Shared
modals and live viewers use the same screen; they do not nest alternate-screen
switches. Prose uses a neutral foreground, emphasis and code use coordinated
colors, and tools show their actual commands and bounded output previews.
PageUp or Ctrl-B scrolls the conversation in place; PageDown or Ctrl-F moves
toward newer messages. Mouse-wheel scrolling works in terminals that forward
mouse events. The composer stays editable while you read. New output preserves
your reading position and shows an update count; End or Esc returns to the
latest content. Ctrl-G opens the dedicated transcript viewer.
The user's prompt is marked by a cyan `❯` and printed once
— the typed line is replaced, not echoed — so a prompt is
never recorded twice.

Narrow terminals — a phone over ssh — get the same
language in less space: the banner splits into short
lines, tool commands and results wrap in bounded previews
(the full text stays in the transcript), and
the usage accounting takes two rows instead of one,
so no line wraps mid-number. A terminal that reports
a zero size mid-resize is read as 80x24 rather than
wrapping the editor at zero columns.

`ctrl-g` opens the transcript while idle or during provider/tool work. Work
continues while it is open; Ctrl-G or Esc closes it and restores the draft.
Ctrl-T or `/thinking` brings provider-exposed reasoning or a public summary to
the front. Unsupported providers show an honest empty state. Up/Down, paging,
Home and End navigate; End follows arriving text. Ctrl-C cancels active work
even from an overlay. Primary output is privately spooled while an overlay is
open, then replayed once when it closes. Fixed composer and activity rows stay
separate from conversation. A short accent divider fits even 20-column phones.

## Side questions

`/btw <question>` asks the model something mid-task and prints the answer
without recording either. The side request forks the last ten turns, runs with
no tools and an explicit instruction not to write, and costs are printed rather
than hidden.

Verified: after a side question the session log is zero records, so the next
turn costs exactly what it would have anyway.

## A second opinion

`extensions/oracle` adds `/oracle [question]`: the current working-tree diff
is sent to a pinned alternate model (Amp calls this an oracle; Claude's
`/advisor` is the same idea), the critique prints inline and is not
recorded, and the reviewer runs through the read-only subagent path — it
gets the inspection tools and no shell, so a review can look at everything
and change nothing.

```toml
extensions = ["python3 ~/.config/glass-core/ext/oracle.py"]
```

Configure the model in `~/.config/glass-core/oracle.json`
(`{"model": "claude-opus-4-5", "prompt": "optional extra instruction"}`, or
`GC_ORACLE_MODEL`). Prices are the alternate model's, metered to the session
ledger like any other request. `/oracle` without a configured model says so
rather than guessing.

## Better edits, opt-in

`extensions/hashline` adds `hl_read` / `hl_edit`, which refer to lines by a
short content anchor rather than by re-typed text:

```
1:37f9b8e4|fn main() {
2:aa5282df|    println!("hi");
```

A stale anchor is rejected before anything is written, and a batch of edits is
all-or-nothing. See `extensions/hashline/README.md` for why this is an extension
and not a third core tool.

## Plan tracking

`extensions/todo` adds a `todo` tool, the way t3 codes ships it. The model
keeps its own plan: `add` items as it breaks a task down, `check` them off as
each finishes. The list lives in the extension process for the session, so it
costs nothing to keep and disappears when the session ends. The tool returns the
whole list after every change, so the model reads the state it just produced
rather than making a separate call.

```sh
extensions = ["python3 ~/.config/glass-core/ext/todo.py"]
```

It is an extension for the same reason hashline is: a plan is a capability, not
a core tool, and shipping it out of process keeps the two core tools to two.

## Delegation

`extensions/subagent` adds a `subagent` tool: delegate one to eight
self-contained tasks and get back their reports. Every task runs a full turn
loop with its own isolated context, so scratch work never enters your
conversation — only the reports do. Sub-agents carry the two core tools
(`bash` and `state`) and nothing else; the work happens in your workspace, so
a sub-agent can edit the same files you can.

Independent tasks in one call run in parallel, up to four at a time, and the
reports come back in request order. Fan-out happens in the host, not in the
extension, because one extension is one serial stdio pipe — the extension
declares the tool and forwards the batch, and the nested loops run in
glass-core where the provider and the tools already live. A sub-agent is never
a second harness to maintain.

```toml
extensions = ["python3 ~/.config/glass-core/ext/subagent.py"]
```

Tasks may opt into two kinds of isolation:

- **A named agent** runs the task with a saved prompt (and optional model):
  `~/.config/glass-core/agents/*.md` with `name`, `description`, optional
  `model` frontmatter and the body as the sub-agent's system prompt. See the
  definitions with `/agents`.
- **`worktree: true`** runs the task in a detached git worktree at HEAD, so
  edits cannot collide with your working tree. Checkouts live under
  `~/.local/share/glass-core/subagents/worktrees/`; uncommitted parent
  changes are not present, and nothing is merged back automatically —
  reclaim the work with `glass-core worktree apply` (below).

A task that fails — or whose worker panics — produces an error in its own
report slot and never aborts its siblings. Every report comes back wrapped in
`<subagent-report>` tags, so report prose cannot read as instructions you
gave: delegated text is generated in a context you never typed into, and the
frame is what marks the boundary.

### Getting work back out of a worktree

```sh
glass-core worktree list [--json]          # name, changed files, age
glass-core worktree apply [<name>] [--remove]
```

`apply` stages everything inside the checkout (`add -A`, so new files are
included), takes a binary-safe diff against its HEAD, and applies it to the
workspace's working tree with `--3way`, which also stages the destination
patch. Inspect the staged diff before committing. Conflicts are left in place with
git's markers and reported — nothing is committed, silently merged, or
thrown away; committing stays `/git`'s job. Without a name, the newest
checkout is used; `--remove` deletes the checkout after a clean apply.
Refuses to cross repositories.

## Web access

`extensions/web` adds `web_fetch` and `web_search`, opt-in like every other
capability:

```toml
extensions = ["python3 ~/.config/glass-core/ext/web.py"]
```

`web_fetch` returns one URL as readable text — HTML stripped to prose with
links kept inline, scripts and styles dropped, the body capped at 512 KB and
the text at 20,000 characters (tunable per call). It is a text browser; raw
bytes, headers, and POSTs remain `curl`'s job. `web_search` defaults to
DuckDuckGo's HTML endpoint with no key required; point it at a SearXNG
instance in `~/.config/glass-core/web.json` when you have one. Keys belong in
that file: the host strips `*API_KEY*`/`*TOKEN*` variables from extension
processes, so environment variables cannot work there. A blocked or
redesigned backend degrades into an explicit message, never a silent empty
result.

## MCP tools, out of process

`extensions/mcp-bridge` makes Model Context Protocol servers callable without
the core growing an MCP client:

```toml
extensions = ["python3 ~/.config/glass-core/ext/mcp_bridge.py"]
```

`~/.config/glass-core/mcp.json` lists stdio servers:

```json
{
  "servers": {
    "files": { "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"] }
  }
}
```

Each tool appears namespaced as `mcp__<server>__<tool>` with its JSON Schema
passed through. Past 40 tools the bridge switches to `mcp_search` and
`mcp_call`, so a large server does not inflate every prompt. A server that
fails to start is skipped with a note on stderr; calls into a dead server
return an error string, never a hung turn. Transports: stdio and Streamable
HTTP (`{"type": "streamable-http", "url": …, "headers": {…}}`, HTTPS for
non-loopback endpoints); the deprecated HTTP+SSE transport is refused with a
pointer. Servers are negotiated with the stateless 2026-07-28 revision when
they support it — `server/discover` probed first, per-request `_meta` after —
and fall back to the classic `initialize` handshake for everything older.
Nothing spawns unless `mcp.json` asks for it.

### Agent Plugins

glass-core loads [Agent Plugins 1.0](https://agent-plugins.org), the
cross-vendor packaging standard for skills and MCP servers:

```sh
glass-core plugin add <path|git-url>   # installs under ~/.config/glass-core/plugins/
glass-core plugin list [--json]        # skills, MCP, and post-install drift
glass-core plugin remove <name>        # its data directory is kept
```

A plugin's `skills/` join the skill catalogue and its `mcp.json` servers load
through the mcp-bridge extension (stdio in this version). Install is
user-level, like extensions: a repository cannot add a plugin, so cloning an
untrusted repo changes nothing here. Validation follows the spec's failure
boundaries — unknown manifest fields are reported and ignored, a bad schema
or name rejects the plugin, and one broken skill or server never stops the
rest from loading. Plugin-relative paths (`./`, `${PLUGIN_ROOT}`,
`${PLUGIN_DATA}`) are containment-checked against the plugin root, and each
plugin gets a persistent data directory at `plugins/data/<name>/`. `plugin
list` marks a plugin whose tree changed since install.

## Context management

Compaction is two-tier, because frontier models degrade on long inputs even with
reasoning enabled (Chroma's context-rot study found this across all 18 models
they tested, with task complexity held constant).

1. **Deterministic, free.** Above 78% of the context window, drop the middle of
   the conversation and keep the opening task, the model's own `state` notes,
   and the last few turns. No API call.
2. **Model summary, paid.** If that still does not fit, ask the model to
   summarize the transcript. Higher quality, non-deterministic, and costs a call.

A safety net runs underneath both: oversized tool results are replaced with
placeholders, preserving head and tail. Tool output is where the tokens are.

`/compact [focus]` compacts on demand, below the threshold — a manual
compaction is a statement about what you want, not a measurement — and the
optional focus steers what the summary keeps. The summary request serializes
each record with a 2,000-character ceiling, so it always fits the window it
is replacing.

Three failure lessons from other harnesses are built in. When a provider
refuses a request as too large, the harness evicts oversized tool results
anywhere in the conversation and drops the middle once, then retries — never
re-streaming the history that just did not fit (Crush's permanent-wedge
cascade). Three consecutive summary failures open a circuit breaker that
stops paid summarization for the session while deterministic compaction
keeps running, instead of a permanent lock after one failure. And an
oversized tool result is only ever replaced with an explicit placeholder,
never silently dropped.

## No permission system

`bash` runs what the model asks for. There is no allowlist, no prompt, no
sandbox. This is a deliberate choice for a single-user tool and it is the
opposite of what every vendor ships by default, so it is worth being explicit:
**run glass-core somewhere you are willing to lose.**

If you want gating, the `Mod_::authorize` hook is where it goes. The shipped
`noisy` mod blocks `rm -rf /` in about five lines as a demonstration.

### Optional sandbox

For unattended runs and untrusted repositories there is an opt-in, kernel-level
middle ground that needs no prompts:

```toml
sandbox = "workspace"   # "off" (default) | "workspace" | "strict"
```

`workspace` confines every shell command — the `bash` tool and `/bg` jobs — to
writing only under the workspace, the temp directory, `/var/tmp`, the
glass-core data/config dirs, `~/.cache`, `~/.cargo`, and `/dev/null`; reads are
unrestricted. `strict` additionally narrows reads and execution to an allowlist
(the workspace, system directories, `~/.cargo`, `~/.rustup`, `~/.cache`), which
is deliberately breakage-tolerant: a tool that reads somewhere unusual will
fail, and that is the point.

With confinement on, two more boundaries apply. **Egress**: outbound TCP is
limited to an allowlist of ports, `sandbox_ports = [443, 80]` by default
(empty denies all outbound TCP); binds are unrestricted so local test servers
still work, and DNS is unaffected because Landlock covers TCP, not UDP. This
needs Landlock ABI 4 (Linux 6.7+); older kernels warn and leave the network
open rather than pretending. Per-port is all the kernel offers — there is no
host filtering, so 443 to anywhere is allowed. **Credentials**: variables
whose names look like secrets (`*TOKEN*`, `*SECRET*`, `*PASSWORD*`,
`*API_KEY*`, `AWS_*`, `GH_*`, `SSH_AUTH_SOCK`, …) are removed from shell
children, so the model cannot read the harness's own API key out of `env`;
`sandbox_env = "inherit"` restores the full environment.

With the default unrestricted permission profile, extensions are trusted local
programs. The opt-in `workspace-edit` and `read-only` profiles also confine
extensions and inherited MCP children using reviewed capability sidecars. See
[security confinement](docs/security-confinement.md) for enforced boundaries and
platform limits; missing strict declarations refuse startup.

Linux uses Landlock via raw syscalls (kernel 5.13 or newer; no new
dependency). macOS wraps commands with `sandbox-exec`; network restrictions
are not implemented there yet (warned once). A platform that cannot deliver
the requested policy warns once at startup; `strict` refuses to run commands
at all rather than silently downgrading to no confinement. `/security` shows
the active state.

## Extending

Three mechanisms, in increasing order of power.

**Project instructions** are read after consent. If the workspace has an
`AGENTS.md`, `AGENTS.override.md`, or `CLAUDE.md`, it goes into the system
prompt — but only once you allow it for that workspace. The decision is keyed
by a content hash, so edited instructions ask again rather than riding on the
old approval, and a denial persists until the content changes.
`trust = "always"` in config restores the pre-gate behavior; headless runs
skip untrusted instructions with a warning on stderr unless `--trust` is
passed. One file per directory (first match wins), 8KB per file, 12KB total,
truncated with a visible marker rather than silently filling the prompt. No
ancestor walking, because that is what makes the feature unpredictable. A
compile-time assertion fails the build if the budget is raised past 16KB.

**Extensions** are external processes that speak newline-delimited JSON on
stdio. This is the important design decision: an extension's dependencies never
enter our address space. pi ships 40 MiB because its extensions run in-process
and it bundles QuickJS, a TypeScript loader, and an image worker. Ours ships
2.4 MiB because `codemode` is a Node script you already have Node for.

```toml
extensions = [
  "node ~/.config/glass-core/ext/mine.js",
  "python3 ~/tools/my-ext.py",
]
```

Hooks: `system_prompt`, `run_tool`, `tool_result` (rewriting), `authorize`,
`turn_end`, slash commands, and a nested `call_tool` that lets an extension ask
the host to run a tool while it executes. That last one is what makes codemode
possible.

The declared tool list is frozen once the first turn runs: it is part of the
cacheable prompt prefix. New tool declarations take effect in the next
session; changes are noted once in the transcript. Disabling an extension
still stops its process immediately, so its frozen declarations can remain
present while calls return an unavailable-tool error.

An extension can keep durable state that never enters the model context.
`append_custom` records an entry, `read_custom` reads a kind back, and at
startup the host seeds each extension (`seed_custom`) with the entries it owns
from the session log — so state survives restarts without a side file. Kinds
are namespaced by the manifest name (`my-ext/state`), so one extension cannot
write another's; entries are appended to the log by the harness, in causal
order, and exports show them as `[extension entry: kind]`. An append reply
acknowledges the in-memory queue; the entry reaches the log when the current
tool or turn callback completes. A crash before that drain can lose the queued
entry. Startup seeding reads raw history even after compaction, and ACP
switches rebuild state for the selected session.

Every exchange is bounded by a timeout and a byte cap. A wedged, hostile, or
crashed extension degrades into a warning, never a hung session. A failed
extension is skipped, never fatal. Nothing spawns unless config names it.

Copy `extensions/hello.py` — it implements the entire protocol in 100 lines.

```sh
/extensions      # what is loaded, what tools it adds, what commands
/codemode        # extension-provided commands appear here
```

**In-process mods** implement `glass-mod-api::Mod_`, which has zero dependencies
and defines the hook surface. They stay for anything that needs to be in the
hot path, and they cannot segfault the harness.

**Skills** are markdown files in `~/.config/glass-core/skills/`:

```markdown
---
name: release-check
description: Steps to cut a release. Use when asked to ship a version.
---

Run the full test suite, then tag, then push.
```

Only the name and description enter the prompt; the model reads the body with
`bash` when it matches. No ABI, no dynamic dispatch, no crash surface.

**Mods** implement `glass-mod-api::Mod_`, which has zero dependencies and defines
the entire extension surface — eight hooks covering prompt contribution, tool
authorization, output rewriting, extra tools, render overlays, and key handling.
Mods may add tools but never remove the built-ins, so a prompt written against
the base surface keeps working when one is loaded.

```rust
struct MyMod;

impl glass_mod_api::Mod_ for MyMod {
    fn system_prompt(&self, _: &TurnContext) -> Option<String> {
        Some("Always run `cargo test` before reporting success.".into())
    }
}
```

Mods are loaded by name from a registry rather than as dynamic libraries.
Owning a stable C ABI forever, and accepting that a user mod can segfault the
harness, is not a trade worth making when hooks and skills cover the same ground.

## Performance notes

Two measurements that shaped the design.

**The transcript viewer is virtualized.** The obvious implementation wraps every
entry on every frame, which at 10k messages builds ~200,000 wrapped lines to
fill a 30-row viewport — 38 MiB of heap churn per frame. Instead each entry is
laid out once, cached with a bounded LRU, and only the visible window is built.
Frame cost tracks the viewport, not the history. This was the same bug class as
pi's #9807 ("typing lag at 800+ messages"), so it is a place we beat pi rather
than match.

**Output past 60KB spills to a temp file** and the model is pointed at it, so it
can `tail` or `rg` the rest instead of the tail being discarded. pi does the
same thing; losing the end of a build log is losing the part that says what went
wrong.

Measured on Linux aarch64 with Rust 1.97 (2026-10-07): approximately 3.7 MB
for the stripped binary, 3.7 MB idle RSS, and 7.0 ms median from process launch to the
first prompt in an isolated PTY. Startup excludes provider network requests.

## Layout

```
crates/glass-mod-api   the in-process hook surface, zero dependencies
crates/glass-tui       terminal rendering, input, virtualized transcript
crates/glass           loop, providers, tools, session, compaction, config,
                       extension protocol, AGENTS.md, concurrency, read guard,
                       post-write verification, /btw, provider onboarding,
                       account auth, command modals, budgets, checkpoints, jobs
extensions/            hello.py (reference), codemode, hashline
```

`glass-mod-api` is the stability boundary. If a hook ever needs a new type, the
type is defined there — and it stays dependency-free so that it always can.

## Tests

```sh
cargo test --workspace
cargo build
python3 scripts/smoke-features.py
python3 scripts/smoke-onboarding.py
python3 scripts/smoke-polish.py
python3 scripts/smoke-workflows.py
python3 scripts/smoke-mobile.py
python3 scripts/smoke-chat.py
python3 scripts/smoke-motion.py
python3 scripts/smoke-install.py
python3 scripts/smoke-trust.py
python3 scripts/smoke-sandbox.py
python3 scripts/smoke-init.py
python3 scripts/smoke-web.py
python3 scripts/smoke-mcp.py
python3 scripts/smoke-acp.py
python3 scripts/smoke-serve.py
```

373 tests. The bash tests spawn real processes and assert on timeouts, process
groups, output truncation, and stderr interleaving, because those are the parts
that break silently.

## Review, checkpoints, and undo

Each agent task in a Git workspace automatically saves the working tree before
and after execution. `/diff` shows the last task's file summary and patch,
including new files. `/undo` restores only those task changes to their starting
contents, preserving edits that existed before the task and unrelated later
edits. It refuses any affected file changed after the task. Git's staging index
is left unchanged.

`/checkpoints` lists saved checkpoints. `/diff <name>` and `/undo <name>` select
an earlier task. `/checkpoint` saves an explicit baseline; `/restore <name>`
restores that baseline and saves a recovery checkpoint first. Unlike `/undo`,
explicit restore intentionally replaces subsequent edits. Recovery checkpoints
can also be restored using `/restore`.

Snapshots include tracked files and nonignored new files across the Git repository.
Ignored build outputs and submodule contents are outside their scope. Git objects
are kept under `refs/glass-core/<session>/`; manifests live beside session data in
`checkpoints/<session>/`. This works with uncommitted user edits and an unborn
branch, and does not create commits. Outside Git, the agent still runs but review
and recovery checkpoints are unavailable.

`/code-review [max-findings]` reviews the working diff — staged and unstaged
against HEAD — for correctness bugs, through a read-only delegated agent that
cannot mutate the tree it is judging. Findings, most severe first, land in the
conversation so the model can act on them; a clean tree is reported without
spending a provider call. `/skill-doctor` reports what the skill library costs:
per-skill body bytes (paid only when the model reads the file) and description
bytes (paid every turn in the catalogue), plus each skill's last-modified date.
Usage is not tracked — skills are read with `bash`, and glass does not
instrument those reads — so the report is size and age, not a hit count.

## Session spending limits

Set `cost_limit = 5.0` in config or use `/budget 5` during a session. `/budget`
shows spending and the current limit; `/budget off` disables the limit. Accounting
covers every model request, including delegated tasks, side questions, summaries,
and cache refreshes, using the rate card active for that request. The ledger
persists alongside the session and reloads on resume.

The limit is checked before each request. An in-flight request can cross the
limit; the next request is refused. Models require a known rate card for limits,
and missing usage or unreadable accounting prevents further budgeted requests.
Use `[pricing]` for models without a built-in card. The displayed session cost
includes delegated work and survives model switches.

## Headless execution

```sh
glass-core run "fix the failing build"
glass-core run --json --budget 2 "review this change"
glass-core run --json --resume SESSION_ID "continue the task"
glass-core run --json --resume SESSION_ID --retry --model MODEL_ID
glass-core run --json --attach src/main.rs "review this file"
glass-core sessions --json
glass-core context SESSION_ID --json
glass-core review SESSION_ID --json
printf '%s' 'explain this repository' | glass-core run --json
```

Headless execution uses the same tools, extensions, project instructions,
compaction, and task checkpoints as the terminal. It saves a resumable session.
`--json` emits one JSON object on stdout with session ID, final text, status,
error, cost, and checkpoint error; diagnostics go to stderr. Exit codes are
0 for a finished agent turn, 1 for an error, 2 for an exhausted tool-round limit,
and 3 for reaching the session cost limit. SIGINT/SIGTERM stop owned shell
process groups, save partial responses, emit `status: "cancelled"` in JSON,
and exit with 130/143. Parse and preflight failures also emit a JSON error
when `--json` is requested. `--connection ID` chooses a saved account without
changing the global preference; use `glass-core status --json` to find IDs.
Run interactively over SSH with `ssh -t host gc`; non-interactive runs require
no terminal. Browser sign-in over SSH supports pasting the final loopback
callback URL in `/login`; API keys can also be configured through environment
variables. There is no credential import from another harness. A finished turn is the model's
completion signal; callers should still check their own build or test results.

## Conversation branches

`/branch` forks the current conversation into a new session; `/branch 3` forks
through user turn 3. The original log stays intact. Branching reads the complete
log, so it works after compaction. Conversation branching keeps the workspace
files as they currently are; use a checkpoint restore separately when desired.
Each branch starts its own spending ledger with the current limit.

### Editor integration (ACP)

`glass-core acp` speaks the Agent Client Protocol (v1) on stdio, so editors
that host agents over ACP can drive glass-core natively. `initialize` and
`session/new` bind a glass session to the editor's working directory; prompts
stream back as `agent_message_chunk`, provider reasoning as
`agent_thought_chunk`, tool activity as `tool_call` / `tool_call_update`, and
each turn closes with a `usage_update` and a stop reason. `session/cancel`
interrupts before the next command, kills owned process groups, and answers
`cancelled` rather than an error.

The engine is the same as the terminal and headless paths — tools,
extensions, compaction, budgets, the trust gate, and the sandbox policy all
apply. Deliberately outside the advertised surface: `session/load`, mode
switching, and the client-side filesystem/terminal callbacks, because
glass-core runs its own shell and has no permission prompts by design.
Client-supplied MCP servers are accepted and ignored; connect MCP servers on
the host with the mcp-bridge extension instead.

## Verify after writes, opt-in

`extensions/verify` runs the project's verifier after commands that write
files and appends failures to the tool result the model reads:

```toml
extensions = ["python3 ~/.config/glass-core/ext/verify.py"]
```

Configure `~/.config/glass-core/verify.json` — `{"command": "cargo test
--quiet", "timeout_s": 90, "debounce_s": 8}` — or let it detect
`Cargo.toml` (`cargo check --quiet`) or a `package.json` `typecheck` script.
The verifier runs through the host's bash tool, so the sandbox policy and
output caps apply; it runs at most once per debounce window; failures append
as `[verify exit N]` and the first success after a failure reports
`[verify ok]`. `/verify` shows the active command and last result. A wrong
guess about how to test a project is worse than silence, so anything beyond
the two detected cases is opt-in.

## Cross-session memory, opt-in

`extensions/memory` keeps durable project memory across sessions:

```toml
extensions = ["python3 ~/.config/glass-core/ext/memory.py"]
```

Memories live under `~/.config/glass-core/memory/<project>/` with a
`MEMORY.md` index (capped at 200 lines / 25 KB) that is injected into the
system prompt at session start; topic files are read on demand. The model
records and prunes them with the `memory` tool (`add`/`read`/`list`/
`remove`), and `/memory` shows the index and directory. It is user-level
only — a repository can never add memories that flow into its own prompt,
so this stays outside the trust-gate surface. Use it for conventions,
gotchas, and preferences; transient task state belongs in `state`.

### A session that outlives the window

```sh
glass-core serve                    # owns a session, prints its socket path
glass-core attach --socket <path>   # terminal client; detach without stopping it
glass-core logs <session-id>        # the last 40 recorded entries
glass-core stop <session-id>        # ask the server to exit (socket removed)
glass-core respawn <session-id>     # stop it, then start a fresh detached server
```

`serve` runs the same engine as the terminal — tools, extensions,
checkpoints, budgets, the trust gate, the sandbox policy — and speaks
newline-delimited JSON over a 0600 Unix socket in the data directory.
`attach` finds the newest socket when `--socket` is omitted; it replays the
last 200 entries from the raw log (so compaction does not hide the record),
streams new output as it arrives, submits prompts, and cancels with ctrl-c.
Up to four clients attach at once: each gets its own replay, live output
broadcasts to all of them, any client can cancel, and a closed terminal is
dropped rather than stalling the session — mirroring one session into two
panes is the point. Access is same-user on a 0600 socket, which is the whole
auth story; there is no TCP listener. Detaching never stops the server, and
SIGTERM shuts it down cleanly. `stop` asks over the same socket: a turn still
running finishes first, then the server shuts down and removes its socket.
`respawn` stops the server if one is running and starts a fresh one for the
same session in its own process group, so it outlives the shell; `logs` reads
the durable log without repairing it or consuming live recovery data, and
works whether or not a server is running. `stop` and
`respawn` take the newest served session when the id is omitted. Slash
commands and modals remain in the interactive session.

## Messaging another session

```sh
glass-core send <session-id> "the migration finished, rebase whenever"
glass-core send --latest "pick up where I left off"
```

```text
/send <session-id> <message>
```

`sessions are separate universes` was true until you wanted two of them to
coordinate. A message is appended to `<data>/inbox/<id>.jsonl` (0600); a
running session reads its mailbox when **idle** and turns each message into a
normal turn, prefixed by a `[inbox] message from session …` note so the model
knows where it came from — the same idle path loops use, so messages never
stack behind a running turn. Messages are capped at 32 KB, targets are
validated (they are filenames), and a writer mid-append is never truncated or
skipped.

## Background commands

```text
/bg cargo run
/jobs
/job 1
/job 1 stop
```

Background commands run in the configured workspace while the agent remains
available. `/jobs` shows IDs, process status, command, and log path; `/job <id>`
prints the last 60KB of output. `/job <id> stop` stops the command's process group.
Live jobs are stopped on normal terminal exit. Logs stay under session data in
`jobs/<session>/` for inspection. Jobs belong to the running terminal process and
are not reattached on resume.

## Outlining before reading, opt-in

`extensions/index` adds an `index` tool: a file's skeleton with exact line
numbers (`index {path: "src/agent.rs"}`), or the files containing a literal
plus their outlines (`index {query: "retry_backoff"}`):

```toml
extensions = ["python3 ~/.config/glass-core/ext/index.py"]
```

Reads dominate token spend, so the point is reading a range instead of a
whole file; the system prompt tells the model to index first. Regex
scanners for rust/python/go/js/ts (unknown types say so rather than guess),
ripgrep when available, 60 lines per file and 200 per call, path-contained
to the workspace.

## Compiler feedback after edits, opt-in

`extensions/lsp` puts a real language server's verdict into the tool result
the model is about to read:

```toml
extensions = ["python3 ~/.config/glass-core/ext/lsp.py"]
```

```json
{
  ".py": {"command": ["pylsp"], "timeout_s": 20},
  ".rs": {"command": ["rust-analyzer"], "timeout_s": 30}
}
```

in `~/.config/glass-core/lsp.json`. After a command writes a file of a
configured language, the server is started lazily, the file is opened, and
diagnostics are appended as `[lsp] path:line:col severity: message` (errors
first, 20 cap) — the original tool output is preserved. Servers stay warm
for the session and shut down on exit. An explicitly requested `lsp {path}`
tool and a `/lsp` status command exist for the warm-up case; a server that
cannot start is reported once and then ignored, because a missing language
server must never break the edit loop.

## Loops

```text
/loop 30m check the deploy and report
/loop                 # list: interval, run count, next fire, prompt
/loop rm 1            # remove one
/loop clear           # remove all
```

A loop is a prompt that fires again on an interval while the session is
**idle** — never mid-turn, never stacked, and never into a composer you are
typing in. Missed intervals skip forward: a session resumed after a day
fires each loop once, not 2,880 times. Loops live in a sidecar next to the
session (`<id>.loops.json`) and survive resume; with `serve` running, a
detached session keeps its loops going, which is the autonomy story without
a daemon: schedule, walk away, reattach to the results. Intervals are `90s`,
`30m`, `2h` (bare numbers are seconds, zero is refused), capped at 16 loops.

The feature smoke check uses a local mock endpoint and a real terminal process;
it makes no paid API calls:

```sh
cargo build
python3 scripts/smoke-features.py
python3 scripts/smoke-install.py
```

### Persistent teammates

Open **`/agents` or Alt+A** for a live roster, main/worker interaction history,
and scrollable transcripts. Tab switches views; `?` lists actions. You can
message a worker, interrupt it, and resume its saved context. Idle agents make
no model requests. Reviewers are read-only unless explicitly given `--write`.

The main agent uses the existing shell tool, so the default tool surface stays
`bash` + `state`:

```sh
"$GC_BIN" agent create review "Review src/parser.rs for EOF bugs"
"$GC_BIN" agent wait review 10
"$GC_BIN" agent send review "Also check the changes to recovery.rs"
```

From another SSH shell, use `gc agent --session MAIN_ID list` and the same
controls. `gc agent --help` lists model selection, token budgets, worktree
isolation, transcript pagination, and lifecycle commands. Restart the main
session to restore its roster; uncertain work is parked for explicit review.

Two workers run at once. Transcripts load in bounded pages; complete histories
are not automatically copied into model prompts. Usage and cost are shown only
when known, with separate reservations for interrupted requests. See the
[implementation contract](docs/plan/persistent-agents.md) for exact bounds and
transport cancellation behavior.

### CLI help

Run `gc help` for a quick start. Use `gc help commands`, `gc help run`, or
`gc help keys` for focused command, headless, and keyboard guides; `gc help all`
shows the full guide. Help adapts to terminal width and uses plain text when
piped or when `NO_COLOR` is set.
