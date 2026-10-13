# Bounded tool-call repair

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

## Research

- AI SDK tool calling: invalid calls can be returned to the model on the next step; custom repair can re-ask using original context and schema. https://ai-sdk.dev/docs/ai-sdk-core/tools-and-tool-calling
- Anthropic: return instructive tool_result errors with is_error, keep each result paired immediately after its tool use. https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls
- Pi: validate arguments before execution and turn validation exceptions into error tool results. https://github.com/badlogic/pi-mono/blob/main/packages/agent/src/agent-loop.ts

## Contract

A successfully completed provider response containing malformed tool arguments is repairable. Broken/truncated transport, missing call identity and incomplete response remain fatal; no partial stream executes tools. Before execution, validate JSON object arguments and advertised structural schema constraints. No argument coercion or guessed shell commands. Error text includes location or field, no raw argument excerpts.

If any call in a response fails validation, reject the entire batch before side effects, pair every call with an informative tool result, show a repair notice and ask the same model to correct calls in the normal loop. Two correction requests per user turn, also bounded by the existing tool-round/quota budget. A third invalid batch is durably resolved then stops with an explicit repair-exhausted error. Execution errors remain ordinary results, distinct from input repair. Guards and authorization run normally on every corrected call.

Raw model arguments stay in the local call record. Provider history adapters replace malformed/nonobject argument payloads with an empty object on the wire only, allowing valid tool/result protocol replay; the error result explains that this placeholder never executed. No additional tool schemas, dependencies or success-path network calls. Validate supported structural JSON Schema keywords; complex extension constraints (including `$ref`, `pattern`, `format` and exclusive numeric bounds) remain the extension's responsibility. Canonicalize execution arguments after validation so decoded and duplicate JSON keys have identical meaning at both boundaries.

## Verification

Unit and mock-provider integration: malformed JSON, missing/wrong arguments, unknown tools, mixed batches, correction success, exhaustion, no duplicate side effects, interrupted streams and authorization after correction. Cover OpenAI chat and Responses and Anthropic. Release PTYs include 20x8 and interactive follow-up after repair, plus existing scrolling/job/output suites. Implementation requires independent review.
