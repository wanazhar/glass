# Prompt caching and compaction: research notes

Collected October 2026. Primary sources only; every number below is from vendor
documentation unless marked otherwise.

## The one thing that decides everything

Caches are **prefix hashes**. A cache entry is the hash of everything up to and
including a block marked `cache_control`. Anthropic:

> "Because the hash is cumulative, covering everything up to and including the
> breakpoint, changing any block at or before the breakpoint produces a
> different hash on the next request."

> "The lookback does not find stable content behind your breakpoint and cache
> it. It finds entries that earlier requests already wrote."

Consequence: **compaction destroys the messages cache.** Replacing the middle of
the message list changes the prefix from the first edited block onward. OpenAI
says the same outright — "Summarization, compaction, or context truncation can
change the prefix and reset cache reuse" — and ships a diagnostic reason called
`context_compacted`.

What survives compaction is only what sits *before* the first edit: tools and
system. Multiple breakpoints do not help, because an entry at position N covers
1..N; editing block k < N kills the entry at N. A second breakpoint at j < k
survives, but it only preserves the prefix before the edit, which is the
tools+system prefix we would already cache separately.

## Cost model

Anthropic (platform.claude.com/docs/en/build-with-claude/prompt-caching):

| | multiplier |
|---|---|
| cache write, 5-minute TTL | **1.25x** base input |
| cache write, 1-hour TTL | **2x** base input |
| cache read / refresh | **0.1x** (0.025x Fable 5.1 / Mythos 5.1, 0.05x Opus 5.5) |
| uncached input | 1.0x |

Cache breakpoints themselves are free. Only the content costs.

Break-even, per turn, with `p` tokens of cached prefix and `n` new tokens:

```
cached:    p * 0.10  +  n * 1.25
uncached: (p + n) * 1.00
```

Setting them equal: `p = n * 0.25 / 0.9 = n * 0.278`.

**So caching pays as soon as the cached prefix exceeds ~28% of one turn's
worth of new tokens.** At our turn size that is ~2,200 tokens. Turn 1 always
loses (you pay 1.25x for a write and read nothing); from turn 2 the ratio
falls fast — measured at 8k tokens/turn: turn 2 = 0.68x, turn 5 = 0.33x,
turn 25 = 0.15x of no-cache cost.

This is the opposite of the intuition that "caching is always cheaper". A
short-lived prefix loses money.

## Minimum cacheable prefix — where glass stands

glass caches 682 tokens: 370 system prompt + 312 tool schemas.

| model family | minimum | glass (682) |
|---|---:|---|
| Opus 5.5, Sonnet 5.5, Fable 5.1, Mythos 5.1, Opus 5, Sonnet 5, Fable 5, Mythos 5 | 512 | caches |
| Opus 4.8, Sonnet 4.6, Sonnet 4.5, Opus 4.1, Opus 4, Sonnet 4 | 1024 | **342 short** |
| Mythos Preview, Opus 4.7 | 2048 | **1366 short** |
| Opus 4.6, Opus 4.5, Haiku 4.5 | 4096 | **3414 short** |

A prefix below the minimum is not cached and **no error is returned** — you
just pay full price. On Opus 4.5/4.6 or Haiku 4.5, glass's entire static prefix
is currently uncacheable.

## What invalidates a cache

Hierarchy is `tools` → `system` → `messages`; a change at one level invalidates
that level and all after it.

Anthropic invalidates tools+system+messages on: tool definitions changed
(names, descriptions, parameters, **or ordering**), web-search toggle,
citations toggle, speed setting, thinking parameters, `output_config.effort`.
Invalidates messages only: `tool_choice`, images added/removed anywhere,
dropped thinking blocks, `disable_parallel_tool_use`.

**Not listed as invalidators on any provider checked:** `temperature`,
`max_tokens`. Unpublished either way.

xAI is blunter: "Any change to earlier messages breaks the cache. Only append
new messages at the end." Editing, removing, **or reordering** any earlier
message breaks it. For reasoning models, omitting `reasoning_content` is "the
top cause of cache misses".

OpenAI diagnostics reasons: `model_changed`, `prompt_cache_key_changed`,
`service_tier_changed`, `tools_changed`, `text_format_changed`,
`reasoning_effort_changed`, `verbosity_changed`, `context_compacted`,
`input_changed`.

## The 20-block lookback, and why it bites an agent

Reads walk backward at most **20 positions** from the breakpoint looking for a
prior write. On the Claude API a run of consecutive `tool_use` blocks counts as
one position, and so does a run of `tool_result` blocks — which is explicitly
designed so parallel tool calls do not push the previous entry out of the
window on their own.

An agent turn is roughly: user, assistant `tool_use` run, user `tool_result`
run = **3 positions**. So the window holds about **6–7 turns**. Past that, a
single tail breakpoint falls off and you pay a full re-write.

Anthropic's fix, quoted: "Adding a second breakpoint at block 15 starts a second
lookback window there, which finds the turn-2 entry." So place a second
breakpoint periodically, from the start, so a write accumulates there before
you need it.

Qwen states the same constraint with a hard number: if more than 20 content
blocks separate the marker from earlier content, the lookback cannot reach it.
Alibaba's lookback is also 20.

## Tool schema stability

