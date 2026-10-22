# Glass Dev live PTY validation — 2026-10-22

Status: Historical record. This report records an interactive validation pass
against the source checkout on 2026-10-22. It describes observed behavior, not a
release certification. Current guide claims remain in `docs/architecture/` and
`docs/cli.md`.

## Purpose and setup

I launched the debug `glass` binary in a real allocated PTY and drove it with
terminal input. The resident adapter used the source-built sibling runtime:

```sh
GLASS_AGENT_HARNESS=glass-core \
GLASS_CORE_BINARY=/home/ubuntu/work/glass-core/target/debug/glass-core \
GLASS_TRUST_STORE_PATH=/tmp/glass-pty-validation.l839eR/.glass/pty-trust.json \
RUST_MIN_STACK=8388608 \
/home/ubuntu/work/glass/target/debug/glass
```

The browser pass additionally set `GLASS_NATIVE_CONTENT_WORKER` to the current
`target/debug/glass-native-content-worker` and set `GLASS_CONFIG_HOME` and
`XDG_DATA_HOME` to temporary directories under the fixture. No installed Glass
or Core runtime, user project, provider account, remote Git host, or global
configuration was used. A disposable project under
`/tmp/glass-pty-validation.l839eR` supplied a small local HTTP page and a local
Git repository. The project trust record and native browser profile stayed in
temporary paths. The Node server and TUI were stopped through their own PTY
sessions after each run.

The default resident harness rendered Glass Core setup guidance. `:actions`
showed the Core action set, not the Pi installer/authentication entries. An
offline fake Core runtime was used for protocol/UI tests; it was not treated as
a live provider response. Provider login, paid model calls, external harness
execution, and remote GitHub/Git operations were intentionally not attempted.

## Fixes exercised

The Agent action catalog and readiness message now follow the default Core
harness. The maintained TUI guide previously described the old Pi setup routes
as the default; it now distinguishes Core setup guidance from the opt-in Pi
runtime and links to each engine guide.

The full-screen editor keeps its last projected buffer visible while a
workspace refresh holds the shared lock. Key presses and bracketed paste that
arrive during that short contention are queued in order, then applied to the
same buffer when the lock is available. The queue has a fixed bound and reports
when input is waiting. Unit regressions cover cached-projection retention,
ordered key/paste application, and routing bracketed paste into the editor. A
PTY regression opens `src/main.rs`, types `// PTY saved`, saves with Ctrl-S,
and verifies the exact file contents on disk.

An isolated local Git regression stages and commits two fixture edits through
the TUI's confirmation path, then checks the local commit and clean worktree
with Git. The test excludes its generated `.glass/` trust store in that
fixture's `.git/info/exclude`; it does not alter the product ignore rules.

## TUI surface and action inventory

The catalog below was read from the current TUI action definitions and checked
against the visible surface launchers where the PTY suite opens them.
“Exercised” means the underlying route or behavior was actually driven in the
PTY or a focused test. Merely opening a launcher verifies discovery, not
successful execution.

