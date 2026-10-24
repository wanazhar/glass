# Glass workspace implementation matrix

This page tracks the workspace work discussed for issue #64. It records the
current code and verification evidence in this working tree; an implementation
entry is not considered complete until its listed end-to-end check passes.

## Ownership contract

Glass is the project workspace and host integration layer. Glass Core owns
agent execution, permissions, provider/session state, and turn lifecycle.
Glass routes human input and Core-authorized host tool calls, then presents the
resulting Core events and workspace resources. Browser revisions, project
identity, and file/resource conflicts remain host validation concerns; Glass
does not add a second agent approval policy.

## Workspace coverage

| Area | Current implementation in this tree | Required evidence / remaining limit |
|---|---|---|
| Agent and navigation | Agent composer is ready at startup; main surfaces are Agent, Code, Browser, Terminal, and Git. Alt+1…Alt+5 and Ctrl+Tab navigate without consuming ordinary typed digits. Advanced Tasks, Debug, and service actions remain reachable through the unified slash/action palette. | Fresh-start, first-send, literal typing, surface switching, Code preview/edit and Core command handoff passed through the rebuilt launcher PTY. |
| Core commands | The Glass CLI forwards unambiguous `glass <core-command>` names and the complete `glass core ...` namespace to the monorepo `glass-core` executable. It never falls back to a `gc` binary or the independent GCN product. In the TUI, `/` groups the live Core catalogue and Glass workspace actions; common supported controls stay in Glass, and unsupported interactive Core controls report unavailable without auto-opening a second TUI. | CLI argument/stdin/stdout/exit pass-through and help have integration coverage. Rebuilt-launcher PTY passed fresh `/`, `/status`, and `/model`; the model picker uses the selected resident session. |
| Conversation | Human, Core, lifecycle, error, and tool activity have distinct labels and compact presentation. Tool rows identify the Core or Glass origin and expandable output. | Current transcript role/tool presentation and scrolling have unit coverage; PTY verified first message and local browser host evidence. A separate visual screenshot review is still pending. |
| Code | Selecting a file refreshes the preview from the selected buffer. The Code view uses the existing syntax highlighter/editor diagnostics; asking from Code attaches a visible, removable, bounded selected-file/range snapshot. Ordinary Agent messages gain no implicit file context. | PTY navigation and unit coverage verify preview refresh, dirty-buffer preservation, explicit context, and Agent focus. Linting remains available only through configured language services. |
| Git | The surface shows branch/upstream and staged, unstaged, untracked, and conflict counts; existing backend actions provide status, diff, stage/unstage, commit, and history. | Disposable-repository PTY coverage exercises stage, unstage, restage, commit, and history. No Git mutation was run against this checkout. |
| Browser and remote viewing | The workspace surface is named Browser, exposes connection/backend state, and has actionable empty-state/navigation hints. SSH forwarding guidance is shown only for loopback URLs and uses explicit host placeholders. | Local fixture through the Core host router verified native start, navigation, observe/action, typed CSS-limit failure, recovery, and backend provenance. A rebuilt native browser plus content worker now navigates read-only to Amazon successfully (revision 2); no Chromium fallback was used. The one-time JS bridge initialization measured about 9.4 seconds on that page; this is one site/run, not broad parity evidence. |
| Tasks and Debug | They are no longer primary tabs. Tasks displays selected Core conversation lifecycle and the selected Core's durable `taskGraph`, separately from Glass workspace task flows. Core host tool descriptions distinguish Glass-managed project processes and configured DAP sessions from Core's one-shot native tools. | Offline Core protocol and PTY coverage verify graph request/rendering without exposing task prompts or checkpoints. Debug empty state names DAP configuration as a prerequisite; actual debug execution still depends on configured adapters. |
| Terminal and processes | A project-managed named process is distinct from Core's one-shot bash. Host tool descriptions explain list/log/port/health/start/stop/input semantics. The unified `/` palette is available from every workspace surface. | A disposable PTY starts a named process via the slash-discovered Glass action, confirms the human-initiated start, observes its exited state, and retrieves captured output through the slash-discovered logs action. No environment secrets are printed. |
| Advanced surfaces | The previous eight-tab bar is reduced to five primary destinations. Tasks, Debug, and settings/services remain discoverable as contextual actions; persisted state is not deleted. | Revisit discoverability in phone-sized PTY frames and confirm no dead primary destinations remain. |
| Native browser engine | The CSS rule ceiling matches the existing 4,096 protocol budget. A rightmost id/class/tag index skips unrelated common selectors; cascade order is preserved and selectors without a safe index remain in the full-match fallback. The 4,096-rule fixture passes: parse+index 256.85 ms, computed styles for 132 nodes 62.45 ms (11.27s total harness). The rebuilt content worker now accepts 513 rules, returns a typed error at 4,097 while retaining the committed revision, and recovers on the following valid navigation. | These measurements cover a bounded synthetic page. Universal/complex selector workloads still use bounded fallback matching; this does not establish broad website parity or anti-bot capability. Chromium remains an explicit compatibility backend. |
| Native CLI stack behavior | The large CLI/native-owner regression runs in an explicit 8 MiB test thread. | This prevents the test harness's default-stack abort and has passed in the serial browser suite. It verifies that harness configuration, not a production-thread stack guarantee; no production stack change is claimed. |
| GC search and fetch | The authoritative implementation is the monorepo Rust module `crates/glass-core/src/web_tools.rs` (baseline `af869913`). This slice improves declared-charset decoding with explicit fallback warnings, preserves byte-limit errors, and normalizes/deduplicates Exa MCP source URLs while honoring the requested result count and labeling provider/source count. | Nine deterministic Rust tests include local HTTP encoding/oversize/redirect cases and a mocked Exa MCP result set with duplicate tracking parameters. No paid account, external search query, provider summarization call, or Chromium fallback was used. Ranking/freshness quality and public-search latency have not been benchmarked; this is bounded data-correctness work, not a claim of best-in-class search. Separate changes in `/home/ubuntu/work/glass-core` are preserved but are not the shipped monorepo implementation and are excluded from delivery and validation claims. |

