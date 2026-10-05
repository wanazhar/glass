# glass

A minimal coding harness. Two tools, one loop, native terminal.

```
binary   2.2 MB   (stripped, lto=fat, codegen-units=1)
RSS      ~2 MB    idle
startup  <10 ms
tools    bash + state
```

## Why two tools

`bash` is the only tool that matters. The model can read, write, search, edit,
build, test, and use git with it, and every tool we don't ship is schema the
model pays for on every single request. `state` exists because with only `bash`
there is no durable place for the model to record what it is doing, and that is
exactly what context compaction needs to survive a long task.

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
| `up` / `down` | prompt history |
| `/help` | help |

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
                       extension protocol, AGENTS.md
extensions/            hello.py (reference), codemode, cache-warmer
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