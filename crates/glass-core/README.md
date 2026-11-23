# glass-core

This Rust package builds the `glass-core` terminal coding harness and its
`gc` command alias. Glass uses it as the default resident model-session engine;
the `glass-dev` host owns project access and governed tool execution.
The `gc` alias refers to this Glass Core binary; it does not invoke the
separate GCN product's `gcn` executable.

The source package is part of the root Cargo workspace and shares its version
and lockfile. The product's install workflow, provider setup, authentication,
model selection, and user documentation live in
[`products/glass-core`](../../products/glass-core/README.md). The
[Glass Core product guide](../../docs/glass-core-product.md) explains its
relationship to the rest of the Glass workspace.

This package does not contain `glass-browser-core`, the native browser engine.

## Optional text-block progress

Protocol v2 hosts may request `optionalCapabilities: ["textBlockBoundaries"]`
on `initialize`. Only those hosts receive the acknowledgement
`capabilities.textBlockBoundaries: true` and `runtime/event` notifications with
`params.type: "text_block_start"` and `params.data: {"blockId":"…"}`. A
boundary precedes text from another provider-identified block; there is no
notification for a single text block or ordinary token deltas. A distinct
Anthropic text-block start is a boundary even when empty, so opted-in hosts can
reject empty final answers rather than promote preceding planning text.
The first block is implicit. Block IDs are opaque and scoped to the turn.

Anthropic block indices, Responses item/content identifiers, and typed Chat
content parts supply boundaries. The extension does not infer them from prose.
For opted-in hosts, compatible Chat `thinking`, structured `reasoning`, and
`reasoning_details` text also use the reasoning channel. Content parts marked
`thought: true` or typed as reasoning/thinking are private rather than answer
text. Responses reasoning-text events use that channel too. Encrypted reasoning
and signatures are not exposed. Explicit empty Chat/Responses content-part
starts preserve boundaries just like empty Anthropic blocks.

Legacy provider text deltas, accumulated answers, session records and tools remain
unchanged. Opted-in durable streams flush pending text before each boundary;
legacy hosts see no new events or capability fields. Existing initializers in
the Rust and TypeScript runtime clients remain opted out.

Only `anthropic`, `openai`, and `openai-responses` are provider adapter kinds.
Gemini, OpenRouter, DeepSeek and Ollama require their OpenAI-compatible endpoints;
their native wire formats are not adapters. Anonymous text chunks (including
single anonymous array parts repeated across frames) cannot identify a new block.
No prose heuristic separates planning from an answer in such a stream.