## Verification in this working tree

Passed so far:

- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
- Focused Code preview and Git summary unit tests

Still open or limited:

- Amazon native navigation now succeeds after avoiding repeated JS bridge rebuilds for unobserved lifecycle events. The first bridge initialization still takes about 9 seconds in the measured run, and this does not prove broad public-site parity or anti-bot behavior.
- Real-provider post-usage waiting remains unresolved in issue #64. A deterministic filesystem-lock fixture demonstrates one possible UI delay: applying Core's settled event waits for durable run-phase classification before publishing the Idle snapshot. Privacy-safe phase timing now brackets that persistence. This does not establish that lock contention caused the reported live wait; no provider retries were used.

Validated in this working tree:

- `cargo test --offline --locked -p glass-dev --lib`: 655 passed, 0 failed; `--bin glass --test cli_forward`: 2 binary tests and 2 CLI forwarding integration tests passed.
- Focused `tui_pty` runs through `target/debug/glass`: fresh startup/first send with no implicit file context, slash discovery and model selection, taskGraph refresh, named Terminal process start/exit/log retrieval, browser host-router recovery/action, literal first typing, and Agent/Browser/Code/Terminal/Tasks/Git/Debug navigation passed. The navigation journey verifies reachability, not every mutation on every surface.
- `cargo test --offline --locked -p glass-core --bin glass-core web_tools::tests`: 9 passed.
- Native CSS rule-limit, 4,096-rule bounded/computable, and navigation-deadline tests passed with the `native-engine` feature.
- Documentation inventory (1,820 pages/catalog entries), depth, coverage, shortcuts, wiki tests (7), formatting, and diff checks passed. Strict Clippy passed for `glass-dev` lib/bins/tests, `glass-core`, `glass-browser`, and `glass-browser-core` with the native engine feature. Rebuilt Glass, Core, browser, and native content worker; `~/.local/bin/glass --version` and `target/debug/glass-core --version` report 0.3.14. The launcher wrapper points at the rebuilt workspace binaries.

