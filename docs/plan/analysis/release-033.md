# Glass v0.3.3 issue #33 delivery analysis

Status: Complete local release candidate

Issue [#33](https://github.com/wanazhar/glass/issues/33) and its
authoritative packaging/browser-recovery amendment define the release
contract. This document is the implementation and review boundary. A checkbox
is complete only when the named code path and evidence exist; prose or a type
without an integrated consumer is not evidence.

## Product correction

```text
                         one Glass workspace
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
       development state     browser authority     Web IR / memory
              │                   │                   │
              └───────────────────┼───────────────────┘
                                  │
                    connection + presentation policy
                                  │
             ┌────────────────────┼────────────────────┐
             │                    │                    │
        local terminal       SSH / Mosh TUI      loopback Remote View
          30–60 FPS          semantic-first       same browser target
```

Layout, transport, graphics capability, and presentation policy are separate
typed decisions. The same `BrowserSession`/workspace identity feeds terminal
presentation, Remote View, agent context, workflows, and fresh Web IR.

## Baseline defects reproduced from 0.3.2

| Defect | Current evidence | Required correction |
|---|---|---|
| Local presentation ceiling | `TuiLiveQuality::Smooth` maps to 12 FPS | local smooth 60, balanced 30, explicit idle/background throttling |
| Width used as remote proxy | `display_class` extends the phone breakpoint for SSH/Mosh | layout depends on geometry/override; transport policy is independent |
| Coarse connection model | `RemoteContext` stores only SSH/Mosh/Herdr booleans | typed layout, transport, graphics, shell/multiplexer, measurements, reasons |
| Fatal browser startup | the browser worker returns after `StartupFailed` | long-lived controller with recovery commands and semantic-only state |
| No target picker | attach requires launch-time `--target-id` | bounded in-TUI target discovery and privacy-aware selection |
| Full install misses core command | `glass-dev` contains only `glass` | add a thin `glass-browser` binary in the superset package |
| Immediate-child process ownership | PTY child `kill()` is the only stop contract | owned job/session tree, graceful/hard stop, cleanup evidence |
| Poll errors hidden | `ProcessManager::list` discards `poll()` errors | typed degraded state/result |
| Tree completeness ambiguous | `list_files` returns only a `Vec<FileEntry>` | explicit limit, truncation and skipped/ignored summary; cached snapshot |
| Per-request LSP path | TUI opens a workspace/client for diagnostics work | persistent workspace service manager and document revisions |
| Neovim proof is not embedded RPC | headless Lua command is labeled `rpc_prototype` | `nvim --embed`, Msgpack-RPC request, buffer/edit/state round trip |
| Invalid design evidence | pinned iOS JPEG cannot be decoded | replace with a valid asset and validate both assets in release checks |

## Module decomposition

| Delivery | Owner | Inputs | Outputs | Integration proof |
|---|---|---|---|---|
| Connection model and policy | `glass-browser::presentation` | terminal geometry, explicit config, environment/probes, renderer feedback | `ConnectionEnvironment`, `PresentationPolicy`, reasons and budgets | deterministic matrix fixtures and TUI diagnostics |
| Browser controller | TUI workspace | desired browser config and user commands | explicit state, endpoint classification, target list, live session generation | occupied port, endpoint attach, disconnect/reconnect scenarios |
| Phone/compact/wide shell | TUI reducer/renderer | display class plus shared workspace state | responsive cards, views, overlays, command palette | buffer snapshots at constrained sizes and keyboard reducer tests |
| Remote View v1 | browser worker | the authoritative session and revision | loopback HTTP/WS viewer, scoped token, latest-frame/input channel | same target/revision, revocation, stale-input refusal |
| Agent live context | development agent gateway + TUI | optional current session/workflow/memory, lease and actor | dynamic tools and live context references | detached/attached/recovering/reconciled tests |
| Runtime reliability | development runtime | project root, jobs, files and editor buffers | tree lifecycle, bounded tree result, persistent language services | descendant cleanup, ignore/truncation, revisioned LSP tests |
| Neovim engine proof | development runtime | `nvim --embed` | RPC channel, buffer state, edit/redraw evidence and decision | real installed-tool CI smoke |
| Distribution/release | Cargo + CI + docs | synchronized 0.3.3 sources | two crates; `glass-dev` installs both commands | clean `CARGO_HOME` transition matrix |

## Integration chains

Every chain is tested with real implementations at its boundary.

```text
terminal probe/config
  -> ConnectionEnvironment
  -> PresentationPolicy
  -> capture scheduler + diagnostics row

TUI command palette/recovery sheet
  -> BrowserConnectionController
  -> endpoint probe / launch / attach / target select
  -> BrowserSession generation
  -> fresh observation + Web IR revision
  -> agent capability refresh

BrowserSession generation
  -> terminal latest-frame mailbox
  -> Remote View latest-frame broadcast
  -> revision-bound pointer/keyboard input

ProjectWorkspace
  -> ProcessManager job ownership
  -> tree snapshot cache
  -> LanguageServiceManager
  -> agent/process/diagnostic cards
```

## TUI inspiration decisions

The design borrows interaction patterns, not product appearance:

- Claude Code: one persistent composer, `/` command discovery, `?` help,
  explicit background-task state, redraw recovery, and shortcuts that document
  tmux/terminal limitations.
- Codex CLI: filtered command popup, configurable status line, task-safe
  command availability, background terminal controls, and copy-friendly raw
  output.
- Lazygit: numbered focus targets, context-sensitive help, consistent
  `Enter`/`Esc` dialog behavior, search/filter, and expandable main panels.
- Ratatui/Glass constraint: every essential action also has a basic printable
  key or command-palette route; function keys, mouse events, and rich terminal
  protocols are enhancements.

## Delivery order and local commits

1. `feat(presentation): model connection-aware display policy`
2. `feat(tui): add remote-first workspace and browser recovery`
3. `feat(remote-view): stream the authoritative browser session`
4. `fix(runtime): harden jobs trees and language services`
5. `feat(packaging): expose browser command from full install`
6. `docs(release): prepare Glass 0.3.3`
7. final fixes found by release review

## Pillar evidence matrix

| Pillar | Required evidence |
|---:|---|
| I | typed connection dimensions, conservative unknowns, overrides and diagnostics |
| II | deterministic environment/activity-to-policy matrix with reasons |
| III | 60/30 local targets, separate scale/FPS, idle/background and latest-state metrics |
| IV | stacked phone cards, six reachable views, attention/recovery sheets and snapshots |
| V | semantic/project/agent/process updates independent from pixel delivery |
| VI | loopback/token/revocable Remote View, same target and stale-input rejection |
| VII | attachment-aware agent tools/references with leases and recovery invalidation |
| VIII | one-way package dependency and development feature/package ownership proof |
| IX | job/tree lifecycle, interrupts, bounded wait, cleanup and explicit poll failure |
| X | tree result metadata, ignore policy, cache/invalidation and conflict-safe saves |
| XI | bounded syntax strategy and persistent revision-aware LSP lifecycle/operations |
| XII | real `nvim --embed` Msgpack-RPC buffer/edit/state exchange and recorded decision |
| XIII | Linux/macOS/Windows browser-free CI, tool jobs and remote-policy fixtures |
| XIV | requested/acquired/presented FPS, age, drops, bytes, latency, scale and reasons |
| XV | clean installed-product tests, historical release language, notes and limitations |

## Mandatory release gates

The automated release validator must account for all 53 issue checkboxes.

| Gate | Count | Evidence owner |
|---:|---:|---|
| 1. Local browser performance | 5 | presentation policy tests and benchmark |
| 2. Remote policy correctness | 5 | connection matrix fixtures |
| 3. Mobile UX | 5 | Ratatui snapshots, design-asset validation, interaction tests |
| 4. Same browser reality | 4 | session identity/revision integration tests |
| 5. Agent/browser integration | 5 | dynamic gateway and lease tests |
| 6. Product boundary | 5 | dependency/package feature inspection |
| 7. Development reliability | 5 | process/tree/editor/LSP tests |
| 8. Cross-platform/release truth | 5 | CI matrix, install smoke, docs checks |
| 9. Full-suite install completeness | 6 | clean `CARGO_HOME` transition matrix |
| 10. Zero-exit browser recovery | 8 | controller/TUI scenario tests |

## Integrated release demonstrations

Scenarios A–K are release artifacts, not aspirational examples. Browser-free
fixtures cover B–E and controller state transitions. Linux Chromium evidence
covers A, I–K where the environment permits. F and G install/use their real
tools in CI. H executes against packaged crates from a clean Cargo home.

## Claim boundary

0.3.3 may claim source support where browser-free CI runs, and native browser
behavior only on environments with recorded browser evidence. Mosh/SSH policy
tests prove deterministic selection, not real cellular throughput. Remote View
v1 is a bounded loopback frame/input channel, not WebRTC or a public relay.

## Result

Completed locally on 2026-08-08. All 53 issue gates map to integrated code and
tests in [the gate review](../reviews/release-033-gates.md). The final checkout
passes 783 library tests with one intentional tool-availability ignore, every
workspace integration target, 19 serial Chromium scenarios, the clean
core/full ownership-transition matrix, strict lint/rustdoc, minimal-feature
compilation, package verification and publication dry runs. No remote state was
changed.
