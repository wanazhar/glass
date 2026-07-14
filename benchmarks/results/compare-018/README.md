# Compare-018 acceptance attempts

Each attempt is retained as a self-contained evidence set so its relative raw
report links remain valid.

| Attempt | Purpose | Result |
|---|---|---|
| [`attempt-01`](attempt-01/) | First 100-iteration execution at `251c8ab` | Claim blocked: two harness defects, one adapter initialization failure, and missing prerequisites. |
| [`attempt-02-diagnostic`](attempt-02-diagnostic/) | Reviewed one-iteration adapter diagnostic | Claim blocked: Glass timed out in frame action; MCP left a modal open; Playwright passed 11/11. |
| [`attempt-03-linux-final`](attempt-03-linux-final/) | Reviewed 100-iteration Linux adapter comparison at `5a687a3` | Claim blocked: Glass had seven fail-closed popup races and Playwright MCP retained an open modal; Playwright passed 1,100/1,100. |

No attempt supplies missing revision-bound ratified, release-validation, or
real-browser platform-matrix evidence implicitly. Those gates remain false
unless separately provided in the versioned prerequisite format.