| Surface | Current catalog | Exercised | Not executed in this pass |
|---|---|---|---|
| Trust | Inspect configuration; open read-only workspace; trust once; trust project | `:trust inspect`, then project trust; trust-gated startup verified | Read-only mode and trust-once persistence behavior |
| Agent — Core default | Compose; readiness doctor; Core setup guidance; provider-login guidance; new conversation; list conversations; compact; task loop; create task; resume task; review workspace changes | Composer entry and fake-Core turn; readiness doctor; setup/login guidance; catalog rendered | Live provider call, new/list/compact with a real session, task dispatch, review mutation |
| Agent — legacy Pi | Compose; Pi setup/update/auth; readiness; new/list/tree/compact/rewind; task loop/create/resume; GitHub status; workspace review; external harness list/start/delegate; plan mode | Legacy harness route is catalogued; external-harness launch confirmation is tested without dispatch | Pi installation/auth/provider usage, external harness launch/delegate, GitHub operations |
| Code | Open; edit; replace selection; comment selection; save; jump to App; propose; review proposals; checkpoint; diagnostics | Open file, edit/save through PTY; Alt-A sends focused editor context; editor palette rendered | Replace/comment/proposal/checkpoint/LSP diagnostics route execution |
| App | Start; observe; navigate; type into selected; targets; live view; jump to source; open detected app; record click path | Clean-profile native start; local HTTP navigation and observation; palette; live-view toggle reports pixels disabled | Target interaction, browser type/click, workflow recording, screenshot, JS dialogs, recovery after a real navigation failure, live graphics |
| Terminal | Start detected suite; start dev server; view logs; restart; stop; attach detected URL | Local Node process started, logs viewed, and stopped through Glass; test covers suite discovery result | Restart and attach-to-URL route execution |
| Tasks | Create task; overnight crew; cancel; retry | Tasks surface and launcher reachable | Dispatch/cancel/retry/crew execution; these require resident task work beyond the offline UI fixture |
| Git | Stage all; commit; selected diff; branches; PR review/ship; fetch/pull; push; merge; rebase; review object | Selected-file diff; isolated local stage and commit with one-use confirmation | Remote operations, branch mutation/rebase/merge, PR review/ship |
| Debug | Start session; refresh threads; continue; step over; pause; breakpoint; run tests; results | Debug surface and launcher reachable | DAP-backed actions; no project adapter was configured |
| More | Start suite; experiments; kernels; workspace state; harness catalog; doctor; private cockpit start/status | More surface/action catalog reachable | Kernel execution, experiments, cockpit, and harness launch |

The Agent slash modal, command palette, `:actions` launcher, help overlay,
keyboard navigation, desktop/compact/phone layouts, and Ctrl-C quit confirmation
were exercised. The quit prompt was dismissed with Esc and later confirmed with
Enter. Core composer entry and a fake-runtime turn were exercised. Code editor
bracketed paste was verified by a focused unit test; the PTY editor test verifies
ordinary typed input and save.

Keyboard and pointer paths were checked against the full key table in the
[current TUI guide](../../architecture/development-tui.md):

| Input family | Observed in PTY/tests | Inventory-only or not run |
|---|---|---|
| Surface selection | Number keys, Tab, and arrow navigation; desktop, compact, and phone workbench reachability | Mnemonic `c/v/w/g/d/m` aliases were not each sent individually |
| Command navigation | `:`, `a`, typed route filtering, `Esc`, help `?`, palette selection and placeholder prefill | Mouse click/double-click/long-press routes are covered by unit tests, not manually clicked in the allocated PTY |
| Composer | Agent entry, offline Core turn, `/` command modal, `Ctrl-L`/mode routing, preserving drafts on close/confirmation | Live provider steer/abort, provider-backed agent slash commands, and real session switching |
| Editor | Open, Insert, typed text, Ctrl-S save, selection context handoff, lock-contention key/paste queue, bracketed paste route | Full Normal-mode motions, structural textobjects, FIM acceptance, LSP keys, soft-wrap and undo/redo were inspected in the guide/catalog; not all were manually replayed in PTY |
| App/browser | App surface, target-picker recovery display, visual toggle status, native startup, local URL navigation and observation | Browser Back/Forward/reload, target selection, click/type, screenshot, dialog response and live graphics |
| Terminal/Git | Local process start/log/stop; file diff; stage and commit in isolated Git | Process restart/attach; remote Git/GitHub; Git branch/rebase/merge actions |
| Tasks/Debug/More/Trust | Surface reachability and launcher rendering; Trust inspect/project trust; Debug and More workbenches reachable | Task DAG dispatch, DAP controls, kernels, experiments, cockpit, read-only Trust and trust-once lifecycle |

The full key catalog also includes prompt editing, browser dialog keys, Git
staging shortcuts, editor Normal/Select/Agent modes, FIM and pair-apply keys,
LSP navigation, and pointer hit regions. Those routes remain discoverable in
the current guide; this pass does not claim each one was physically pressed.

The code-derived TUI catalog contains 19 legacy Agent entries, 11 Core Agent
entries, 10 Code entries, 9 App entries, 6 Terminal entries, 4 Tasks entries,
11 Git entries, 8 Debug entries, 8 More entries, and 4 Trust entries. Entries
that appear in multiple surfaces are listed in each surface because they are
separate discoverable routes.

## CLI entry-point inventory

