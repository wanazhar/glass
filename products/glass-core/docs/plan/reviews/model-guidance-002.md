# Model guidance review

## Scope

Workflow selection and checklist behavior, current and resumed system prompts, native schema guidance, complete feature discovery, and tool-loop progress reminders.

## Independent review

One GPT-6 Luna reviewer at medium reasoning inspected the implementation. No blocking correctness, API or permission issue was reported. The reviewer noted that state-only sequences do not trigger progress reminders. This is intentional: notes, capability discovery and memory housekeeping should not generate checklist pressure. File reads, execution and other non-state tool results count toward the threshold, including failed attempts.

## Verification

- Workspace tests: 806 core, 13 extension API and 102 TUI tests passed; one existing doctest remained ignored.
- Clippy passed for the workspace and all targets with warnings denied.
- Release checklist PTYs passed at 80×30, 40×24 and 20×8, including resume, human changes, busy browsing, continuation, legacy adapter and approved-plan execution.
- A local SSE fixture verified that execution requests contain one workflow protocol, read-only planning excludes it, and discovery returns the complete catalogue.
- The fixture retrieved the checklist recipe and executed its JSON example successfully against the release binary.

These checks verify delivery and mechanics. They do not prove that every provider model follows the instructions. No live provider behavior claim is made.
