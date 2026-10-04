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

Two mechanisms, both deliberately minimal.

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

## Layout

```
crates/glass-mod-api   the whole extension surface, zero dependencies
crates/glass-tui       terminal rendering, input, transcript viewer
crates/glass           loop, providers, tools, session, compaction, config
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