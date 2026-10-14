# Configuration and providers

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Configure

Configuration lives at `~/.config/glass-core/config.toml`, or the XDG config
location. Session data uses the separate XDG data location. Both use the
`glass-core` namespace; configuration from the `glass` project is not imported.

Example for an OpenAI-compatible Chat Completions endpoint:

```toml
provider = "openai"
base_url = "https://api.openai.com/v1"
model = "YOUR_MODEL_ID"
api_key_env = "OPENAI_API_KEY"
max_tokens = 16000
```

For a local endpoint without a key:

```toml
provider = "openai"
base_url = "http://localhost:11434/v1"
model = "YOUR_LOCAL_MODEL_ID"
api_key_env = ""
```

The other adapters are `anthropic` and `openai-responses`. Select an adapter that
matches the endpoint. Protocol compatibility alone does not establish support
for tools, images, reasoning, model discovery, or every streaming event. Set
`image_input = false` for a text-only model to disable native image viewing and
reject existing image context before a foreground, served or ACP request.

`/settings` and `gc settings --json` show effective settings and their provenance.
`/profiles` saves workspace-specific provider/model choices, account references,
instructions, appearance, mods, and extensions. Profiles contain account IDs,
not copied credentials. Administrator requirements and global denials retain
precedence over project and runtime choices.

Common optional fields:

| Field | Purpose |
| --- | --- |
| `max_tokens`, `reasoning_budget` | Request output and reasoning allowances |
| `fallback` | Alternative model IDs on the active connection |
| `cost_limit` | Session spending limit when a rate card is available |
| `mods`, `extensions`, `skills_dir` | Optional hooks, subprocesses, and skills |
| `notify`, `program_status` | Terminal notifications and status reporting |
| `permission_profile`, `sandbox` | Distinct execution-confinement settings |
| `extension_isolation` | Extension startup confinement |
| `log_fsync`, `live_flush_ms` | Session durability and partial-output persistence |
| `mermaid`, `hyperlinks` | Optional diagram and terminal-link presentation |

For security fields and their platform limits, see [Permissions](security.md).

## Model fallback

`fallback` lists alternative model IDs on the active connection:

```toml
fallback = ["ALTERNATIVE_MODEL_ID"]
```

The provider chain can try these models when the primary fails before producing
output, subject to the adapter's eligible failure categories. A visible notice
identifies a model change. Authentication failures are not routed through fallback.
Partial streams retain their recorded state and use recovery instead of automatic
replay of completed tools.

Fallback applies to delegates using the same provider factory. Context-window
planning remains based on the primary model, so configure alternatives with
compatible capacity. See `/status` and `/requests` for the model actually used.

## Prompt caching

The usage footer separates request input/output, session totals, reported cache
usage, and known cost. `/usage` and `/requests` provide more detail. A cache hit
ratio is calculated from provider-reported usage; it is not proof that a
particular source file was read or understood.

Anthropic requests use cache markers for the system block, the latest message,
and a periodic history anchor. Compaction changes the message projection while
retaining the applicable system prompt. Resuming reconstructs that projection
from saved compaction records.

Other endpoints may report different cache fields or no cache information.
Connection changes, tool declarations, instructions, and reviewed knowledge can
change the request prefix. Use `/context` to inspect supplied context and its
changes before attributing a cache miss to a specific feature.

Known model prices come from the runtime's rate cards. For other models, configure
all four `[pricing]` fields: `input`, `cache_read`, `cache_write`, and `output`, in
USD per million tokens. Use the provider's applicable rates. Missing prices remain
unavailable; provider usage and account-plan allowances are separate measures.

## Optional cache warming

The optional cache-warmer extension requests prefix refreshes during idle periods.
The host sends the refresh request and cancels pending warming when a new turn
starts. Refreshes use provider quota and are included in session accounting.
They can add cost even when no user task runs.

Enable the bundled `cache-warmer` through `/extensions`, or configure its command:

```toml
extensions = ["python3 /path/to/glass-core/extensions/cache-warmer/cache-warmer.py"]
```

The extension defaults to `refresh_after_s = 240` and `min_prefix_bytes = 4000`.
These are extension scheduling values, not guarantees of a provider's cache TTL.
`/extensions` shows active values. Leave warming disabled unless the measured
latency benefit justifies the additional requests.

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

## Request pacing

GC coordinates request starts across models and teammates using the same endpoint
and credentials across GC processes on the same machine. The first two requests can start immediately.
Further requests in a tight burst have a minimum 200 ms spacing. Ordinary model
turns usually take longer than this and incur no additional wait. File tools,
commands, and local previews do not use this gate.

HTTP 429 and 503 responses increase request spacing from 2 seconds up to 60 seconds.
Eight successful responses halve that spacing. Five idle minutes clear learned
spacing once any server cooldown has expired. GC honors `Retry-After` seconds
or HTTP dates, and exhausted OpenAI request-limit reset headers, with a maximum
cooldown of 24 hours. Longer waits appear as a provider cooldown notice and can
be cancelled. Credentials are hashed for the in-memory account key and are never
written by the pacer. Small private records under the data directory share timing
and cooldowns across processes; each request holds the account lock only while
reading or updating that record. Explicit stream throttles before answer text or
tool calls also trigger cooldown and bounded recovery. Completed tools and the
request context remain unchanged. Five attempts are allowed before reporting a
persistent refusal; ambiguous transport failures are not automatically replayed.

Kilo documents a [200-request hourly limit per IP for free models](https://kilo.ai/docs/gateway/usage-and-billing).
Other applications on the same IP consume that allowance too. Request pacing
cannot increase an upstream quota; it coordinates GC requests and handles
transient pressure without replaying completed tools.

In selection menus, F1 or Right opens the full entry in a scrollable reader.
Larger menus also show a wrapped selection preview. The slash command palette
uses F1 for complete command details. Escape returns from details to the menu.

### Activity animations and exit confirmation

Use `/animation` to preview Dot, MiniDot, Pulse, Spark, Bounce, Orbit, Grow, or
Blink. Arrow keys move through the live previews; Enter applies the selection.
The picker scrolls on small terminals. An active workspace profile retains the
selected animation. Each frame occupies one terminal cell and uses the existing
100 ms activity timer.

Ctrl-D opens an exit confirmation. Enter exits; Escape returns to GC with the
composer and menu state preserved. During an active turn, confirming cancels the
current request before exit. Saved conversation history remains available;
unsent drafts are discarded on confirmed exit.