The source-built `glass --help` listed 87 actual subcommands. A smoke script ran
`glass <entry> --help` for all 87 entries and the two remaining usage/help rows;
all 89 invocations exited successfully. This verifies that each entry's help
route parses and renders. It does not claim that every CLI operation was
executed. The inventory was:

- Lifecycle and workspace: `update`, `install-chromium`, `certify`,
  `workspace`, `project`, `agent`, `harness`, `memory`, `surfaces`, `backend`,
  `daemon`, `replay`, `capabilities`, `doctor`, `mcp-config`, `profiles`,
  `knowledge`, `result`, `delete-profile`.
- Browser navigation and actions: `navigate`, `recover`, `click`, `preflight`,
  `click-at`, `click-expect-popup`, `double-click`, `hover`, `drag`, `type`,
  `key`, `key-down`, `key-up`, `shortcut`, `clear`, `check`, `uncheck`,
  `select`, `upload`, `screenshot`, `text`, `dom`, `observe`, `inspect-page`,
  `smoke-sites`, `find-target`, `act-and-verify`, `extract-structured`,
  `extract-web-ir`, `recover-run`, `scroll`, `wait`, `diagnostics`,
  `accept-dialog`, `dismiss-dialog`, `dismiss-consent`, `download`, `targets`,
  `archive-targets`, `new-target`, `select-target`, `close-target`, `frames`,
  `select-frame`, `evaluate`, `cookies`, `export-cookies`, `import-cookies`,
  `pdf`, `fill-form`, `batch`.
- Workflow and local session: `workflow`, `task`, `ir`, `workflow-resume`,
  `resolve-intent`, `execute-intent`, `verify`, `reconcile-refs`,
  `observe-delta`, `checkpoint`, `snapshot`, `clipboard-read`,
  `clipboard-write`, `browser`, `session`, `help`, `tui`.

The real browser check exercised TUI startup, local navigation, and semantic
observation against a local server only. The full CLI browser surface was not
replayed command-by-command; remote sites, downloads/uploads, consent handling,
cookies, clipboard, screenshots, and mutations were not attempted.

## Validation evidence

- Full serial PTY integration suite: 17 tests passed. Log:
  `/tmp/glass-pty-validation-suite-final.log`.
- Full `glass-dev` library unit suite: 561 tests passed. Log:
  `/tmp/glass-pty-dev-lib-tests-final.log`.
- The final Pi-modal close status copy was generalized to “Agent commands” and
  its focused draft-restoration test passed after that edit.
- Focused ordered editor key/paste contention test and full-screen bracketed
  paste routing test passed in the `glass-dev` library test binary before the
  final PTY run.
- Isolated Git stage/commit PTY test passed. Log:
  `/tmp/glass-pty-git-stage-commit.log`.
- CLI help smoke: 89/89 entries exited successfully.
- Strict `glass-dev` Clippy passed with all targets/features and
  `-D warnings`. Log: `/tmp/glass-pty-clippy-final.log`.
- `cargo fmt --all -- --check`, documentation coverage/depth/release-truth,
  TUI shortcut, version-sync, and `git diff --check` validations passed.
- The browser worker binary and `glass-dev` binary built from this checkout.
- The native browser started successfully with a clean `GLASS_CONFIG_HOME` and
  the source-built worker. The local server recorded `GET /`; the TUI completed
  the browser navigation/observation flow. After that response, the content
  worker remained busy until the TUI was closed through its normal quit prompt;
  the page's displayed title/text was not asserted. The original attempt used
  the machine's normal browser config and returned `invalid storage`; rerunning
  with an isolated config removed that failure. The existing profile was not
  inspected or modified.
- Live browser pixels remained disabled (`--tui-live off`), so Kitty/ANSI image
  rendering was not tested. No real provider credentials or DAP adapter were
  available in this isolated pass.

## Source of truth

The action inventory is defined in `crates/glass-dev/src/tui/command.rs` and
key ownership in `crates/glass-dev/src/tui/mod.rs` and
`crates/glass-dev/src/tui/state.rs`. The PTY scenarios are in
`crates/glass-dev/tests/tui_pty.rs`; focused editor state tests remain beside
the implementation. Current user instructions are in `docs/INDEX.md`,
`docs/cli.md`, `docs/glass-core-harness.md`, and
`docs/architecture/development-tui.md`.
