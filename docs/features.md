# Complete feature reference

This reference maps shipped capability domains to their user-visible entry
points. “Available” describes source implementation in `0.3.3`; it is not a
cross-platform certification claim. The machine-readable target status is in
[feature-parity.json](feature-parity.json).

## Interfaces

| Domain | CLI | TUI | MCP | Rust | Guide |
|---|---|---|---|---|---|
| Install/update and browser launch/attach | `update`, global options, `doctor`, `install-chromium` | startup | session configuration | `BrowserSession`, `SessionOptions` | [Installation](installation.md) |
| Navigation and targets | `navigate`, `targets`, `new-target`, `select-target`, `close-target` | `navigate` | navigation and target tools | `BrowserSession` target/navigation methods | [CLI](cli.md) |
| Frames and topology | `frames`, `select-frame`, `verify` | current frame state | frame tools and predicates | session frame/topology APIs | [Actions](actions.md) |
| Structured observation | `observe`, `inspect-page`, `observe-delta` | semantic page pane | `observe`, `inspectPage`, `observeDelta` | `observe`, `semantic_observe` | [Semantic observation](semantic-observation.md) |
| Deep/visual evidence | `dom`, `screenshot`, `pdf`, `diagnostics` | explicit screenshot/live view | `getDOM`, `screenshot`, `printToPdf`, `diagnostics` | observation, visual, diagnostic APIs | [Feature details](#observation-and-evidence) |
| Pointer/keyboard/forms | click/type/key/form commands | common action commands | action tools | guarded session methods | [Actions](actions.md) |
| Wait and verification | `wait`, `verify`, `act-and-verify`, `preflight` | verified activity state | corresponding tools | wait/predicate/action APIs | [Action contract](action-contract.md) |
| Task Protocol and Web IR | `task`, `ir` | semantic execution results | task/Web IR tools | `task_protocol`, `task_compiler`, `web_ir` | [Semantic execution](semantic-execution.md) |
| Intent resolution | `resolve-intent`, `execute-intent`, `find-target` | intent commands | intent tools | session intent module | [Intent resolution](intent-resolution.md) |
| Workflows | `workflow`, `workflow-resume`, `checkpoint` | workflow command | `workflow`, checkpoint tools | workflow types and methods | [Workflows](workflows.md) |
| Knowledge/memory | `knowledge`, `memory` | knowledge summary | knowledge/memory tools | knowledge store/retrieval APIs | [Knowledge](knowledge.md) |
| Profiles/storage | `profiles`, cookies commands | profile status | cookie/storage tools | profile/storage APIs | [Profiles](profile-ergonomics.md) |
| Emulation | viewport option; browser API for network/CPU/UA/geo/timezone | status only | emulation tools | session emulation APIs | [CLI/MCP](cli.md) |
| Downloads/uploads/clipboard | dedicated commands | common clipboard keys only | dedicated tools | session APIs | [CLI](cli.md) |
| Policy/security | global policy options | effective policy status | startup policy and typed denials | `BrowserPolicy` and capability types | [Policy](policy.md) |
| Workspaces/daemon | `workspace`, `daemon` | daemon status | workspace and lease tools | `workspace`, `daemon` modules | [Daemon](daemon.md) |
| Development Runtime | `project`, `agent` | Development workspace | `project.*`, `agent.*` | `development` module | [Development Runtime](development-runtime.md) |
| Terminal/remote browser view | TUI live and Remote View commands | semantic-first Browser view; Herdr/Kitty/ANSI or tokenized loopback Safari | metadata only; no MCP pixel stream | `connection`, `presentation`, `terminal_graphics`, `development::remote_view` | [Mobile/remote](mobile-remote.md) |
| Backends/surfaces/replay | `backend`, `surfaces`, `replay` | bounded status | corresponding tools | backend/surface/replay contracts | [Architecture](architecture/README.md) |
| Extensions | `--experimental-extensions`, capabilities | negotiated status | negotiated experimental status | `extensions` module | [Extensions](extensions.md) |
| Reliability/certification | `certify`, `smoke-sites` | compact status | scenario operations through core tools | reliability modules | [Reliability](reliability.md) |
| TypeScript/Python clients | repository smoke commands | — | clients wrap MCP | — | [Client SDKs](#typescript-and-python-clients) |

## Browser lifecycle and ownership

- Owned mode launches Chrome/Chromium, owns its process, selects exactly one
  page target, and closes Chrome explicitly on `BrowserSession::close`.
- Attach mode connects to an existing CDP endpoint. It does not own Chrome,
  its profile, launch flags, or shutdown.
- Incognito uses a disposable profile. Named profiles retain browser-managed
  state and are protected by an exclusive profile lock.
- An occupied launch port, ambiguous target, incompatible attach option, or
  foreign healthy endpoint fails closed.
- Chrome sandboxing remains enabled unless
  `GLASS_DISABLE_CHROME_SANDBOX=1` is set exactly.

## Observation and evidence

The default is structured-first:

```console
glass observe --level summary
glass observe --level interactive
glass observe --level structured --region REGION_ID
```

`summary`, `interactive`, `structured`, `detailed`, and `raw` progressively
increase semantic detail. Expansion is region-scoped and revision-bound.
Compact observation and semantic observation remain distinct contracts.

Explicit evidence operations are:

- `dom`/`getDOM` for the full DOM;
- `screenshot` for viewport, clip, element, or full-page PNG/JPEG/WebP evidence;
- `pdf`/`printToPdf` for printable page bytes;
- `diagnostics` for bounded console/network evidence;
- `observe --form-values` when policy allows form-value reads; and
- structured extraction with typed fields, item/byte limits, provenance, and
  revision-bound continuation.

Screenshots and live frames are never silently substituted for semantic
evidence. Live terminal pixels are ephemeral and latest-frame-only.

## Interaction and verification

Glass supports unique-target click, double-click, hover, drag, type, clear,
check, uncheck, select, key/down/up, shortcuts, bounded form fill, upload,
coordinate click, scroll, dialog accept/dismiss, recognized consent dismissal,
and popup-expecting click.

`--interaction human` sends bounded smooth pointer movement; `fast` sends
direct pointer events. Both modes produce browser-side evidence and preserve
the same targeting and revision contract.

Use `preflight` for read-only target resolution and clickability. Use
`act-and-verify` or verification predicates for postconditions. A failed
preflight performs no pointer event, focus, scroll, or revision mutation.

## Semantic compiler

The semantic pipeline is:

```text
fresh browser evidence -> Glass Web IR v1 -> Task Protocol compiler
                       -> revision-bound live bindings -> guarded operation
                       -> postcondition receipt/recovery
```

Compilation is browser-free and deterministic. Authored values do not enter
plans or receipts. Live binding must find exactly one current semantic target
at the compiled revision. Historical knowledge is advisory and cannot select
an executable target or authorize mutation.

Supported task families cover forms, field reads, navigation, dialogs,
pagination, tables, collections, and regions. Read-only and mutating families
have different lease and confirmation requirements.

## Workflows, checkpoints, and replay

Workflows provide typed inputs, bounded steps, conditions, retries, outputs,
evidence, traces, and terminal proof. Authoring commands compile YAML/JSON,
format, validate, lint, preview, diff, initialize templates, and record
semantic events. Checkpoints are at most 4 KiB and contain no secrets.

Resume refuses definition mismatch, route drift, completed checkpoints, and
post-dispatch uncertainty. Redacted reliability replay bundles are scenario-
bound evidence; attaching one does not start or mutate a browser.

## Knowledge and advisory memory

Persistent knowledge is isolated by exact origin, profile, workspace, backend,
surface, and lifecycle evidence. Records may be fresh, stale, contradicted, or
quarantined. Retrieval is bounded and exact/graph-compatible; embeddings are
disabled unless explicitly injected.

The separate advisory memory commands inspect, explain, export, forget, prune,
and reindex memory records. Neither memory system stores live browser handles
or authorizes an action.

## Development Runtime

The project runtime provides canonical-root detection, bounded file listing
and reads, native buffers, atomic saves, undo/redo, fuzzy search, PTYs, process
health/output, real rust-analyzer diagnostics, source/runtime graph, semantic
breakpoints, actor-attributed timeline/events, replay, Git worktree
experiments, collaboration claims, Neovim probes, resident sessions, reconnect
capsules, attention inbox, and verification cards.

Project reads are handle-bound and capped. Mutations stay inside the canonical
root and record actor provenance. Prompt text and tool arguments are represented
in audit state by bounded metadata and digests, not raw values.

## Terminal and remote experience

Desktop, compact, and phone layouts share one reducer and browser worker.
Phone mode exposes Overview, Agent, Browser, Project, Diff, and Process without
requiring function keys. Semantic tap selects one of at most nine
revision-bound targets.

The optional live browser backend order is Herdr, direct Kitty, ANSI, then
semantic fallback. `live auto` requires a detected native backend; `live on`
allows ANSI. Safari over an SSH local port forward is the stable full-fidelity
iPhone path. Glass never opens CDP publicly.

## Backends and surfaces

CDP is the production backend. WebDriver BiDi is an experimental bounded
adapter. The proof backend is browser-free and only certifies protocol
conformance. Capability omission or incompatibility is a typed denial, never a
fallback to raw transport.

Surface contracts describe document, frame, shadow, SVG, canvas, media,
embedded, PDF, browser-native, remote-stream, terminal, and extension
boundaries with provenance and coverage. Detection alone does not authorize
interaction. Coordinate actions require strong geometry evidence.

## TypeScript and Python clients

Repository clients in `clients/typescript` and `clients/python` are thin MCP
clients, not browser runtimes. They negotiate capabilities, expose browser and
Development Runtime helpers, maintain bounded request state, support
cancellation, cursor-based project events, process-health waits, reconnect
workflows, and mutation-lease scopes. They are not published to npm or PyPI in
the `0.3.3` line.

Run their browser-free conformance smokes:

```console
npm --prefix clients/typescript run typecheck
GLASS_BINARY="$PWD/target/release/glass" node clients/typescript/smoke.mjs
GLASS_BINARY="$PWD/target/release/glass" python3 clients/python/smoke.py
```

## Limits and failure rules

Every input and retained output has a contract-specific bound. Unknown fields,
unknown variants, stale revisions, ambiguous targets, missing capabilities,
unsafe paths, oversized frames, and unsupported platform gates fail explicitly.
Glass does not silently retry a possibly applied mutation, downgrade evidence,
capture a screenshot, select a different target, or fall back from an explicit
backend preference.
