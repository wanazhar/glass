# glass

A minimal coding harness. Two tools, one loop, native terminal.

```
binary   2.4 MB   (stripped, lto=fat, codegen-units=1)
RSS      ~2 MB    idle
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

## Why two tools

`bash` is the only tool that matters. The model can read, write, search, edit,
build, test, and use git with it, and every tool we don't ship is schema the
model pays for on every single request. `state` exists because with only `bash`
there is no durable place for the model to record what it is doing, and that is
exactly what context compaction needs to survive a long task.

## Prompt caching

glass caches aggressively and shows you the hit rate after every turn:

    12350 in / 3 out · cache 97% (12,000 read / 300 written)

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
extensions = ["python3 ~/.config/glass/ext/cache-warmer.py"]
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
idle RSS and ships an 80MB binary. glass is roughly 50x smaller because it does
not do the things that make it big: no syntax highlighting, no image codecs, no
telemetry, no MCP, no embedded JS engine, and no tokio.

## Install

```sh
cargo build --release
./target/release/glass
```

## Configure

`~/.config/glass/config.toml`. Every field is optional.

```toml
provider = "anthropic"          # "anthropic" | "openai" | "openai-responses"
base_url = "https://api.anthropic.com"
model = "claude-opus-4-5"
api_key_env = "ANTHROPIC_API_KEY"
max_tokens = 16000
mods = []                       # "noisy" is the shipped example
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

## Keys

| key | |
|---|---|
| `ctrl-g` | fullscreen transcript: scroll, collapse, follow |
| `ctrl-c` | interrupt |
| `ctrl-d` | quit |
| `up` / `down` | prompt history (line motion in a multi-line draft) |
| `enter` | steer the running turn |
| `alt-enter` | queue for after the turn |
| `/help` | help |
| `/model <name>` | switch the model for the rest of the session |
| `/extensions` | loaded extensions, their tools and commands |

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

## Sessions

One append-only JSONL file per session under `$XDG_DATA_HOME/glass`. The same
file is the crash-recovery mechanism, the resume mechanism, and the source for
the transcript viewer — which is what keeps the two renderers from drifting.

```sh
glass list
glass resume
glass resume <id>
```

## Rendering: why there is no alt-screen

The default view writes into the terminal's own scrollback. History therefore
belongs to the terminal, which already scrolls, searches, and selects for free,
and the harness heap holds only a bounded diff buffer. pi publishes "a few
hundred kilobytes" for the same design.

Holding history in RAM instead is not free: 10,000 messages at a realistic 1–4KB
average is 10–40MB, which is a harness's entire memory budget before it starts.

`ctrl-g` opens a fullscreen ratatui transcript for people who want a scrollbar,
jump-to-turn, and per-turn collapse. It is opt-in, so you only pay for it when
you actually scroll back.

## Side questions

`/btw <question>` asks the model something mid-task and prints the answer
without recording either. The side request forks the last ten turns, runs with
no tools and an explicit instruction not to write, and costs are printed rather
than hidden.

Verified: after a side question the session log is zero records, so the next
turn costs exactly what it would have anyway.

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
extensions = ["python3 ~/.config/glass/ext/todo.py"]
```

It is an extension for the same reason hashline is: a plan is a capability, not
a core tool, and shipping it out of process keeps the two core tools to two.

## Delegation

`extensions/subagent` adds a `subagent` tool: hand a self-contained task to a
second agent and get back its report. The sub-agent runs a full turn loop with
its own isolated context, so its scratch work never enters your conversation —
only the final report does. It carries the two core tools (`bash` and `state`)
and nothing else, and runs synchronously inside your turn: the work happens in
your workspace, so a sub-agent can edit the same files you can.

```sh
extensions = ["python3 ~/.config/glass/ext/subagent.py"]
```

The heavy lifting stays in the harness. The extension only declares the tool and
forwards the task; the nested loop itself runs in glass, where the provider and
the tools already live. That keeps the extension a thin shell and means a
sub-agent is never a second harness to maintain.

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

## No permission system

`bash` runs what the model asks for. There is no allowlist, no prompt, no
sandbox. This is a deliberate choice for a single-user tool and it is the
opposite of what every vendor ships by default, so it is worth being explicit:
**run glass somewhere you are willing to lose.**

If you want gating, the `Mod_::authorize` hook is where it goes. The shipped
`noisy` mod blocks `rm -rf /` in about five lines as a demonstration.

## Extending

Three mechanisms, in increasing order of power.

**Project instructions** are read automatically. If the workspace has an
`AGENTS.md`, `AGENTS.override.md`, or `CLAUDE.md`, it goes into the system
prompt. One file per directory (first match wins), 8KB per file, 12KB total,
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
  "node ~/.config/glass/ext/mine.js",
  "python3 ~/tools/my-ext.py",
]
```

Hooks: `system_prompt`, `run_tool`, `tool_result` (rewriting), `authorize`,
`turn_end`, slash commands, and a nested `call_tool` that lets an extension ask
the host to run a tool while it executes. That last one is what makes codemode
possible.

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

**Skills** are markdown files in `~/.config/glass/skills/`:

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

Measured on a 2006-era toolchain: 2.4 MB binary, ~2 MB idle RSS, sub-10ms
startup.

## Layout

```
crates/glass-mod-api   the in-process hook surface, zero dependencies
crates/glass-tui       terminal rendering, input, virtualized transcript
crates/glass           loop, providers, tools, session, compaction, config,
                       extension protocol, AGENTS.md, concurrency, read guard,
                       post-write verification, /btw
extensions/            hello.py (reference), codemode, hashline
```

`glass-mod-api` is the stability boundary. If a hook ever needs a new type, the
type is defined there — and it stays dependency-free so that it always can.

## Tests

```sh
cargo test --workspace
```

116 tests. The bash tests spawn real processes and assert on timeouts, process
groups, output truncation, and stderr interleaving, because those are the parts
that break silently.