Anthropic: "Verify that the keys in your `tool_use` content blocks have stable
ordering as some languages (for example, Swift, Go) randomize key order during
JSON conversion, breaking caches."

Qwen folds `tools` into the system message for cache computation and ignores
`cache_control` on tools, so tool definitions must be byte-stable: order, field
order, field structure.

Implication for glass: `serde_json` preserves struct field order, so we are
deterministic as long as we do not iterate a `HashMap` when building tool JSON.
Worth a test, because a nondeterministic tool block silently destroys the cache
and no error is ever returned.

## Compaction strategies that preserve something

Ranked by what actually survives:

1. **Compact the oldest turns; keep the tail verbatim.** The moment you compact,
   the pre-compaction cache is gone — accept one write. From then on every
   subsequent turn is append-only, so the post-compaction prefix re-caches
   normally. This is Anthropic's own
   [compaction-keep-recent-turns](https://platform.claude.com/docs/en/build-with-claude/compaction-keep-recent-turns)
   pattern.
2. **Appending a summary instead of replacing preserves the cache but saves
   nothing** — you still pay full input for every summarized token. You have
   bought latency, not money. You cannot have both.
3. **Context editing** (`clear_tool_uses`) removes old tool results without a
   summarization step. Anthropic is honest that it invalidates the prefix and
   advises clearing enough tokens to make that worthwhile. Good fit for an agent
   loop, because file contents and search results are the bulk.
4. **Judge compaction on total input cost, not hit rate.** OpenAI: "Compare
   total input cost before and after compaction: fewer input tokens can still
   save money even when the cache-hit rate falls." A 3k summary is far cheaper
   than 100k of raw tokens at 1.0x vs 0.1x.

## Measurement

Per-request fields: Anthropic `cache_read_input_tokens`,
`cache_creation_input_tokens`, `cache_creation.ephemeral_5m/1h_input_tokens`
(note `input_tokens` is only post-breakpoint; total = read + creation + input).
OpenAI `usage.prompt_tokens_details.cached_tokens` + `.cache_write_tokens`.
DeepSeek `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`. Gemini
`cachedContentTokenCount`. Mistral, Groq, xAI, Qwen, GLM:
`prompt_tokens_details.cached_tokens`.

Diagnostics: Anthropic `diagnostics={"previous_message_id": ...}` →
`cache_miss_reason.type`. OpenAI `prompt_cache_options.comparison_response_id` →
`response.prompt_cache_diagnostics.reason`. Both free, both report only the
first divergence.

Anthropic publishes **no** hit-rate figure. OpenAI's ">90%" is explicitly
illustrative. Groq's 99.3% is a worked example. Treat all published hit rates
as marketing.

## Provider summary

| provider | mode | min prefix | breakpoints | TTL | read | write |
|---|---|---:|---:|---|---:|---:|
| Anthropic | auto + explicit | 512–4096 by model | 4 | 5m / 1h | 0.1x (0.025x–0.05x on newest) | 1.25x / 2x |
| OpenAI ≥5.6 | implicit + explicit | 1024 visible | 4 writes | 30m | 0.1x | 1.25x |
| Gemini | implicit / cachedContent | 2048–4096 | one handle | 1h default | unpublished | input + storage |
| Qwen / DashScope | explicit or implicit | 1024 (512 GLM) | 4 | 5m | 0.1–0.25x | 1.25x |
| DeepSeek | automatic only | unpublished | none | hours–days | ~0.02–0.033x | free |
| Mistral | automatic, `prompt_cache_key` | 64-block granularity | none | unpublished | 0.1x | free |
| Groq | automatic, not disableable | 128–1024 | none | ~2h | 0.5x | free |
| xAI | automatic, `x-grok-conv-id` | unpublished | none | unpublished | unpublished | free |
| GLM / Z.AI | automatic | unpublished | none | unpublished | ~0.2–0.5x (conflicting) | free |

DeepSeek's ~0.02x read is dramatically cheaper than everyone else's 0.1x. For
a long bash-heavy session that is a real cost argument on its own.

## References

- [Anthropic — Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [Anthropic — Cache diagnostics](https://platform.claude.com/docs/en/build-with-claude/cache-diagnostics)
- [Anthropic — Context editing](https://platform.claude.com/docs/en/build-with-claude/context-editing)
- [Anthropic — Compaction](https://platform.claude.com/docs/en/build-with-claude/compaction)
- [Anthropic — Compaction that keeps recent turns](https://platform.claude.com/docs/en/build-with-claude/compaction-keep-recent-turns)
- [Anthropic — Tool use with prompt caching](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-use-with-prompt-caching)
- [OpenAI — Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
- [OpenAI — Prompt cache diagnostics](https://developers.openai.com/api/docs/guides/prompt-caching/diagnostics)
- [xAI — What breaks caching](https://docs.x.ai/developers/advanced-api-usage/prompt-caching/multi-turn)
- [Groq — Prompt caching](https://console.groq.com/docs/prompt-caching)
- [DeepSeek — Context caching](https://api-docs.deepseek.com/guides/kv_cache/)
- [Alibaba — Context cache](https://www.alibabacloud.com/help/en/model-studio/context-cache)
- [Mistral — Prompt caching](https://docs.mistral.ai/studio/conversations/advanced/prompt-caching)
