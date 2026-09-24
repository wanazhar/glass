# Glass Core Web Profile

Profile: `GCWP-0.1`

Status: implementation baseline for issue #40. This profile is the M0
contract; it is not a conformance result or a production certification record
for the current native backend.

Authority: [issue #40](https://github.com/wanazhar/glass/issues/40), with the
implementation and ownership contract in
[`docs/architecture/native-engine.md`](../architecture/native-engine.md).

## Purpose

`GCWP-0.1` defines the smallest profile that can honestly replace Chromium/CDP
for ordinary Glass browser work. “Complete” means complete for this versioned
profile, not complete for every obsolete plugin, proprietary vendor service,
unstable draft API, or browser-specific extension.

The profile is deliberately external-web capable. A native implementation that
only loads `about:blank`, `data:` URLs, or registered fixtures does not satisfy
this contract.

## Non-negotiable contract

When the native backend is selected, it must:

1. navigate ordinary external HTTP(S) pages without launching Chromium,
   connecting to CDP, using a remote browser, or silently switching backend;
2. expose the declared web-platform behavior through one Glass-owned lifecycle,
   origin, document, script, style, layout, render, input, storage, and
   accessibility model;
3. implement every normal Glass operation mapped to this profile, including
   CLI, `BrowserSession`, MCP, and TUI paths;
4. return typed, versioned unsupported errors only for capabilities explicitly
   outside this profile; and
5. run untrusted content behind the production process/sandbox boundary before
   promotion. The current in-process engine is a development implementation,
   not a security boundary.

CDP remains an explicitly selectable migration backend until native promotion.
It is never an implicit fallback for a native request.

## Capability matrix

| ID | Required ownership | Included behavior | Promotion evidence |
| --- | --- | --- | --- |
| `runtime` | browser and content runtime | process lifecycle, IPC, cancellation, event loop, task/microtask ordering, worker lifecycle, quotas, crash recovery, deterministic traces | lifecycle, cancellation, hostile-content, timeout, and restart tests on every supported OS |
| `network-origin` | network and security layer | URL/encoding, DNS, HTTP(S), TLS policy, redirects, MIME/charset, cache, cookies, origins/sites, CORS, CSP, mixed content, referrer, service workers, WebSocket/EventSource where selected, permissions | mandatory security/URL tests, selected Fetch WPT, differential request traces, no cross-origin leaks |
| `html-dom` | document platform | standards HTML tokenization/tree construction and error recovery, including SVG/MathML attribute-name adjustment and XLink/XML/XMLNS attribute namespace identity; document lifecycle, DOM mutation/selection/ranges, events, forms, focus, shadow DOM, custom elements, frames, and document policies | HTML parser/DOM/event WPT, malformed-document corpus, frame/AX fixtures |
| `javascript-webidl` | script and binding layer | ECMAScript realms, Web IDL bindings, promises, timers, modules, structured clone, workers, fetch/XHR, required DOM APIs, exception propagation, microtask ordering | script/evaluate parity, async-ordering, module, worker, and binding tests |
| `css-layout` | style and geometry layer | tokenizer/parser, cascade, selectors, inheritance, custom properties, media/container queries, block/inline/flex/grid/table/positioned layout, overflow/scrolling, writing modes/bidi, animation/transition, and selected fragmentation | selected CSS/layout WPT, geometry differential corpus, scroll/hit-test invariants |
| `rendering` | paint and compositor | fonts/shaping/rasterization, images/SVG/canvas, media resource lifecycle, paint, clipping, transforms, filters, layers, compositing, hit testing, software/headless/GPU surfaces, screenshots, and print where selected | deterministic visual/print corpus, pixel/geometry diffs, capture repeatability |
| `contexts-input` | browser primitives | tabs/windows, browsing contexts, frames/popups/opener relationships, history/session state, keyboard/pointer/touch/IME, selection, drag/drop, clipboard, file chooser, upload/download, dialogs, prompts, permissions | native-only end-to-end Glass workflows and recovery tests |
| `storage` | durable web state | profile isolation, cookies, cache, session/local storage, IndexedDB-class storage selected by the profile, service-worker state, downloads, and deletion/eviction policy | restart/persistence/isolation/cleanup tests with no cross-profile leakage |
| `accessibility` | semantic and assistive surface | roles, states, properties, name/description computation, focus, actions, and incremental updates for the declared DOM/layout surface | accessibility-tree differential fixtures and action/focus tests |
| `glass-integration` | public Glass contract | stable backend capability profile, navigation, targets, DOM/AX/evidence, actions, key input, script/evaluate, waits/events, screenshots, contexts, storage, downloads/uploads, prompts, CLI, MCP, and TUI parity | all normal operations pass in native-only mode with no hidden CDP process/socket |

## Explicit exclusions

The following are outside `GCWP-0.1` unless a later profile revision adds them:

- Flash, NPAPI/PPAPI, obsolete plugins, browser extensions, and vendor-specific
  account services;
- proprietary DRM/Widevine and codecs whose licensing or platform contract is
  not available to Glass;
- unstable draft APIs and browser-internal privileged pages;
- OS browser chrome that belongs to Glass rather than the page engine;
- undocumented compatibility quirks that cannot be represented in a stable
  test or security contract.

An exclusion is not permission to misrender or silently skip a required
dependency. If an excluded feature is encountered, the engine must produce a
typed diagnostic, preserve origin/security invariants, and keep the rest of
the session recoverable.

## Supported platform matrix

The promotion matrix is:

| Platform | Required release target | Required evidence |
| --- | --- | --- |
| Linux | x86_64 GNU/Linux, headless software path | native-only conformance, security, package, and binary gates |
| macOS | x86_64 and arm64 where the existing release matrix supports them | native-only conformance, capture, package, and binary gates |
| Windows | x86_64 MSVC, headless/software path first | native-only conformance, process isolation, package, and binary gates |

GPU acceleration is an optimization, not a prerequisite for the first
promotion. The software/headless path must be complete and deterministic;
GPU paths must not change security or semantic behavior.

## Conformance and acceptance thresholds

The pinned WPT manifest and its revision are a later checked-in artifact under
the BE-09 conformance task. It must contain stable test IDs, platform results,
and exclusion reasons; an aggregate pass percentage alone is insufficient.

Promotion requires all of the following:

- 100% of mandatory URL, origin, security, HTML-parser, and DOM-invariant
  tests;
- at least 98% in every selected stable profile suite, with zero unexplained
  failures and no skipped test counted as a pass;
- a versioned synthetic/real-site corpus covering navigation, redirects,
  script/modules, asynchronous waits, DOM/AX, actions, frames/popups,
  storage, upload/download, dialogs, permissions, screenshots, and recovery;
- differential traces and visual/geometry results against at least one
  reference browser, with every difference classified as expected profile
  behavior, an approved exclusion, infrastructure, or a defect;
- reproducible native-only results on Linux, Windows, and macOS from clean
  release artifacts; and
- security, memory, CPU, startup, navigation, cancellation, and crash-recovery
  budgets recorded before the final promotion run.

The thresholds are fixed for `GCWP-0.1`. A failing result is fixed, explicitly
excluded through a profile revision, or a release blocker; it is never hidden
by changing the threshold after the run.

## Performance and resource budgets

BE-00 establishes measurement rules; BE-09 records hardware-specific baselines
and promotion results. Every benchmark reports cold and warm native startup,
time to first DOM, time to interactive for the corpus, screenshot latency,
steady-state RSS, peak RSS, CPU time, cancellation latency, and target growth.

The first promotion target is:

- no unbounded memory growth across a repeated corpus run;
- cancellation and close complete within the declared operation timeout and
  leave no child process or socket behind;
- native warm navigation and capture remain within 2x the Chromium reference
  for the same synthetic corpus, unless a profile record explains the cost;
- native steady-state RSS remains within 1.5x the reference for the same
  scenario, with content-process and browser-process usage reported
  separately; and
- build configuration remains stable, with no routine `cargo clean` or
  target-wide rebuild used to obtain a result.

These are promotion budgets, not current implementation claims.

## Security boundary

Before promotion, page content is treated as hostile. The design must provide
an out-of-process content boundary, OS-appropriate sandboxing, origin/site
isolation, permission mediation, bounded IPC, resource quotas, crash
containment, and redacted diagnostics. Network credentials, cookies, page
secrets, evaluated source, and downloaded content must not enter ordinary logs.

The in-process local engine is retained for development and deterministic unit
tests only. It cannot satisfy `runtime` or `network-origin` production gates by
itself.

## Revision policy

`GCWP-0.1` is immutable once the first conformance run begins. A profile
revision must:

1. add a new profile identifier;
2. record added, removed, or reclassified capabilities and exclusions;
3. update the WPT manifest and real-site corpus;
4. preserve old conformance evidence as historical; and
5. update issue #40, the architecture doc, the task records, and user-facing
   capability documentation in one change.

No implementation slice may claim browser completeness by changing this
profile retroactively.
