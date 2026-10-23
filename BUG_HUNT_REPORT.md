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
| Core slash commands | `/` discovers the selected Core session's command catalogue and groups Core commands separately from Glass workspace actions. Glass actions route to workspace surfaces; Core commands retain their runtime dispatch or explicit native-UI handoff. | Rebuilt-launcher PTY passed fresh `/` discovery, `/status`, and `/model` handoff. Core/Glass ownership and collision behavior have unit/render coverage. |
| Conversation | Human, Core, lifecycle, error, and tool activity have distinct labels and compact presentation. Tool rows identify the Core or Glass origin and expandable output. | Current transcript role/tool presentation and scrolling have unit coverage; PTY verified first message and local browser host evidence. A separate visual screenshot review is still pending. |
| Code | Selecting a file refreshes the preview from the selected buffer. The Code view uses the existing syntax highlighter/editor diagnostics; asking from Code attaches a visible, removable, bounded selected-file/range snapshot. Ordinary Agent messages gain no implicit file context. | PTY navigation and unit coverage verify preview refresh, dirty-buffer preservation, explicit context, and Agent focus. Linting remains available only through configured language services. |
| Git | The surface shows branch/upstream and staged, unstaged, untracked, and conflict counts; existing backend actions provide status, diff, stage/unstage, and commit. | The current PTY journey reaches Git and focused unit tests cover summary/diff projection. A full disposable-repository stage/unstage/commit journey remains outstanding; no Git mutation was run against this checkout. |
| Browser and remote viewing | The workspace surface is named Browser, exposes connection/backend state, and has actionable empty-state/navigation hints. SSH forwarding guidance is shown only for loopback URLs and uses explicit host placeholders. | Local fixture through the Core host router verified navigation, observe/action, typed native CSS-limit failure, recovery, and backend provenance. No implicit public exposure. Public Amazon navigation still times out at 60 seconds; native CSS capacity work is tracked separately below. |
| Tasks and Debug | They are no longer primary tabs. Tasks now show Core conversation lifecycle rows separately from Glass workspace task flows. Core host tool descriptions distinguish Glass-managed project processes and configured DAP sessions from Core's one-shot native tools. | PTY checked the contextual Tasks/Debug routes. Core conversation state is the resident-runtime mirror; Glass workspace task flows remain host-owned workflows, not Core's separate `durable/taskGraph` store. That durable graph is not yet rendered in Glass. Debug empty state now names DAP configuration as a prerequisite. |
| Terminal and processes | A project-managed named process is distinct from Core's one-shot bash. Host tool descriptions explain list/log/port/health/start/stop/input semantics. | Verify a harmless process lifecycle and output through the actual workspace; do not print environment secrets. |
| Advanced surfaces | The previous eight-tab bar is reduced to five primary destinations. Tasks, Debug, and settings/services remain discoverable as contextual actions; persisted state is not deleted. | Revisit discoverability in phone-sized PTY frames and confirm no dead primary destinations remain. |
| Native browser engine | The CSS rule ceiling matches the existing 4,096 protocol budget. A rightmost id/class/tag index skips unrelated common selectors; cascade order is preserved and selectors without a safe index remain in the full-match fallback. The 4,096-rule fixture passes: parse+index 256.85 ms, computed styles for 132 nodes 62.45 ms (11.27s total harness). The rebuilt content worker now accepts 513 rules, returns a typed error at 4,097 while retaining the committed revision, and recovers on the following valid navigation. | These measurements cover a bounded synthetic page. Universal/complex selector workloads still use bounded fallback matching; this does not establish broad website parity or anti-bot capability. Chromium remains an explicit compatibility backend. |
| Native CLI stack behavior | No change in this slice. | `cli::runner::tests::native_persistent_owner_reports_unsupported_chromium_settings` reproducibly overflows the test thread stack before browser CSS is reached; the helper’s changes only alter native stylesheet rule storage/matching. Treat the runtime stack overflow as separate and unresolved until a clean-baseline or production-stack investigation proves its scope. |
| GC search and fetch | The authoritative implementation is the monorepo Rust module `crates/glass-core/src/web_tools.rs` (baseline `af869913`). This slice improves declared-charset decoding with explicit fallback warnings, preserves byte-limit errors, and normalizes/deduplicates Exa MCP source URLs while honoring the requested result count and labeling provider/source count. | Nine deterministic Rust tests include local HTTP encoding/oversize/redirect cases and a mocked Exa MCP result set with duplicate tracking parameters. No paid account, external search query, provider summarization call, or Chromium fallback was used. Ranking/freshness quality and public-search latency have not been benchmarked; this is bounded data-correctness work, not a claim of best-in-class search. Separate changes in `/home/ubuntu/work/glass-core` are preserved but are not the shipped monorepo implementation and are excluded from delivery and validation claims. |

## Verification in this working tree

Passed so far:

- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
- Focused Code preview and Git summary unit tests

Still open or limited:

- A native navigation to Amazon still times out after the bounded 60-second content-load deadline. The 4,096-rule fixture and subsequent same-session recovery pass; this does not prove broad public-site support.
- Glass Tasks reflects GC session lifecycle and separately labels Glass workspace task flows, but the standalone Core `durable/taskGraph` API is not yet displayed there.
- `native_persistent_owner_reports_unsupported_chromium_settings` has a separately observed test-thread stack overflow; the native CSS changes do not establish whether it is a baseline/runtime stack issue.
- Real-provider post-usage waiting remains unresolved in issue #64. No provider retries were used for this verification.

Validated in this working tree:

- `cargo test --offline --locked -p glass-dev --lib`: 640 passed, 0 failed.
- Focused `tui_pty` runs through `target/debug/glass`: fresh startup/first send with no implicit file context, slash command discovery and handoff, literal first typing, browser host-router recovery/action, and Agent/Browser/Code/Terminal/Tasks/Git/Debug navigation passed. The navigation journey verifies reachability, not every mutation on every surface.
- `cargo test --offline --locked -p glass-core --bin glass-core web_tools::tests`: 9 passed.
- Native CSS rule-limit, 4,096-rule bounded/computable, and navigation-deadline tests passed with the `native-engine` feature.
- Documentation inventory (1,820 pages/catalog entries), depth, coverage, shortcuts, wiki tests (7), formatting, and diff checks passed. Strict Clippy passed for `glass-dev` lib/bins and for `glass-core` plus `glass-browser-core` with the native engine feature. Rebuilt Glass, Core, and native content worker; `~/.local/bin/glass --version` and `target/debug/glass-core --version` report 0.3.14. The launcher wrapper points at the rebuilt workspace binaries.

