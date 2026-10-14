# Model guidance

GC teaches tools through three layers: the advertised schemas, a compact workflow protocol, and local recipes loaded on demand. A feature being installed does not mean that every model will choose it correctly. The harness supplies instructions and checks; the model still chooses its actions.

## Checklist behavior

For work with several steps or deliverables, the model is instructed to create `state.todo` before substantive work. Simple answers and greetings do not need a checklist. Resumed work should reuse existing tasks. Progress updates use stable task IDs and can batch several changes. Before the final report, the model should reconcile open tasks and report blockers honestly. Completion does not establish that a test passed.

After twelve non-state tool results without a checklist update, GC can insert one progress reminder during the existing tool loop. It does not issue a separate model request or force another turn. The reminder is limited to one per user turn. Failed tool results count as work too: repeated failures are a reason to reassess the plan. Automatic continuation remains a separate, optional checklist setting.

## Workflow discovery

Use `state` with `{"guidance":{"action":"catalogue"}}` to list CLI entries, human controls and recipe topics. Search accepts a `query`; read accepts a `topic`.

| Topic | When to use it |
| --- | --- |
| checklist | Multi-step work and progress reconciliation |
| files | Revision-checked reads, edits, writes and structural edits |
| background | Long commands, incremental output and owned processes |
| agents | Focused delegation, limits, reuse and report review |
| context | Durable working notes and compaction recovery |
| learning | Recall, save and correct reusable project lessons |
| workspace | Claims, evidence, artifacts, handoffs and budgets |
| research | Web retrieval, source attribution and image inspection |
| integrations | Configured language, debug and MCP tools |
| sessions | Saved conversations, diagnostics, history and handoffs |
| delivery | Reviews, result contracts, worktrees, PRs and skills |
| extensions | Configured tool discovery and deferred activation |
| recovery | Tool failures, stale revisions and provider failures |
| ui | Human controls and authorization boundaries |

Configured extension tools are discovered with `{"capabilities":{"search":"QUERY"}}`. When deferred discovery is enabled, activate returned names with `{"capabilities":{"activate":["NAME"]}}`. Use the resulting advertised schema. Recipes do not promise that an integration is installed. CLI entries use `"$GC_BIN" <command> --help` for exact syntax. Slash commands and launch flags are labelled separately from CLI entries; they are not model tool calls.

## Resume, cost and performance

The versioned workflow suffix is added to the effective system prompt, including prompts restored from compaction. Identical suffixes are not duplicated. Read-only planning retains its restricted prompt and does not receive execution instructions. The suffix is bounded to 1,200 bytes; detailed recipes remain outside the initial prompt. It adds input tokens, but no network lookup or extra request. A software upgrade changes the prompt prefix once; requests within that version use the same suffix.

Catalogue discovery returns all entries instead of silently cutting off after 24. Runtime progress reminders inspect the in-memory current turn, not the session file or the network.

## Research basis

[Codex's public prompt](https://github.com/openai/codex/blob/main/codex-rs/models-manager/prompt.md) gives explicit planning triggers and progress-update rules. [OpenAI's prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide) recommends compact plans for substantial tasks and reconciliation before completion. [OMP's todo guidance](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/prompts/system/eager-todo.md) covers the whole request and avoids redundant updates. GC adopts those principles using its existing state tool and durable checklist.

Deterministic provider fixtures verify instruction delivery and tool lifecycle, including resume and small terminals. They do not establish compliance by every live provider model.
