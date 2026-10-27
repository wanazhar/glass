# Glass workspace implementation matrix

This page tracks the workspace work discussed in [issue #64](https://github.com/wanazhar/glass/issues/64). It records the
current code and verification evidence in this working tree; an implementation
entry is not considered complete until its listed end-to-end check passes.

**Matrix version:** Glass and Glass Core 0.3.14; evidence recorded through
2026-10-26. This file is the canonical repository-versioned scope and evidence
matrix for issue #64.

## Ownership contract

Glass is the project workspace and host integration layer. Glass Core owns
agent execution, permissions, provider/session state, and turn lifecycle.
Glass routes human input and Core-authorized host tool calls, then presents the
resulting Core events and workspace resources. Browser revisions, project
identity, and file/resource conflicts remain host validation concerns; Glass
does not add a second agent approval policy.

## Workspace coverage

| Area | Owner(s) | Acceptance / evidence | Remaining scope / limits |
|---|---|---|---|
| Agent and navigation | Glass Dev TUI; Glass Core runtime for agent execution and turn lifecycle | Composer-ready startup; Agent, Code, Browser, Terminal, and Git surfaces; Alt+1…Alt+5 and Ctrl+Tab preserve literal typed digits. Advanced Tasks, Debug, and service actions are reachable through the unified slash/action palette. Fresh-start, first-send, literal typing, surface switching, Code preview/edit, and Core command handoff passed through the rebuilt launcher PTY. | No additional first-send or keyboard navigation gap is recorded in this matrix. |
| Core commands | Glass Dev CLI/TUI for forwarding and presentation; monorepo `glass-core` for command execution | Unambiguous `glass <core-command>` names and the full `glass core ...` namespace forward to the monorepo executable. CLI argument/stdin/stdout/exit pass-through and help have integration coverage. Top-level failures use the shared width-aware Glass error report on terminals and a concise `Error: …` line on piped stderr; a real `glass daemon status` regression verifies the missing-daemon recovery instruction, exit status 1, and empty stdout. Core human reports retain the shared Glass TUI heading and report style; Browser human start/doctor/profile/install/error output uses the same width-aware renderer. Clap help wraps at terminal width; `workspace`, `agent`, `project`, `harness`, `memory`, `surfaces`, `backend`, `daemon`, `logs`, and `mcp-config` help omit inherited Browser flags without removing their parser compatibility, while browser command help retains those flags. Rebuilt Browser doctor/profile/error and Browser/Glass help PTYs were reviewed at 58, 80, and 120 columns. Piped profile output and JSON doctor output remain on their existing paths. Rebuilt-launcher PTY passed fresh `/`, `/status`, and `/model`; model selection uses the resident session. `/` groups the live Core catalogue and Glass workspace actions; all built-in Core commands declare a presentation disposition; unsupported interactive controls open a Glass-owned notice panel and run no action. Standalone `gc help tools` and `gc help commands` now name built-in `web_search`/`web_fetch`, distinguish the separate native `glass-browser` executable, and state that `glass.browser.*` requires a host-advertised capability. Actual 58×24, 80×24, and 120×36 terminal renders keep the complete tools page visible; `gc model --help` and `gc models --help` instead give the interactive-only `/model` guidance without a TTY. The focused help and non-TTY CLI integration tests pass. The complete destination policy and versioned result contract are documented in [Core command presentation](docs/architecture/core-action-presentation.md). Resolver order is explicit binary, `GLASS_CORE_BINARY`, matching Core adjacent to Glass, then PATH; each selected candidate must report the workspace package version and still pass runtime protocol negotiation. A PTY harness smoke exposed that `GLASS_CORE_BINARY` was being shadowed by an adjacent binary; precedence is now covered explicitly. Direct `target/debug/glass agent doctor` selected monorepo Core 0.3.14 at `target/debug/glass-core`, protocol v2, mixed mode. | The independent GCN product and a standalone `gc` binary are outside this route; the Glass bridge remains pointed at the monorepo executable. |
| Local tool diagnostics | Glass Core records; Glass CLI presents | Failed provider tool calls write bounded, timestamped, workspace-local JSONL entries with static error classes, sanitized tool/backend names, hashed session/call correlation, and exact provider wire/route names plus schema fingerprints. `glass logs --root PATH [--json] [--catalog]` reads the current and one rotated file. Unix logs are mode 0600; each file is capped at 512 KiB. Focused tests prove redaction of invalid raw arguments and provider schema capture. | This is an actionable failure journal, not a session transcript or successful-call audit; raw errors, prompts, arguments, URLs, page data, and credentials are deliberately omitted. |
| Conversation | Glass Dev TUI presentation; Glass Core event lifecycle | Human, Core, lifecycle, error, and tool activity have distinct compact labels; tool rows identify Core or Glass origin and expandable output. Ratatui frames verify the role hierarchy at 58×24, 80×24, and 120×36. An isolated fake-Core PTY verifies prompt, assistant result, and attributed tool card stay in the conversation without a catalogue/system dump. Rebuilt-launcher fake-Core PTY captures show the conversation at 58×24, 80×24, and 120×36 (`target/visual-review-20261026/agent-transcript-*.png`); matching ANSI/text captures are retained beside them. A visual review found the narrow transcript footer wrapped; the <=71-column footer now uses a concise, single-line hint and has a width regression. The 80-column frame remains legible. An isolated standalone Core first-run frame at 80×24 provides a credential-free product comparison without a prompt. All resident Core command results and failures use typed owner-scoped panel data and stay out of conversation text; actual assistant turns and attributed tool results remain visible. Projection/state coverage checks catalogue, status, compact result, model picker, Core settings panel, history/todo unsupported cases, error placement, and late-result no-reopen behavior. The refreshed three-size PTY checks `/settings` through Core's settings/model RPCs; the exact `commands/execute` sequence remains `help`, `status`, and deliberate `error`, and the test asserts zero `turn/prompt` calls plus no help/error/unsupported text in the composer frame after closing panels. ANSI streams are saved under `/tmp/glass-command-panels-settings-20261026`. | The standalone Core comparison is its credential-free first-run screen, not a provider-backed transcript. The retained agent-transcript PNGs predate the footer refinement; updated transcript captures still need to be generated. PTY coverage asserts no agent-turn prompt from palette commands but does not inspect an internal transcript-entry count. |
| Code | Glass Dev editor and workspace | Selecting a file refreshes preview from the selected buffer. Syntax highlighting/editor diagnostics are used; asking from Code attaches visible, removable, bounded context from multiple unsaved buffers and selected ranges, while ordinary Agent messages receive no implicit file context. A rebuilt-launcher PTY verifies the exact attached context reaches the isolated fake Core; state coverage checks multiple selected ranges and selected-only content. `lsp format SERVER PATH` and `lsp range-format SERVER` send unsaved buffer contents; range formatting sends the selected range. Both translate UTF-16 edits against the captured text into exact-base diff proposals, with no mutation before explicit acceptance. `lsp rename` sends all open unsaved buffers (128 documents / 16 MiB limit), then routes a stdio LSP `WorkspaceEdit` through project path validation into per-file proposals; deterministic fixtures verify unsaved source and call-site delivery, range-format coordinates, outside-project rejection, and lock-contention retry without losing the format baseline or multi-file edit. Workspace previews now focus the first edited buffer and open the review editor, so acceptance and save target the proposal's actual path. The language client answers rust-analyzer workspace configuration/folder requests, keeps only one bounded references readiness request in flight, and retries a transient no-reference response only when references confirm that the selected symbol is indexed. A delayed-reference fixture reproduces why concurrent short-timeout probes starved indexing; a separate deterministic fixture covers LSP `ContentModified` retry. The rebuilt-launcher fake-server PTY verifies two-file preview, separate Enter acceptance, and saved output for each file. An opt-in PTY, executed with installed Rust Analyzer 1.97, verified its real cross-file WorkspaceEdit through separate per-file acceptance and on-disk saves. | LSP workspace resource operations (create/rename/delete) are explicitly unsupported. Multi-file proposals are reviewed and accepted independently, not as an atomic transaction; accepting one file while rejecting another can leave a partial rename. Formatting and language-service diagnostics still depend on a configured server. |
| Git | Glass Dev workspace and Git backend | Branch/upstream and staged, unstaged, untracked, and conflict counts are shown. Existing actions provide status, diff, stage/unstage, commit, and history. Disposable-repository PTY covers stage, unstage, restage, commit, and history. | No Git mutation was run against this checkout; the listed PTY evidence is from a disposable repository. |
| Browser and remote viewing | Glass Dev Browser surface; `glass-browser` / `glass-browser-core` runtime | The workspace exposes connection/backend state and actionable empty-state/navigation hints. SSH forwarding guidance is restricted to loopback URLs and explicit host placeholders. A local fixture through the Core host router verified native start, navigation, observe/action, typed CSS-limit failure, recovery, and backend provenance. The versioned native media corpus includes image decode/paint, bounded WebM/VP8 playback, and an authored 250 ms PCM WAV journey. WAV decodes to non-silent PCM; Remote View exposes a token-scoped playable WAV artifact with native provenance and user-controlled controls, without autoplay or a host sound device. Focused decode and all ten Remote View tests pass. A rebuilt native browser and worker navigated read-only to Amazon at revision 2 without Chromium fallback. The native GitHub corpus journey passed observed navigation into the Rust `library` directory (6/6 steps, 50.62 s); the deep Svelte reactivity docs journey passed navigation, semantic title assertion, accepted viewport scrolling, and post-scroll observation (6/6, 80.21 s). A fresh configured-provider PTY run successfully called `glass.browser.state`, `web_fetch`, and `web_search` (3 calls, 0 transport retries). A second run completed native incognito start → navigate to `https://example.com` → semantic observe (title `Example Domain`, revision 2) → stop; no Recovery modal appeared after the intentional stop. `stoppedByRequest` distinguishes an explicit stop from an unexpected disconnect; focused snapshot regressions retain crash recovery for the latter. | The Amazon JS bridge initialization measured about 9.4 seconds on that page. Provider smoke covered one site and one run, not universal tool availability or broad public-site parity. The versioned public MDN CC0 WebM journey checks decoded media metadata, playback-clock progression, pause, and a screenshot step. Its first run with the standalone glass-browser binary passed navigation and observation, then the video resource predicate errored after 43 ms and the content worker closed before playback began; no public video pass is claimed. The earlier stack overflow came from the duplicate-name Glass Dev browser wrapper and is not native-site evidence. The public report and private trace are `/tmp/glass-native-mdn-video-product-binary-20261026.json` and its adjacent `.artifacts/` directory; the Glass Dev wrapper now uses the bounded 16 MiB main-thread stack. |
| Tasks and Debug | Glass Dev surfaces and DAP host; Glass Core owns durable task state and `taskGraph` | Tasks is contextual, not a primary tab. It shows selected Core conversation lifecycle and the selected Core's durable `taskGraph`, separately from Glass task flows. Host tool descriptions distinguish Glass-managed processes/configured DAP sessions from Core one-shot native tools. Offline Core protocol and PTY coverage verify graph request/rendering without exposing prompts/checkpoints. | Debug empty state identifies DAP configuration as a prerequisite. Actual debug execution still depends on configured adapters. |
| Terminal and processes | Glass Dev process service and workspace actions; Glass Core owns its one-shot bash tool | Project-managed named processes are distinct from Core one-shot bash. Tool descriptions cover list/log/port/health/start/stop/input. A disposable PTY starts a named process through the slash-discovered Glass action, confirms the human-initiated start, observes its exit, and retrieves output through the slash-discovered logs action. | No environment secrets are printed in the recorded journey. |
| Advanced surfaces | Glass Dev TUI | Five primary destinations replace the prior eight-tab bar. Tasks, Debug, and settings/services remain contextual actions; persisted state is retained. Visual review at 120×36 also found the Changes title clipping `INDEX/WORKTREE` in its narrow sidebar; panels under 40 columns now show the available concise `CHANGES · N` title, with a regression against clipped text. | Phone-sized PTY discoverability review and confirmation that no primary destination is dead remain open. |
| Native browser engine | `glass-browser-core` native engine; `glass-browser` CLI and worker | The CSS rule ceiling matches the 4,096 protocol budget. A rightmost id/class/tag index skips unrelated common selectors while preserving cascade order; unsafe-index selectors use full-match fallback. The 4,096-rule fixture passed: parse+index 256.85 ms and computed styles for 132 nodes 62.45 ms (11.27 s total harness). The rebuilt worker accepts 513 rules, returns a typed error at 4,097 while retaining committed revision, then recovers on the next valid navigation. A native host-view fixture requires 100 element names across document/sectioning, text/list/table, forms, embedded media, interactive, SVG, MathML, and custom-element categories; it also checks selector traversal, form state, table rows, SVG attributes, MathML text, and template content. The versioned native journey corpus and runner are linked from [the native journey guide](docs/architecture/native-journey-corpus.md). Observations expose up to 64 revision-bound overflow targets, and `scrollTargetObserved` requires an explicit name/tag selector with a unique fresh-observation match; local tests cover a labeled generic nested container, actual movement, and bounded miss diagnostics. The Svelte deep-page run additionally verifies a 400-pixel viewport scroll and a fresh post-scroll observation. The versioned local journeys now pass observed multi-page navigation/history and nested scrolling, decoded image-link navigation with a captured source pixel, and a dynamic disclosure control plus typed GET search/result/back flow. A 2,000-anchor host-view regression and a dense 4,000-link fixture with id/href/class/title/rel attributes pass. A mixed 8,500-element host fixture (about 15.9k DOM nodes, including IMG/LINK/SCRIPT-compatible resource attributes) passed repeated retained-realm installs and changed-snapshot refresh under the unchanged 256 MiB QuickJS cap; the measured pre-GC high-water was 123.6 MiB. The expanded mixed fixture adds 20 nested wrappers and unique per-element style values (15,966 nodes total); it passes repeated retained-realm installs and changed-key refresh after allocator-pressure collections. In the local trace, pre-GC QuickJS heap use was 138.0 MiB and allocator use peaked at 226.3 MiB, with four collections; this is a bounded stress-fixture result, not a Wikipedia pass. Dynamic IMG/LINK/SCRIPT `src`/`href` reflection and script `referrerPolicy` are installed before native element prototype descriptors are copied; an isolated regression verifies URL getters and attributes, plus IMG completion/current-source state after assigning `src`. A mixed 3,000-element document with an approximately 32.7 MB retained realm also initializes under the unchanged 256 MiB QuickJS cap. One native MDN deep-reference journey passed in 47.1 s (42.9 s navigation, 2.6 s observation). The latest bounded Wikipedia article-to-observed-subpage run on 25 October remains an open product bug: navigation hit its 120 s deadline before observation. Lazy image-pixel decoding reduced image-resource host-view setup from 6.90 s to 0.0007 s; 12 installs completed, with the slowest at 8.03 s under the unchanged 15 s host-view limit. The run measured 27.8 s to compute styles for 16,053 elements, 20.5 s measuring text widths, 23.5 s building initial layout, and 60.8 s cumulative page-script execution (47.5 s in the slowest classic script). After this public run, a bounded per-layout text-width cache was added for exact short LTR text with default shaping options; a synthetic 64-span fixture reported 127 cache hits, 769 direct measurements, and 44.3 ms layout. A later bounded Wikipedia run with the cache enabled exceeded the 120 s outer runner deadline and returned inconclusive without step evidence. Its trace recorded 7,397 cache hits, 54,926 direct width measurements in 12.995 s, 12 host installs (maximum 6.60 s), nine layout builds in 15.36 s, and 45.64 s cumulative page-script execution (33.72 s maximum for one script). The previous run measured 20.46 s for text widths, but the public DOM varies, so this is not a controlled speedup comparison or a Wikipedia pass. The preceding host-view deadline and earlier QuickJS null exception are distinct failures. Do not report Wikipedia compatibility or this journey as a pass. Commit `ac35a9d7` then made each uncached style computation build one selector-candidate list, sharing it between the custom-property and computed-style cascades and reusing its allocation across ancestors. The sorted candidate results match the allocating oracle, warmed scratch capacity stays stable, two layout-style-cache regressions pass, and strict native-core Clippy passes. A single bounded native Wikipedia article-to-observed-subpage run after the host-snapshot and allocator-pressure changes ended inconclusive at the runner's 130.9-second outer deadline, before any step evidence. It recorded 33.46 seconds in document-wire computed-style projection, 20.59 seconds in page-script execution, 16.27 seconds in native CSS style computation, and 12.50 seconds in JavaScript host-view evaluation; no individual host-view completion phase exceeded 5.19 seconds in this trace. This is not a Wikipedia pass, and the 120-second journey remains open. The redacted report and private trace are `/tmp/glass-native-journey-wikipedia-post-host-memory.json` and its adjacent `.artifacts/` directory. The new deterministic 8,009-node mixed-DOM style fixture produces identical computed styles on the cached-parent fast path and forced full-chain path; measured preorder style passes were 3.886 s versus 5.509 s respectively (29% faster in this fixture). This does not establish a Wikipedia pass. | These are bounded fixtures and measured journeys, not complete HTML/CSS standards conformance or broad website parity. One current trace shows large per-document style/layout and page-script costs; the detailed report is `/tmp/glass-native-journey-wikipedia-element.json` with its mode-0600 trace beneath `/tmp/glass-native-journey-wikipedia-element.artifacts/`. The native engine now has a bounded silent WebM/VP8 path: the local authored two-color journey verifies real decoded frames, duration, seek/play/pause, elapsed playback, and changed painted pixels. Per-resource limits cover encoded bytes, dimensions, frame count, and decoded allocation; local regressions verify frame-count, dimension, and allocation rejection. The Rust demuxer and software decoder are optional native-engine dependencies, and FFmpeg is used only to author local fixtures. Audio output, captions, other codecs/containers, and public-site video remain unsupported or unverified; poster fetch/render remains a separate image path. See the [native media capability and dependency audit](docs/architecture/native-journey-corpus.md#native-video-capability-boundary). Universal/complex selectors still use fallback matching. Anti-bot behavior is not claimed. Chromium remains an explicit compatibility backend. |
| QuickJS host-view memory and reuse | `glass-browser-core` native JavaScript runtime and DOM projection | The realm remains bounded at 256 MiB. Temporary host JSON is installed as a runtime string and deleted by bootstrap; style interning reduces duplicated computed-style data. Initial ordinary-element `text` and `inner_html` are no longer redundantly serialized into every ancestor snapshot entry: JavaScript getters derive them from the already-linked child identities, while script and option text remain explicitly projected. The allocator-pressure collection guard also checks QuickJS `malloc_size` and reclaimable slack, so retained-allocation pressure can trigger collection even when `memory_used_size` is below its threshold; both checks leave the 256 MiB cap unchanged. The static host-view installer is compiled once and retained privately in the Rust runtime via a QuickJS persistent handle. A runtime-scoped cache key reuses the immutable host state only when the document projection and that runtime’s last successful state both match; a second runtime sharing the same Rust document receives a full snapshot. A trace-backed 2,586-element regression observed five reusable installs across seven bootstraps: full host-state JSON was 4.72 MiB, while reused state reduced each payload to about 59.5 KiB; repeated host-view execution still took 1.65–1.92 s. The same regression checks the fixed heap limit and a second runtime receives full state. A follow-up same-state regression preserves document root identity and document-subtree MutationObserver delivery across repeated installs. The installer is not exposed on `globalThis`; the repeated-install regression checks that. The handle is explicitly released before the QuickJS runtime drops. An opt-in heap guard runs cycle collection above 128 MiB and leaves the fixed cap unchanged. Snapshot projection reuse is keyed by revision, invalidation generation, viewport, and text limit; it skips animated images and estimates duplicated retained bytes, refusing snapshots above 16 MiB. Focused cache tests verify fresh-document equivalence after attribute mutation, image-property refresh, animated-image exclusion, and oversized file-payload exclusion. | One fresh React Docs navigation without collection hit a QuickJS `Null("null")` host-view exception at a recorded 172.6 MiB heap. After the 128 MiB collection guard, two collections reduced 134.7→119.5 MiB and 129.9→115.8 MiB; the observed heap stayed near 130 MiB and the Null exception did not recur. The bounded 120 s command still timed out during bootstrap style interning on a later host view and reached no observation. Collection addresses transient heap pressure; it does not establish that React Docs loads successfully. A later rebuilt React Docs run exposed the remaining failure as one 18.22 MiB `document_base64` field exceeding the unchanged 16 MiB IPC frame cap. Document snapshots now use fast zlib transport compression only when it reduces bytes; legacy raw snapshots remain readable, and the 16 MiB decompressed-document and IPC limits remain fixed. The following single polite incognito React Docs batch completed navigate, observe, scroll 400 CSS pixels, observe (4/4 steps, final revision 3) in 97.33 s, with 8 page-script evaluations (63.12 s cumulative, 25.71 s maximum) and 16 host-view evaluations (29.60 s cumulative, 5.37 s maximum). This verifies one React Docs load and two observations under the bounded native backend; it does not establish universal React compatibility or broad public-site parity. A second fresh batch after the generation-keyed host-state invalidation and unchanged-tree synchronization also completed 4/4 at revision 3 in 88.04 seconds. Its trace measured 16 host-view evaluations at 26.13 seconds total, 8 page-script evaluations at 55.96 seconds total (21.11 seconds maximum), five layout builds at 14.88 seconds, and 3.52 seconds to encode the document wire; sampled QuickJS heap use peaked at about 120 MiB under the same 256 MiB cap. The earlier batch took 97.33 seconds, so the shorter second run is one observation rather than a controlled benchmark. A subsequent React Docs run after caching decoded canvas image resources completed the same four steps in 75.80 s (max RSS 142,008 KiB). Its privacy-safe trace measured 11 content-worker host-view installs at 10.91 s total (0.272 s median); image-resource projection cost 2.65 s across those installs, with 1.404 s on the initial full state and later exact-state installs rounded to 0.00 s. The earlier single run measured 87.95 s overall and about 22 s in host-view work; these runs are not a controlled benchmark. A regression verifies reused decoded resources, changed-resource invalidation, and isolation from page mutations of the public maps and pixel arrays. The bounded per-node style cache now returns before ancestor inheritance is recomputed when the style epoch is unchanged; `ClearFileInput` explicitly invalidates that epoch, with a required file input validity regression. A fresh rebuilt React Docs batch completed the same 4-step flow. Its trace recorded 4,463,834 raw snapshot bytes and 726,932 compressed bytes (under the unchanged 16 MiB cap), with three full 4,436-node style passes (3.87 s, 4.77 s, and 4.17 s) followed by passes at 8.6–9.7 ms; page script execution was 21.78 s cumulative and bootstrap host-view work 12.80 s cumulative. This is one bounded native run; it does not prove universal site compatibility or a controlled speedup. |
| Native public-page breadth and resource limits | `glass-browser-core` native engine; `glass-browser` runner; Glass maintainers own acceptance evidence | A prior incognito native Amazon page completed at revision 2. A read-only Wikipedia probe measured 799,591 response bytes and 15,475 parsed nodes; this exposed the 256 KiB document cap. Bounded defaults were raised to 1 MiB and 16,384 nodes, and content parsing stops token processing immediately on DOM/depth budget failure. Cache invalidation tests cover target, hover, focus, script style changes, `:has`/`:first-child` tree mutations, viewport changes, decoded image/video-poster intrinsic-size changes, and clone isolation. A new per-document one-entry layout snapshot cache is keyed by revision and viewport and cleared with computed-style invalidation; its regression compares cached output with fresh layouts after style mutation, viewport change, resource arrival/removal, and clone. The native CSS parser retains at most 64 `@font-face` rules; excess faces produce a bounded diagnostic. The 65-rule regression passes. A 2,586-element local host-view fixture reproduced QuickJS `Null("null")` allocation failures at 64, 128, and 192 MiB; the bounded 256 MiB realm cap and 15 s host-view-only deadline pass while page-script execution remains separately bounded at 5 s. Interning two computed styles reduced serialized host state from 14.15 MiB to 4.66 MiB; measured retained heap after bootstrap was 119.6 MiB. | Earlier bounded runs found Wikipedia beyond its deadline; its CSS background-image discovery took 34,096 ms for 230 selected links. React docs exceeded the deadline during bootstrap; later font-face and custom-property memoization reduced its measured work, but its run still ended in a worker interruption before inspection. On 25 Oct, source trace identified a 1,025-byte MDN attribute exceeding the old 1 KiB wire limit and lifecycle mutation work cut at the generic 5 s deadline. The per-value cap is now 64 KiB (with serialized document and node budgets unchanged); the regression accepts exactly 64 KiB and rejects a larger value at content-wire validation. Lifecycle event dispatch now has a separate fixed 30 s ceiling; ordinary mutations remain at 5 s. The content-process load guard is bounded at 120 s, while the caller's explicit timeout now also bounds native CLI navigation including lock wait and revision validation. A stalled local response regression exercises the one-second session API deadline and verifies observation and close remain responsive afterward. On 25 Oct, a fresh React Docs run using the rebuilt CLI and a 30 s caller deadline returned a typed timeout at 30 s, proving the CLI no longer silently waits for the 120 s process ceiling; the page did not reach inspection within that bound. Rebuilt polite native MDN navigation completed at revision 2 in 35.6 s on an isolated profile: 2,414 nodes, 850 element snapshots, 1.24 s host-view evaluation, 16.0 s page-script execution, and 6.77 s lifecycle mutation. A fresh polite GitHub navigation completed at revision 2 in 80.6 s with an explicit 90 s caller limit and no login. Its document had 2,483 nodes, 1,856 element snapshots, and 1,113 layout boxes; host bootstrap was 8.37 MiB, evaluated in 1.91 s, and retained about 107.6 MiB. Module ordinal 7 took 21.99 s. A later same-revision layout-backed snapshot reused cached layout in about 2.9 s without rebuilding the layout tree, after the initial full layout took 12.84 s. The page still takes substantial time to initialize; this does not establish broad compatibility. A fresh polite native incognito batch on Rust’s public site (`www.rust-lang.org` redirecting to `rust-lang.org`) completed navigate, observe, scroll by 400 CSS pixels, and observe again: 4/4 steps succeeded at final revision 3. Content loading took 42.5 s and content application 1.46 s; the final snapshot had 639 nodes, 242 elements, 197 layout boxes, a 2.79 MiB bootstrap, and a 1.48 s host-view evaluation. The session used no login and performed no page-data writes. After memoizing nearest scroll ancestors per layout snapshot, a traced React Docs run measured native layout tree build at 6.91 s and the complete 34-container scan at 0.318 s; the earlier 30 s probe had been cut off inside that scan. The 90 s run still timed out during classic script ordinal 4 after repeated 8.47 MiB bootstraps, 10.7–12.9 s DOM snapshots, and about 2.94 s host-view evaluation; it did not reach observation. An earlier fresh Wikipedia run reproduced `Exception generated by quickjs: Null"null"` after 38.7 s with 3,911 snapshot nodes, 2,308 elements, a 12.05 MiB host bootstrap, and 11.21 s host-view evaluation. Corrected stage tracing identified this host-view evaluation as the failing stage; retained heap immediately after failure was 163.1 MiB against the fixed 256 MiB QuickJS limit. The pinned QuickJS `quickjs.c::JS_ThrowError2` path deliberately uses `JS_NULL` when it cannot allocate an Error object to report out-of-memory, and `JS_ThrowOutOfMemory` uses that path. The bootstrap contains no literal `throw null`; this is source-supported evidence of transient allocator OOM during host-view evaluation. The post-unwind heap reading is not a peak measurement. The host-view stage failed at 11.21 s, below its 15 s host-view deadline and the 60 s caller budget, so this was not a deadline interruption. After moving host JSON into a runtime string and out of the generated JavaScript literal, a fresh polite native incognito Wikipedia batch completed navigate, observe, scroll 400 CSS pixels, observe (4/4 steps, final revision 3) in about 53 s. Eleven host-view installs completed; the final host JSON plus bootstrap source measured 5.31 MiB, the slowest host-view eval was 4.01 s, and retained QuickJS heap after the final bootstrap was 128.3 MiB. The same bounded run had 2,764 content-wire nodes, 1,195 element snapshots, 282 layout boxes, and 40.5 s cumulative page-script execution. A later React trace exposed repeated ancestor clip work: an O(nodes) batched overflow/SVG clip projection now preserves the prior per-node algorithm across ordinary, nested-scroll, fixed, and SVG fixtures. The equivalence regression and overflow/SVG integration cases pass. In the latest single 120 s polite React navigation, overflow clipping took 1.97 s across 16 snapshots and projected clipping 1.83 s; the older traced run spent 12.52 s and 13.20 s respectively. The initial 8 MiB run overflowed the host main thread during initial-shell CSS styling while the content worker was concurrently processing React; a single diagnostic with a 16 MiB process stack ran the React worker through script ordinal 8, and the CLI now uses that fixed 16 MiB main-thread stack. With a rebuilt CLI and standard launch environment, one 120 s polite incognito batch reached the worker but ended with an IPC exit before observation. Its 11 document bootstraps took 49.38 s, host-view evaluation 32.02 s, and 8 page-script evaluations 72.28 s. A one-entry 16 MiB-bounded scroll projection cache produced 9 hits across 11 snapshots; layout geometry projection fell from 14.72 s to 2.56 s, and repeated snapshot DOM projection fell from 47.55 s to 30.66 s. This is measurable improvement but React still did not reach observation; it is not a site compatibility pass. React script execution and host-view installation remain further optimization targets; eBay returned HTTP 403 under the polite policy. A fresh native-only Wikipedia article→observed-subpage attempt on 25 Oct failed before observation after 113.2 s. The final 16,053-node wire (8,015 element snapshots) required an 18,544,284-byte host snapshot; host-view install reached exactly 15,005,447 μs and raised `Error: interrupted`, matching the fixed 15 s host-view limit. This run is classified as `native_host_view_deadline`, not OOM evidence. The prior QuickJS null exception at a later element remains a separate unresolved failure mode. The local dense 5,500-anchor/1,000-span test and three native corpus journeys pass, but do not establish Wikipedia compatibility.  A native GitHub Rust-repository journey passed navigation, observation, and its repository identity assertion; the responsive library link resolved uniquely, but click exceeded the fixed 5-second content-process deadline before subpage observation (31.3 seconds in the target-resolution run). Live markup showed duplicate small/large responsive links; the local region-scoped duplicate-link regression passes. A follow-up after the hidden-state map optimization still exceeded the deadline at 5.143 seconds after 30.3 seconds total, with no subpage observation. The local 5,500-link host-view fixture passes; public timings are not a controlled comparison. Native snapshot visibility now propagates inherited hidden state in one tree traversal, with CSS-hidden and attribute-hidden descendant regressions. Native semantic target enumeration also filters nodes carrying that hidden state; the regression verifies that only the visible responsive repository link is offered. The versioned local journey runs through `BrowserRuntimeSession.observe()` and `clickObserved`; it places the visible target beyond the 32-target response window, then resolves one exact role/name match from the fresh bounded native semantic snapshot and clicks its revision-bound reference. Hidden responsive duplicates are excluded. The earlier public run still did not pass GitHub or demonstrate a controlled public speedup; this local fix does not upgrade that result. This single bounded failure does not establish that every native site fails or succeeds. |
| Native behavior inventory | `glass-browser-core` engine and `glass-browser` integration tests | The current `glass-browser-core` native-engine lib suite passes 932/932, including the fragment-child command-order and dynamic script referrer-policy regressions; the suite was rerun after both fixes. Fixtures span browser contracts/interactions, frames, forms/input, lifecycle, fetch/XHR/EventSource/WebSocket, workers/storage, canvas, APNG/GIF/WebP, SVG, CSS/layout/paint, and resource policy. Coverage includes a focused Grid layout/paint/hit-test case, sticky positioning, canvas readback/raster, and many pointer/hit-test paths. The full run also passes the formerly failing adoption-bound, same-origin frame load, no-listener resource-effect, explicit-stack form-mutation, and animated APNG data-image cases. Core style and layout cache tests compare output with fresh documents and exercise viewport, script style, dynamic selector, tree mutation, clone isolation, and class-attribute changes. | This is deterministic local fixture evidence, not complete standards conformance. Grid has one focused case rather than a broad layout matrix. Shadow DOM traversal/rendering, pseudo-element painting, lazy-media behavior, and accessibility conformance remain unverified; SPA primitives have fixtures but app-level compatibility remains open. Native public-site observations now include Amazon, MDN, GitHub, Rust, and Wikipedia. React Docs now completes one polite native navigate/observe/scroll/observe batch at revision 3; eBay returned HTTP 403 under the polite policy. These are evidence gaps, not claims of unsupported behavior. |
| Core runtime feature controls | Glass Core owns provider/model/session/agent policy and durable tasks; Glass Dev owns relay and in-workspace presentation | Glass relays Core events, keeps host workspace controls separate, and forwards Core commands through `glass <command>` or `glass core ...`. Model selection and `taskGraph` use runtime-backed controls. `/settings` now uses the existing Glass Core settings/model panel; the three-size fake-runtime PTY verifies session-settings and model-catalogue RPC results, provider/model display, no leakage of the private fixture metadata canary, and composer return. | The panel exposes effective session fields and model selection; it is not provider/account management. Other interactive Core commands remain explicitly unsupported in Glass, with no action or conversation message. A full API inventory and in-workspace acceptance for those commands remain open. |
| Remaining Code and Debug scope | Glass Dev editor and DAP integration | Single-file preview/edit, dirty-buffer protection, exact selected-range extraction, multi-buffer context attachment/removal, per-file LSP WorkspaceEdit preview/accept/save, and real Rust Analyzer cross-file rename are covered by state, router, fixture, or PTY evidence in the Code row above. | Language-service formatting/diagnostics still depend on configured servers. Useful configured Debug execution remains unverified. The Agent no-implicit-context rule remains part of acceptance; unavailable diagnostics/debug capabilities must stay explicit. |
| Issue #65 audit | Glass maintainers own the disposition record; monorepo `glass-core` owns Core permission/read-only/browser policy | Existing comments record the MCP lease-context and stale TUI assertion fixes. A source pass over concrete 51–55 references found format-derived connect origins (`connect.rs::authorized`, covered by `identity_and_origin_are_both_required`); authorization and local POST validation now use checked `origin_authority` parsing instead of two `split_once("://").unwrap()` assumptions, with malformed-scheme regression coverage. A `by_index` map built from the call enumeration consumed by the concurrency plan (`agent.rs`), bridge-origin validation reached only after `bridge_origin.is_none()` is rejected (`surfaces.rs::SurfaceProvenance::validate`), and SVG attributes traversed as complete pairs (`extraction.rs::svg_attribute`); no input-triggered panic was reproduced. The reported MCP server count (#56) is source-dispositioned: all 390 current `.unwrap()` sites are inside the `#[cfg(test)]` module beginning at `server.rs:7788`; the production section before test modules has zero `.unwrap()` sites. A targeted pass over the representative paths for 57–60 found the no-input agent approval path returns before `input_keys.unwrap()`, the parallel `by_index` map is built from the same indexed call set used by its plan, current production `session.rs`/`checkpoint.rs`/`queue.rs` contain no `.unwrap()` or `panic!` calls, and team map lookups are checked or keyed under the same held state lock (its 34 production unwrap sites are lock/map invariants; a poisoned lock still causes a theoretical panic without an identified triggering panic source). A further pass found an actual duplicate-execution hazard: `concurrency::execute` discarded a panicking worker result, after which `agent.rs` treated the absent index as unprocessed and retried the tool inline. The executor now preserves that index as `ToolPanicked`; Agent surfaces an explicit unknown-result error and does not retry. The deterministic regression simulates a side effect before panic and verifies a result is returned for every call in stable order, including the panicked call. The 163-block/four-declaration workspace unsafe inventory and source-contract review are recorded in the Issue #65 inventory section below; QuickJS value ownership, OS handle lifetime, syscall result handling, layout initialization, descriptor use, and terminal initialization are grouped by owning module there. The concrete manual-clamp/collapsible-if references (#81–82) do not reproduce as current lint failures: strict all-target `glass-core` Clippy passes, and the cited render/state line locations no longer contain those constructs. The full `glass-core` binary test target now passes 939/939 after correcting stale built-in tool-name assertions to include `web_search`/`web_fetch`, an outdated fixture that expected horizontal Mermaid edges to render vertically, and the new provider usage test fixture to include answer text. The refreshed scan finds 23 remaining product-code `dead_code` annotation sites and one test-only helper in product source, plus the separate integration-test allowance. Annotation sites include module- and impl-scoped suppressions, so the earlier 44 annotated-item count was stale and is not a defect count. The source review removed suppressions from production-used request/CORS kinds, queue accessors, session metadata, sandbox policy helpers, and extension failure/cancellation paths. `Session::turn` remains because the CLI reads it. `fs_scan::Walk::{dirs,bytes}`, `NativeJavaScriptRuntime::has_pending_jobs`, and the horizontal flowchart allowance were removed in earlier checkpoints. Remaining annotation sites and host/test hooks are listed below by file. | #65 remains open. Items 61–80 and 91–108 have no individual text in the filed issue body; their missing descriptions and paths cannot be source-dispositioned. Items 83–90 are themes rather than eight numbered findings: pedantic/native-test lints, numeric casts, unsafe blocks, TODO macros, `dead_code` and module-level allows, concurrency, and error/policy/sandbox/authoring audits. The original targeted cast lint reported 150 warnings across 38 `glass-core` files (43 paths including `glass-tui`). All original warning paths received source-level dispositions. A fresh successful run of the same four targeted cast lints reports 45 `glass-core` binary warnings plus 3 `glass-tui` display-precision warnings across 26 paths; the test target additionally emitted 50 diagnostics, 44 duplicating other target warnings. The captured run contains 51 distinct cast line sites. Concrete checked/saturating fixes cover protocol/provider stream indices, provider usage totals and prompt estimates, session and log indices, exit codes, durable/web cursors, runtime time/pagination bounds, provider durations, sandbox ABI values, TUI scroll/dimension math, and Mermaid chart coordinates/date ratios. The current source sites were reviewed by path: Unix `Child::id` values fit the positive `pid_t` contract; bounded byte/cursor conversions stay below documented 8 MiB, 32 KiB, or 10M limits; `u32`→`i32` request values are constrained before narrowing; durations saturate at `u64::MAX`; provider pressure and request-cost conversions affect only fractional threshold/report values; and `glass-tui/layout.rs` casts format elapsed-time display only. The clean exit code is current lint evidence; warnings remain source-dispositioned rather than individually suppressed. In addition to the earlier checked extension protocol version, provider-reported `u64` usage counts now clamp to `u32::MAX` instead of wrapping; prompt bucket totals saturate. Anthropic and OpenAI stream indices now use checked `u64`→`usize` conversion with explicit errors, and token estimates saturate before clamping to `u32`. Landlock ABI version conversion now checks signed and `u32` bounds before accepting the kernel response. Session and persisted-log turn/index conversions now reject values that exceed platform `usize`, and malformed persisted bash exit codes outside `i32` no longer wrap. Runtime inspect/model cursors, durable-entry cursors, artifact byte offsets, and web source-record indices use checked conversions. Durable change-feed limits clamp to the documented 1–256 range before platform conversion. Tests cover oversized provider values, summed usage overflow, session indices and exit codes, and checked cursor conversion; full Core tests and strict all-target Clippy pass. The remaining cast warnings are retained with the bounds and display-only/OS-ABI rationale above; the finding remains open for the independent error, policy, sandbox, concurrency, and authoring sweeps below. A block-by-block review of every `unsafe` site in `crates/glass-core/src/sandbox.rs` found no definite production memory-safety flaw. It identified parent-process ruleset-fd leaks in Linux tests, now wrapped in `OwnedFd`, and repeated `sandbox::init` calls that silently retained the first `OnceLock` state; initialization is now serialized and repeated calls warn while retaining that first process policy. The original 207 lexical `unsafe` token count is not a block count; the current syntactic inventory is recorded below and distinguishes blocks from declarations. Every remaining source annotation site is listed by item and file below; module-scoped annotations may cover multiple items. The separate integration-test contract contributes one additional test-only allowance. The native installer cache also uses a private `PersistentHostViewInstaller` wrapper with an `unsafe impl Send`; its safety argument is exclusive ownership, mutex-serialized context access, and explicit persistent-handle clearing before runtime drop, with its focused large-host-view regression and strict native Clippy check passing. Executable `todo!()`/`unimplemented!()` macros were absent, and the side-effect-then-panic concurrency retry was fixed with a regression. A bounded Linux shell policy sweep found that an egress proxy startup failure previously left `STATE.refusal` empty despite the fail-closed comment. Refusal now also covers a requested policy whose proxy started but whose workspace confinement fell back to `Policy::Off`; deterministic regressions cover unavailable confinement and proxy startup failure. The process-wide initializer is serialized and explicitly retains the first configuration on repeated calls. The extension pipe wait previously declared `poll(2)` with a hand-written ABI that fixed `nfds_t` to `u64`; it now uses the platform `libc::pollfd` and `libc::poll` declarations. The 940-test Core target, including a readiness regression for empty and readable streams, strict all-target Core Clippy, and formatting check pass after this portability fix. The CONNECT proxy still configures cooperative proxy environment variables; it does not enforce host-level egress for commands that use direct sockets, and platform/network policy limits remain an explicit audit boundary. The residual cast review is source-based and does not add independent boundary tests for every conversion. The unsafe source inventory and contract review are now recorded below. The full lint/allow-attribute disposition, broader race/interleaving matrix, exhaustive error-handling sweep, and complete authoring-input path list remain incomplete. The policy and sandbox entry paths exercised in this hunt are source-dispositioned below with their focused test evidence. A deterministic authoring-input reproduction found YAML aliases could expand a source below the 256 KiB input cap into repeated large scalar allocations; bounded deserialization now caps expanded values at 32,768 nodes and 1 MiB of scalar/tag bytes, with regressions for an over-limit alias and an accepted small alias workflow. A focused signed-request review found CR/LF injection through method, target, and digest fields plus unescaped agent/key identifiers in RFC 9421 structured strings; the fix validates method tokens, preserves method case, rejects controls, escapes structured strings, and passes three focused regressions. Keep the broader authoring path list and other absent entries unverified rather than counting pattern matches as findings or silently closing the issue. |
| Native CLI stack behavior | `glass-browser` and Glass Dev native CLI; workspace maintainers own entrypoint validation | Both the standalone `glass-browser` CLI and the user-facing `glass` launcher now move command execution to a fixed 16 MiB main thread; Tokio workers remain bounded at 4 MiB. The large CLI/native-owner regression still runs on an explicit 8 MiB test thread. Before fixing the Glass Dev entrypoint, native CLI runs from `target/debug/glass` overflowed the default Linux main-thread stack during frame reconciliation, including a minimal `example.com` navigation; after the fixed 16 MiB wrapper, `example.com` navigation passed at revision 2 and a fresh polite incognito React Docs batch completed navigation, observation, a 400 CSS px scroll, and a second observation (4/4 steps, final revision 3). | A pre-cache React run sampled QuickJS at 118.1 MiB maximum; it measured 16 host-view installs/24.43 s total, 8 page-script evaluations/54.90 s total, and six layout builds/19.82 s total. The preceding successful batch measured 26.13 s host-view, 55.96 s page scripts, and 14.88 s layout; these single runs are not a controlled performance comparison. A follow-up privacy-safe trace split direct JavaScript source evaluation from host setup: source evaluation totaled 0.435 s across 16 calls and pending jobs totaled 0.1 ms, while six layout snapshots totaled 20.81 s; two 1,223-box layout builds took 10.51 s and 10.30 s. The outer 55.8 s page-script-stage total includes that native snapshot and host-view work. The later decoded-resource cache React batch stayed near 112.5 MiB maximum QuickJS heap and 142,008 KiB process RSS; its detailed one-run result is in the host-view row. A later rebuilt React run completed all four steps in 88.82 s at 139,048 KiB RSS. Its trace measured 61.96 s in page script execution; the first full snapshots took 10.48 s and 10.31 s for native layout on 1,223 nodes, and 14.67 s and 17.37 s for bootstrap/host setup. Later matching host snapshots took about 50 μs and later script evaluations about 1.7 s each. This points to repeated invalidation during initial script setup as the remaining measured work; it is a single unpaired run, not a controlled speed claim. A later run from the same polite incognito harness failed before observation when the decoded document snapshot exceeded the fixed 16 MiB budget by at least one byte; its 4,436-node projection confirms public-site document size varies between runs. Do not infer stable React pass/fail or parity from either run. A per-node computed-style cache fast path now avoids rebuilding ancestor inheritance for already-computed styles; equivalence and invalidation tests pass. Identical script attribute value and namespace writes also preserve valid computed-style and layout caches; the focused regression verifies unchanged layout output, while the existing changed-attribute regression still verifies invalidation. This is an invalidation reduction, not a measured end-to-end speedup. One fresh React Docs run completed the four-step native flow; its exact raw/compressed snapshot sizes and style-cache timings are recorded in the host-view row. A deep Svelte reactivity docs run on 25 October passed native navigation, observation, title assertion, 400-pixel viewport scroll, and post-scroll observation in 80.21 s (6/6). The same run measured 47.61 s page-script evaluation, 33.36 s document bootstrap, 16.85 s CSS style computation, and 14.73 s layout build. These overlapping stage totals show a functional pass with high load latency, not broad parity. A bounded polite MDN Array-reference run initially failed native document validation because a computed background-image ID had no source-map entry. The source collector now registers URLs declared in CSS custom properties and recursively follows supported `var()` fallbacks; a local content-wire round-trip regression reproduces that case. After rebuilding, one polite MDN batch completed all four steps at final revision 3 in about 107.57 s; its 5,490-node snapshot was 10,793,269 raw and 1,077,721 compressed bytes, page-script evaluations totaled 41.42 s, and four 1,822-box layout builds took about 7.0–7.4 s each. This is one near-deadline run, not broad parity evidence. A bounded polite eBay attempt returned HTTP 403; no outcome was bypassed or blindly retried. A GitHub Rust-repository journey used fresh semantic role/name-fragment matching and safely collapsed two visible link nodes only because their privacy-safe destination fingerprints matched; a local regression confirms different destinations remain ambiguous. The first public run exposed a 5-second listener-free beforeunload mutation timeout. A guarded no-listener fast path now reuses the unchanged parent document and skips redundant snapshot/resource settlement; it preserves full dispatch whenever listener state is unknown or any inline/registered listener exists. The versioned journey was corrected to the live `main` branch path without weakening its body-content assertion. One bounded native run then passed all six steps (navigate, observe, semantic link click, subpage observe, URL and `core` content assertion) in 50.62 seconds, ending at `/rust-lang/rust/tree/main/library`; click plus navigation took 22.47 seconds and the listener-free mutation took 14.88 ms. This is one public-site run, not a parity claim. The stack fix is bounded and leaves the 256 MiB heap and operation deadlines unchanged; this does not establish that every native site completes. |
| GC search and fetch | Monorepo `glass-core`, `crates/glass-core/src/web_tools.rs` | Baseline `af869913`; the slice adds declared-charset decoding with explicit fallback warnings, preserves byte-limit errors, normalizes/deduplicates Exa MCP source URLs, honors requested result count, and labels provider/source count. Nine deterministic tests cover local HTTP encoding/oversize/redirect cases and mocked duplicate Exa URLs. | No paid account, external search query, provider summarization call, or Chromium fallback was used. Ranking/freshness quality and public-search latency have not been benchmarked; this is bounded data-correctness work, not a best-in-class search claim. Separate changes in `/home/ubuntu/work/glass-core` are preserved and are not the shipped monorepo implementation. |

## Issue #64 standalone native browser session lifecycle (2026-10-27)

The screenshots showed a real distinction: `glass-browser navigate` is a
one-shot operation, so each separate invocation starts and closes its own
native engine. The named native session API is the supported cross-invocation
path (`session start NAME`, then `--session NAME` for commands, and explicit
`session stop`). A lifecycle gap caused that owner process to inherit the
caller's process group; Core's intentional foreground shell cleanup could
therefore kill it as soon as `session start` returned.

The managed owner now starts in its own OS session. Its private status file and
command socket continue to define the owner, and explicit stop closes that
one browser. A subprocess regression starts a native owner under an isolated
caller process group, applies the same group kill used by Core bash cleanup,
then runs separate CLI processes from a temporary working directory outside
the repository. It navigates to a local HTTP fixture, observes it, types into
its name field, clicks Save, and verifies a later text call reads `Saved Ada`.
The same owner also survives a failed command and a cancelled attached wait on
`about:blank`; explicit stop exits the owner and records stopped status. The
fixture listener and config are test-owned and private; no public network,
Chromium, or external browser is involved.

This fixes the managed named-session lifecycle. A plain one-shot command
remains intentionally stateless, and the screenshot alone did not establish
whether a named session had been started. The owner has to be started and the
same name passed on each later command. Core's compact base prompt now says
that multi-call browsing needs a named session; its on-demand research recipe,
`gc help web`, and browser CLI long help show the exact start/attach/stop
syntax. Regression assertions enforce the compact prompt and recipe byte
budgets.

## Native Wikipedia follow-up (2026-10-26)

One bounded native run used the rebuilt monorepo browser and worker after adding reusable selector-candidate storage to background-image discovery. It ended inconclusive at the harness's 130.9-second outer deadline: navigation hit its 120-second step deadline before the journey could observe the article or follow a subpage. The run is recorded at `/tmp/glass-native-journey-wikipedia-background-scratch-fff88d7b.json`; its mode-0600 diagnostics and progress files are in the adjacent `.artifacts/` directory. It is not a Wikipedia pass.

The trace measured 38.37 seconds for document-wire computed styles, 33.04 seconds for native CSS computed styles (49,330 calls), 21.17 seconds for custom properties (49,330 calls), 22.85 seconds for layout snapshot work, 19.44 seconds for document-wire encoding, 42.01 seconds for page-script execution, 22.04 seconds in the interrupted content lifecycle mutation, 7.38 seconds for background-image discovery, and 13.38 seconds in JavaScript host-view evaluation. A local 5,005-element/128-rule fixture returned identical background-image candidates with allocating and reused-scratch paths; timings were 18 ms and 17 ms respectively. That fixture shows the allocation path is equivalent but does not establish a material performance improvement or explain the public-site deadline. The 120-second Wikipedia journey remains open; no unchanged public-site rerun is warranted until a measured product bottleneck is addressed.

The measured custom-property loop previously tested every style-rule candidate, including ordinary property declarations. The stylesheet now builds a selector index containing only rules with custom-property declarations, while ordinary computed-style matching keeps the complete candidate index. A local fixture with 8,029 elements and 1,056 matching rules (32 carrying custom properties) produced identical custom-property maps. The custom-property pass measured 14.521 s with all candidates and 1.196 s with the specialized index, reducing the matched candidates from 1,056 to 32. All 21 layout-style-cache tests and strict native-engine Clippy pass. This is synthetic fixture evidence; the previous Wikipedia trace's 21.17 s was cumulative over 49,330 calls and no public-site outcome is inferred from the fixture.

One bounded native Wikipedia journey after rebuilding the browser and worker with this index completed navigation and article observation (the first two steps), then the outer 130-second runner deadline interrupted the journey's wait step before the observed subpage. The result is `inconclusive`, not a journey pass; see `/tmp/glass-native-journey-wikipedia-custom-property-index-772d8f3a.json` and its adjacent private trace. This trace recorded 49,643 custom-property calls totaling 0.167 s, compared with 49,330 calls totaling 21.167 s in the preceding run. Native style computation measured 19.344 s versus 33.036 s, and document-wire style projection 30.243 s versus 38.369 s; those stages and other page work can vary with the live response, so the overall comparison is not controlled. This is a clear improvement in the targeted custom-property phase at near-identical call counts, but the page still did not reach its observed subpage and the 120-second journey remains open.

After `cc3a8791`, a bounded Wikipedia journey using the text-width cache and rare-selector anchor completed article navigation in 96.343 s but did not finish its first observation before the 130.978 s runner deadline. The report is `/tmp/glass-native-journey-wikipedia-layout-cache-cc3a8791.json`; its private trace is in the adjacent artifact directory. Trace review found 48 full style invalidations, 36 caused by decoded image arrivals. Image and video-poster resource arrival now invalidates only layout and script projections, preserving the unchanged CSS and custom-property caches (`b612c105`). A local 36-image regression verifies that each decoded image rebuilds geometry while retaining equivalent cached styles and custom properties. A second bounded public run against that uncommitted source completed navigation in 95.697 s, then its observation was interrupted at the 130.920 s outer deadline. Its trace recorded 36 resource-only invalidations, 10 full style invalidations, 52,028 style-compute calls totaling 31.883 s (the preceding run recorded 55,135 calls/34.583 s), and 34.854 s for document-wire style projection (previously 37.941 s). The response and stage timings vary, so this is not a controlled end-to-end speed claim. Both runs remain inconclusive; no observation, semantic subpage, or Wikipedia acceptance is claimed.

A workload-matched local selector fixture now parses 4,096 rules over 16,006 DOM nodes and includes common tag/class rules alongside frequent and rare anchors. Reusable candidate lookup visits 16,928,000 rule candidates in 311–336 ms; computing styles for 8,004 elements takes 1.98–2.03 seconds. Candidate results match a full selector scan for representative anchors, and scratch capacity stays stable. A trial replacing the index's ordered maps with hash maps measured 317 ms candidate lookup and 2.10 seconds style computation, so that change was discarded. This local fixture suggests candidate-key lookup itself is not the dominant source of the public trace's 31.88-second CSS phase; selector matching and repeated document/style work remain under investigation. It is not public-page parity evidence.

Issue #65 Browser error-path follow-up: navigation preflight now carries the already-parsed URL through policy evaluation instead of calling `expect` on a second parse; backend startup converts an internally inconsistent selected position to `SelectionFailed`; CDP screencast sink access recovers a poisoned optional-channel mutex so the connection's event reader remains usable. The focused poisoned-lock regression and all 868 `glass-browser` library tests pass, as does strict Browser and native-engine Clippy. These changes address the reviewed local paths; they do not close the wider policy/daemon interleaving inventory below.

`RetryPolicy::max_attempts` is a public field, so callers can construct a zero-attempt policy directly even though `RetryPolicy::new` clamps it. Both retry loops previously skipped all attempts and reached `unreachable!`, panicking on that valid public struct state. Execution now clamps the effective attempt count to one in both click and navigation retries; the direct-zero regression passes with the full 11-test retry module, and strict `glass-browser` Clippy passes. This covers the retry-limit panic path; other workflow and platform interleavings remain open.

## Issue #65 finite acceptance reconciliation (2026-10-27)

This is a source-and-report reconciliation of the filed issue body, not a
claim that every raw pattern match is a defect. The issue body gives 35
numbered entries for #26–60 and two lint examples #81–82. It does not give
independent source paths or acceptance criteria for #61–80 or #91–108; #83–90
are broad audit themes, and #57–60 are grouped rather than individually
scoped. All 35 entries and both lint examples were reconciled with the current
source/report; 20 IDs (#61–80), 18 IDs (#91–108), and the eight thematic IDs
(#83–90) have no independent source-level disposition. The reported total of
108 therefore must not be read as 108 independently reproduced findings.

| Filed scope | Current disposition | Remaining acceptance |
|---|---|---|
| #26–30, Core read-only / sandbox profile | Permission-profile wiring, Linux deny-write behavior, and host-approval hooks have source dispositions and focused tests. | macOS profile behavior has not been run on macOS. Record a macOS regression result or an explicit platform support boundary before claiming cross-platform completion. |
| #31–40, shell read guards and command policy | Scanner limits and supported shell forms are documented; command ordering, wildcard matching, and read-only policy have focused coverage. `STALE_READ_CALLS = 25` is an advisory threshold, not a permission bypass. | This scanner intentionally recognizes a bounded command grammar; do not describe it as a complete shell parser. |
| #41–50, browser read-only and task policy | Current validators gate read-only mutations, unknown task risk fails closed, and navigation/preflight/workflow paths have focused dispositions. The filed `semantic.rs:1803` and `types.rs:1326` references no longer point to the described behavior. | Treat those two line references as stale; retain the current source/tests as the authority. |
| #51–56, panic-safety examples | Malformed origin parsing is checked; the parallel tool executor now returns an error for missing call/approval entries; the bridge-origin and SVG tuple paths are guarded by validated invariants. The MCP unwrap tally is test-only in the current source. | Focused tests cover the concrete fixes. Broader production error and concurrency review is separate from these examples. |
| #57–60, session / agent / team / checkpoint / queue | The issue supplies a grouped topic, not four independently testable descriptions. Reviewed follow-ups include session, checkpoint, queue, team-lock and MCP lifecycle paths; the report records each changed path and its tests above. | Keep the group open until each remaining named path has a bounded invariant/reproduction, or explicitly record that no distinct finding was filed for it. |
| #61–80 | No finding text or source path appears in the filed body. | No source disposition can be claimed from these absent descriptions. |
| #81–82, two Clippy examples | The cited manual clamp and collapsible-if sites are stale in current source; strict all-target Core Clippy passes. | Mark as stale examples, not active lint failures. |
| #83–90, hygiene and robustness themes | Cast sites, unsafe ownership families, dead-code annotations, tool-panic retry behavior, and selected policy/authoring paths have source review and focused fixes/tests. | Cross-platform sandbox behavior, wider daemon/request interleavings, and all remaining public structured/file input paths are not yet a complete audit. |
| #91–108 | No finding text or source path appears in the filed body. | No source disposition can be claimed from these absent descriptions. |

The finite remaining gates are: (1) obtain or explicitly bound macOS sandbox
support evidence; (2) finish one scoped pass over Core/browser daemon and
request interleavings, with named paths and deterministic regressions for any
reproduced defect; (3) inventory public structured-value and file parsers and
verify limits are applied before allocation/parsing; and (4) disposition the
grouped or absent issue ranges without inventing findings. Until those gates
are completed or explicitly de-scoped by the issue owner, #65 remains open.

### Issue #65 Core settings file bounds (2026-10-27)

Core settings were another repeated file-to-parser boundary: `config.toml` was
read without a byte limit by the main config loader, permission-profile
writer, and pre-tool hook loader; `requirements.toml` and `profiles.json` also
had unbounded reads. They now share a regular-file reader that preserves
symlink support, avoids blocking on FIFOs on Unix, and reads at most limit+1
bytes before UTF-8 or TOML/JSON parsing. The cap is 1 MiB for main config and
profiles, and 64 KiB for administrator requirements. Regressions for oversized
config and requirements files verify refusal before TOML parsing; the
full Core binary suite passes 965/965, config tests 27/27, requirement tests
7/7, permission tests 3/3, and strict Core binary Clippy passes. This bounds
these settings paths only; it does not complete the remaining public
parser/file inventory.

### Issue #65 plugin and durable JSON sidecar bounds (2026-10-27)

The next local JSON inputs were plugin manifests, add-on preferences, session
follow-up queues, and saved plans. Plugin manifests now use one no-follow
regular-file read capped at 256 KiB for both plugin loading and capability
checks, removing the metadata/read race and post-allocation size check. Add-on
preferences are capped at 64 KiB on read and write. Saved queues and plans
are capped at 1 MiB on read and write, so a generated sidecar cannot exceed
its own reopen limit. Focused over-limit regressions pass for all four paths;
the full Core binary suite passes 969/969 and strict Core binary Clippy passes.
This covers these local manifest/state inputs only; the remaining public
transport and authoring parser inventory is still open.

### Issue #65 ACP frame input bound (2026-10-27)

The ACP stdin reader used `BufRead::lines`, which could grow a `String`
without limit before JSON parsing. It now reads at most 1 MiB per line,
drains an oversized frame through its newline, reports it as a parse error,
and keeps framing aligned for the following request. A deterministic
regression covers an accepted boundary line and an oversized line followed by
a valid frame. The full Core binary suite passes 970/970 and strict Core
binary Clippy passes. This bounds ACP framing only; other transports remain
separate inventory entries.

### Issue #65 credential and trust store bounds (2026-10-27)

The credential and workspace trust JSON stores also accepted arbitrary file
sizes. Both now read through the shared bounded regular-file helper and cap
serialized writes at 1 MiB. An oversized credential store is returned as an
error without replacing credentials; an oversized or invalid trust store
continues to behave as an empty consent cache, so no prior trust decision is
silently granted. Focused tests cover oversized reads and writes for both
stores. This adds two settings paths to the bounded-input coverage and does
not close the broader issue inventory.

### Issue #65 served-session attach state bound (2026-10-27)

The detached-session attach cursor and pending-request state also used an
unbounded JSON read. It now reads through the shared 1 MiB regular-file cap
and rejects serialized writes over the same limit. Oversized or malformed
attach state keeps its prior empty-state recovery behavior. A focused
read/write regression verifies that neither direction accepts an over-limit
file. This closes the attach-state sidecar path only.

### Issue #65 session preference bind bound (2026-10-27)

Session preference display already read `meta.json` through the shared 4 MiB
regular-file cap, but workspace binding separately used `std::fs::read` and
parsed the entire file before checking the selected workspace. Binding now
uses the same bounded reader and refuses an over-limit or invalid preference
file without overwriting it. The focused regression writes a file one byte
over the cap and verifies it is rejected and preserved. This closes the
binding path only; other remaining parser and file inputs still require their
own inventory and evidence.

### Issue #65 interrupted-turn sidecar bound (2026-10-27)

Interrupted-turn recovery also read its `*.live.json` sidecar with an
unbounded `std::fs::read` before JSON parsing. Recovery now uses the shared
regular-file reader with a 512 KiB cap. The writer bounds provider-supplied
tool ID/name fields on UTF-8 boundaries and refuses any serialized snapshot
above that same cap. Regressions cover cap+1 rejection before parsing,
preservation of the over-limit file, and bounded identifier serialization.
All eight partial-sidecar tests pass. This closes this recovery sidecar only;
it does not complete the remaining public parser/file inventory.

### Issue #65 browser knowledge-store snapshot bound (2026-10-27)

`KnowledgeStore` validated serialized byte limits after reading persisted JSON
with `fs::read`, so an oversized snapshot could allocate before hitting its
existing store limit. Snapshot reads now take at most `maxBytes + 1` from a
regular no-follow file, apply the configured limit before JSON parsing, and
use the same limit for open, refresh, and mutation reloads. A deterministic
32-byte-limit regression confirms an oversized invalid-JSON file returns the
capacity error before the parser and remains intact. This closes the
knowledge-store snapshot boundary only; the remaining file/parser inventory
still needs scoped dispositions.

## Issue #65 dead-code allowance dispositions

The refreshed source scan finds 23 remaining product-code `dead_code` annotation sites plus one test-only helper in product source; the separate integration-contract test has one more test allowance. Several reviewed suppressions were removed after production callsites were verified. `Session::turn` is retained because the `sessions` CLI reads it; its stale suppression is removed. This is a count of annotation sites (module and impl scopes can cover multiple items), not 44 distinct defect candidates. The previous 44-item count is stale. The unused provider `Usage::hit_rate` method and stale module-level allowance in `browser/session/types.rs` were also removed.

| Source location | Disposition |
|---|---|
| `glass-browser-core/native_engine/resource_loader.rs`: `NativeSubresourceKind`, `NativeCorsMode` | Removed both suppressions after confirming production callsites across resource-destination and CORS handling. |
| `glass-browser-core/native_engine/dom/tokenizer.rs`: `Doctype` (`not(test)` allowance) | Test-only legacy tokenizer structure; production parsing uses the HTML5 tree builder. The cfg explains why production builds do not construct it. |
| `glass-browser-core/native_engine/javascript/runtime/evaluation.rs`: `NativeJavaScriptRuntime::has_pending_jobs` | Removed as an unused duplicate of the runtime job check; the runtime already drains jobs through the bounded evaluation path. |
| `glass-browser/session/polite.rs`: `PoliteNavigationClassification`, `PoliteNavigationOutcome::classification` | Error classification is used by production site-smoke reporting. The success-outcome helper is now compiled only for tests, where it is exercised. |
| `glass-browser/tui/herdr_graphics.rs`: `HerdrFrame`, `HerdrEvent` (`not(unix)` allowance) | Unix-only terminal graphics transport leaves the shared frame/event types unconstructed on non-Unix targets. The platform cfg is the reason for the allowance. |
| `glass-browser/terminal_graphics/mod.rs`: `render_current_if_new` | Removed after source review confirmed no redraw callsite used it; its only test and `last_rendered_frame` state were removed with it. |
| `glass-dev/development/project.rs`: `confirm_live_update` | HMR evidence reducer requires a browser consumer's newer semantic revision; no governed observation route calls it yet. This is unfinished HMR integration, not a confirmed live-update path. |
| `glass-dev/experiments.rs`: `refresh_changed_files` | Issue #60 F88 P2 reducer awaits the governed experiment observation route. It remains an explicit feature gap and should not be advertised as refreshing evidence. |
| `glass-core/main.rs`: `onboarding` module | Feature-dependent CLI and web-command routes call onboarding; the module-level allowance covers functions unused in some build/test configurations, not a detached module. |
| `glass-core/egress.rs`: `EgressProxy::port` | Direct-port accessor is exercised by tests; production children receive the proxy URL instead. |
| `glass-core/provider/mod.rs`: `Request::model` | Provider request contract retains model identity for adapters that route by per-request model; some current adapters route through their configured endpoint. |
| `glass-core/agent.rs`: `_message_type_is_used` | Test-only import anchor, explicitly inside test support. |
| `glass-core/provider/chain.rs`: `unstick` (`not(test)` allowance) | The live harness rebuilds the chain each turn; reset remains a test/host reuse hook. `serving_model` is production-used by `Provider::model()` and no longer carries a dead-code suppression. |
| `glass-core/queue.rs`: `take_follow_up` (`not(test)` allowance) | Test-only compatibility accessor. `is_empty` and `describe` are used by production queue and serve commands and no longer carry suppressions. |
| `glass-core/readguard.rs`: `mark`, `marked`, `observe` (`not(test)` allowances) | Compatibility/test facades over indexed read tracking; production uses call-indexed variants. |
| `glass-core/session.rs`: `Record::Bash::call_id`, `Session::path`, `Session::turn` | Production code consumes all three: output/session reconciliation reads the call id, Agent/recap/reminder/adviser/ACP/durable-runtime paths use the path, and the `sessions` CLI displays the projected turn count. Their dead-code suppressions were removed. |
| `glass-core/ext/host.rs`: `Extension::{cancel_token,is_live}` | Host/test lifecycle accessors remain intentionally available. `ToolFailure::describe`, `Extension::cancel`, and `Extension::force_stop` have production callers and no longer carry suppressions. |
| `glass-core/ext/mod.rs`: `Registry::{cancel,force_stop,cancel_token,last_shutdown}` | Registry-level lifecycle/reporting endpoints support the same not-yet-wired Agent cancellation and shutdown result path. `last_shutdown` is test-observed; no claim is made that user cancellation currently reaches every extension. |
| `glass-core/ext/protocol.rs`: `CallChain::{path,is_root}` | Test/diagnostic accessors over validated call lineage; the dispatch hot path uses depth/root length directly. |
| `glass-core/sandbox.rs`: `extension_capabilities` | State-aware extension launch-readiness hook remains reserved for host integration. `SessionApprovals`, `harden_command`, `ApprovalDecision`, the status/security summaries, and `inspect_extension_capabilities` are production-used by command, permission, file, and policy paths; their stale suppressions were removed. |
| `glass-core/fs_scan.rs`: `Walk::{dirs,bytes}` | Removed. Neither field was read; aggregate cache byte accounting already lives on `Cache`, and nested directory walks are not implemented. |

This list is a source disposition, not proof that each planned host integration
is complete. The cancellation, approval UI, HMR, experiment refresh, and unused
DTO contract items remain actionable follow-up work.

## Issue #65 workspace unsafe inventory

The current source inventory under `crates/*/src` contains 163 `unsafe {}`
expressions plus four `unsafe extern`/`unsafe impl` declarations across 31 Rust
files. The issue's original 207 count is a lexical token count and is not a
count of unsafe blocks. Review grouped the sites by their owned resource or
foreign-call contract:

| Source family | Reviewed contract and evidence |
|---|---|
| `glass-core` (13 files) | Descriptor-relative filesystem calls use owned descriptors and checked syscall results; pipe/process/sandbox FFI checks return values and closes owned descriptors; signal and environment mutation is scoped to initialization; the extension poll ABI now uses `libc`'s platform declarations. Hook readers set nonblocking mode on their owned pipe read ends on Unix and use `PeekNamedPipe` before reads on Windows; the detached-child regression kills only its own test process. The earlier block review of `sandbox.rs` found and fixed Linux test descriptor leaks with `OwnedFd`. |
| `glass-browser-core` (5 files) | QuickJS values remain tied to the live context and are consumed/freed once; the persistent host-view wrapper's `Send` contract is exclusive ownership plus mutex serialization and clearing before runtime drop; layout scratch initializes every field before `assume_init`; sandbox/job handles close on error and drop; worker termination targets the owned child. |
| `glass-browser` (4 files) | Peer credential buffers are initialized and length-checked; Windows UTF-16 paths remain NUL-terminated for the call; persisted process liveness uses checked positive `pid_t` conversion; daemon shutdown validates positive Unix PIDs and uses Linux pidfds to bind `SIGTERM` to the opened process identity. Commit `f9d78d4d` also parses `lsof` output as a positive `pid_t` before signaling. Focused regressions cover zero, negative, and out-of-range values. |
| `glass-dev` (8 files) | Windows job/process handles have explicit ownership and close paths; process-group calls operate on groups created for the child; peer credentials and private request ownership/mode are checked; UTF-16 path calls retain their buffers; test-only signal probes do not mutate a process. Daemon status now rejects IDs that narrow to a negative `pid_t`, and Linux shutdown signals through a pidfd. |
| `glass-tui` (1 file) | `tcgetattr` initializes `termios` only on success; `assume_init` follows that check; `tcsetattr` receives the initialized structure. |

New deterministic regressions reject `u32::MAX` as a Unix daemon PID in both
daemon implementations and signal an owned short-lived child through the
pidfd-based termination path. The four focused tests pass. This is a source
contract review, not a formal proof or a fuzzed race proof. Linux kernels that
return `ENOSYS` for `pidfd_open` retain the legacy `kill(pid, SIGTERM)` fallback;
other pidfd errors fail closed. pidfd pins the process opened at termination
but does not prove that a reused PID still names the daemon recorded in an old
status file. Process-group cleanup cannot pin a whole group with pidfd, so
process-group identifier reuse after the child has exited also remains an
explicit interleaving boundary. Read-only `kill(pid, 0)` probes can also
observe PID reuse, but they do not signal or mutate the observed process.


### Issue #65 budget-lock poison follow-up (2026-10-26)

`glass-core::budget::Shared` is public, so a panic while another caller holds
the ledger can poison accounting. The provider wrapper and Glass Core TUI
previously unwrapped that lock, including from `Meter::drop`; the persistence
path lock had the same panic risk in the drop path. `budget::lock` now recovers
the ledger as `unknown`, clears the poison marker, and makes a configured cost
limit refuse another request. A poisoned persistence-path lock skips the save
and marks the ledger unknown instead of panicking; interactive budget/status
output reports the amount as unavailable when accounting is unknown. Core path
reads return an error instead of unwrapping a poisoned path mutex. Two
deterministic tests simulate an interrupted ledger update and a poisoned path
lock, verify fail-closed behavior, and verify that drop completes without
panic. The six focused `budget::tests` pass; strict binary Clippy and a debug
Core build pass.
This closes the reviewed budget-lock panic path, not the remaining error,
policy, sandbox, concurrency, and authoring sweeps.

### Issue #65 Core budget-context mutex follow-up (2026-10-27)

The Glass Core REPL's provider factories and session-reset paths also used
unchecked budget-context mutex access, separate from the ledger's recovered
lock. Reads now return a factory error on poison, and reset paths propagate an
error instead of panicking. A focused poisoned-context regression passes;
this is one Core error-propagation fix, not closure of the remaining
interleaving audit.

### Issue #65 event-stream cursor overflow (2026-10-27)

Core accepted the reconnect cursor as a `u64` from the HTTP `Last-Event-ID`
header, then evaluated `cursor + 1` while deciding whether retained events
had a gap. A maximum-valued cursor could overflow in debug builds. Reset
classification now uses saturating addition; a boundary regression covers
`u64::MAX` and ordinary gap/no-gap behavior.

### Issue #65 teammate hub shutdown poison safety (2026-10-27)

`team::Guard::drop` previously unwrapped hub-state, worker-list, and registry
mutexes while tearing down the listener and joining owned workers. A prior
panic while holding one of those locks could make the destructor panic and
skip cleanup. Shutdown now recovers poisoned locks solely for cleanup,
interrupts and joins owned workers, removes the hub, and clears the poison
marker. A deterministic poisoned-state regression verifies dropping the guard
does not panic.

### Issue #65 parallel tool-batch lookup safety (2026-10-27)

The parallel bash executor previously unwrapped the call and approval maps
using the scheduler's index. Those maps are built from the same enumerated
batch, but an internal scheduler/index mismatch would panic a worker instead
of returning a result. The executor now reports a failed tool outcome for a
missing call or approval context. Two focused regressions cover both missing
map entries; this closes that panic path while the broader concurrency and
interleaving review remains open.

### Issue #65 process-ID follow-up (2026-10-26)

Two unchecked Unix narrowing paths are fixed: `browser/persistent.rs` rejects persisted `u32` process IDs that cannot be represented as positive `pid_t` before `kill(pid, 0)`, and `browser/chrome.rs` parses `lsof` output as positive `pid_t` before `SIGTERM`. A follow-up changes `kill_chrome(port)` to inspect only TCP listeners and signal only same-user TCP listeners whose lsof command names match Chrome/Chromium variants; other services, other users, and outgoing clients are excluded, and lsof inspection errors are returned. Its parser regression and the surrounding Chrome unit module pass (18 tests), with strict `glass-browser` library Clippy and formatting. The public function has no in-repository callers; UID and command-name filtering are not an executable-identity pin; a same-user process that deliberately adopts a Chrome command name remains outside this safeguard. Owned sessions should use `ChromeProcess::shutdown`, which signals the exact child Glass launched.

### Issue #65 fail-closed task-risk follow-up (2026-10-26)

`task_compiler::risk_at_least` treated `UnknownRisk` as satisfying any minimum
risk because it ranked above known values. Compiled plans already reject
unknown risk earlier; the comparison now also rejects unknown actual and
required classes directly. The complete task-compiler test module passes 20/20,
including a regression for both unknown sides and known-risk ordering. This
hardens that comparison invariant without closing the remaining authoring or
policy audit.

### Issue #65 malformed MCP frame cleanup follow-up (2026-10-26)

`run_mcp_stream_inner` previously propagated a malformed frame-read error
immediately, before canceling in-flight requests, closing owned sessions, and
releasing a daemon mutation lease. It now retains the framing error while it
performs the normal connection cleanup, then returns the error. A focused test
initializes a daemon client, acquires its lease, sends invalid Content-Length,
and verifies both the framing failure and lease release (1/1). This addresses
the read-error cleanup path only; the broader daemon interleaving audit remains
open.

### Issue #65 direct workflow JSON parser bound (2026-10-27)

The public `WorkflowDefinition::from_json` API previously parsed an arbitrary
source string into a JSON value before validating workflow limits. It now
enforces the same 256 KiB pre-parse bound as `compile_workflow_json`, using one
shared constant. Its boundary regression accepts exactly 256 KiB and rejects
one byte over before parsing; the existing bounded YAML-alias regression also
passes. `from_value` now runs the shared iterative structured-value budget
(32,768 nodes, 128 levels, and 256 KiB accounted size) before typed conversion;
an oversized-array regression verifies it returns before schema handling. The
same audit found
that `CdpBrowserSession::parse_workflow_checkpoint` checked its 8 KiB serialized
checkpoint limit only after JSON deserialization. It now rejects oversized
source before parsing; a regression supplies a syntactically valid checkpoint
prefix followed by more than the limit and verifies the typed size error. The
current string entry points are bounded before parsing: workflow JSON and YAML
at 256 KiB, and persisted workflow checkpoints at 8 KiB. Structured workflow
and reliability `Value` entry points also enforce their own shape budgets.
Other `Value` APIs need separate review. The
public `GlassTask::from_json` path also lacked a pre-parse source cap despite
the existing 256 KiB argument-JSON budget; it now rejects larger source before
deserialization, with exact-limit and one-byte-over boundary coverage. The
Task Protocol parser tests and workflow parser tests cover these direct JSON
entry points; this is not a blanket disposition of the other #65 audit themes.
The corresponding `glass workflow`, `glass task`, and `glass ir` file commands
also used unbounded `read_to_string`, which could allocate the full file before
their parser contracts rejected it. Those paths now use one opened-file reader
that checks metadata size and reads at most limit+1 bytes, catching file growth
between the metadata check and read. Its oversized-file regression verifies the
reader rejects before JSON parsing.

## Issue #65 policy and sandbox path dispositions

The named permission and policy entry points received focused source review
and test runs after the daemon fix. `glass-core/src/sandbox.rs` passed 13 tests,
including Linux read-only Landlock write denial inside and outside the
workspace, pinned-root and pinned-capability descriptor replacement races,
strict extension sidecar validation, and fail-closed egress setup.
`glass-core/src/readguard.rs` passed 17 tests for reader evidence, shell
redirection, output-target exclusions, build-output path matching, canonical
path equivalence, and write blocking. `glass-core/src/policy.rs` passed its
full-value wildcard test. In `glass-browser`, `browser/policy.rs` passed 9
tests for private/local URL rejection, canonical pinned hosts, capability
denials, read-only preflight/redaction, and symlink escape; `task_compiler.rs`
passed 19 tests for unknown-risk rejection, risk-downgrade rejection,
confirmation, named-target non-fallback, form scoping, and postconditions;
`browser/session/workflow.rs` passed 36 tests for bounded inputs, retry
behavior, state transitions, post-dispatch reconciliation, redaction, and
resume/trace validation.

The task risk floor now rejects `UnknownRisk` on either side of its comparison;
unknown risk cannot satisfy an ordinary minimum merely because it ranks above
known categories. A focused regression covers unknown actual/required risk and
both sides of the known-risk ordering. The compiler's existing earlier
unknown-risk rejection remains in place as a separate plan invariant.

These tests disposition the concrete issue entry points; they are not a proof
for all platform implementations or every command parser. The Linux Landlock
cases ran on Linux only. The separate macOS profile, all tool-specific error
propagation, concurrent request/daemon interleavings, and remaining workflow
authoring shapes still need individual evidence before #65 can close.

The current validation host is Ubuntu Linux aarch64, so macOS runtime and
sandbox behavior is explicitly unverified. In particular, source-level review
does not establish the macOS network-egress boundary; that platform acceptance
remains open for a macOS host.

### Issue #65 syntax-check subprocess setup and timeout signaling (2026-10-27)

The Core syntax-only diagnostics subprocess now treats unexpectedly absent
stdin/stdout/stderr pipes as an `unavailable` checker result and kills/reaps
the child before returning. Timeout cleanup converts the child ID to
`libc::pid_t` with checked conversion before signaling its process group, and
falls back to killing the exact child if conversion is unavailable. The
post-wait status path no longer unwraps an optional value. The focused
diagnostics test filter passed five tests, including a checked PID conversion
regression; strict Core binary Clippy and formatting passed. This covers the
named subprocess setup and PID-narrowing paths only. It does not close the
broader #65 process-group, cross-platform, and error-propagation audit.

### Issue #65 semantic intent JSON source bounds (2026-10-27)

The public `SemanticIntentRequest::from_json`,
`SemanticIntentExecutionRequest::from_json`, and `SemanticIntentResult::from_json`
entry points now reject UTF-8 JSON source above 256 KiB before deserialization.
Field and collection validation still runs after parsing. Regressions verify
all three parsers reject oversized JSON before schema errors and that a valid
request padded to exactly the limit is accepted. The focused intent module
passes 11/11 tests; strict `glass-browser` library Clippy passes. This
closes these three direct JSON parser paths, not the complete authoring-input
audit for #65.

### Issue #65 reliability and live-site authoring source bounds (2026-10-27)

The public `ReliabilityScenario::from_json`,
`ReliabilityFixtureManifest::from_json`, and
`ReliabilityReplayBundle::from_json` methods now reject UTF-8 source above
512 KiB before serde parsing. The `smoke-sites` manifest parser has the same
limit. The `certify run` and `smoke-sites` file paths use the bounded opened-
file reader, which checks metadata and reads at most limit+1 bytes so file
growth during a read is rejected before parsing. Direct JSON parser tests
cover oversized input, exact-limit scenario acceptance, and live-site
manifest rejection; the existing bounded-reader regression covers oversized
files. The reliability runner now applies the same opened-file pattern to each
referenced workflow JSON/YAML source at the existing 256 KiB workflow limit;
an oversized-file regression proves the file is rejected before parsing.
`certify plan` now also uses the bounded reader for scenario and fixture
files rather than parsing an unbounded fixture read.
`ReliabilityScenario::from_value` and `ReliabilityReplayBundle::from_value`
also enforce an iterative 32,768-node, 128-level, 512 KiB accounted-value
budget before typed conversion. Focused tests reject oversized arrays and
excessive nesting. `WorkflowDefinition::from_value` enforces the same
iterative node/depth budget with the 256 KiB workflow input ceiling. Other
structured APIs and the remaining parser and error-propagation audit for #65
remain open.

### Issue #65 extraction request JSON source bound (2026-10-27)

`ExtractionRequest::from_json` now rejects source above its 256 KiB contract
budget before serde deserialization. The exact boundary remains accepted, and
the regression verifies an oversized invalid document returns the size error
before a JSON syntax error. Existing request validation still enforces the
tighter node, text, depth, duration, and output budgets. This closes the
direct extraction-request string parser only; transport decoding and other
authoring parsers remain separate audit items.

### Issue #65 semantic observation JSON source bound (2026-10-27)

`SemanticObservation::from_json` now rejects source above 256 KiB before serde
deserialization, matching the bounded structured observation contract. Its
regression covers oversized invalid JSON and exact-limit acceptance. Existing
per-field, record-count, and 64 KiB aggregate structured-data checks still run
after parsing. This closes this direct response-contract parser path only.

### Issue #65 persisted snapshot read bounds (2026-10-27)

Core partial-turn sidecars now read at most 512 KiB plus one byte before JSON
parsing, and streamed tool identifiers are capped at UTF-8 boundaries. The
browser knowledge store and durable workspace snapshots also read only their
configured limit plus one byte before deserialization, returning a capacity or
corruption error without allocating the complete oversized file. Boundary
regressions cover the knowledge store, workspace store, and partial sidecar.
Other persisted-state paths and concurrent replacement scenarios remain in
the #65 source-level acceptance matrix.

### Issue #64 native public video failure recheck (2026-10-27)

The focused local VP8/WebM journey passes with native `videoWidth` and
`videoHeight` assertions at 16×16, in addition to its existing decoded-frame
play/pause/seek/paint checks. A single source-built public MDN run completed
navigation (5.24 s) and observation (2 ms), then lost the content worker 30 ms
into the first media-resource wait, before decode or playback. A direct
evaluation probe also lost the worker before returning its first query result.
The worker emitted only the privacy-safe static diagnostic class `worker`, so
the underlying failure is not identified and no public video pass is claimed.
The redacted report and owner-only trace are
`/tmp/glass-native-mdn-video-sourcecheck-20261027.json` and its adjacent
`.artifacts/` directory. This remains an open native media acceptance failure.
An additional source-built batch with bounded worker diagnostics reports
operation class `content_process_script` and reason class `other`. The worker
logs only static allowlisted categories, never the operation text, error
reason, URL, or page content. This narrows the failed boundary but does not
identify a cause or count as a media pass; the owner-only records are in
`/tmp/glass-mdn-video-reasonclass-qs3za94c/`.
Review of that attempt's bounded step error identifies a content-process pipe
closure and exit status 1 during the script exchange. The safe reason classifier
now maps those known generic pipe messages to `content_process_pipe_closed`
(and truncated frames to `content_process_truncated_frame`); its regression
uses synthetic private suffixes to verify the emitted category remains static.
The captured worker stderr still contains the earlier `other` category because
no additional public run was made. Why the content process exits remains
unidentified, and the public video journey remains failed before decode.

## Verification in this working tree

Passed so far:

- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
- Focused Code preview and Git summary unit tests

Still open or limited:

- Amazon native navigation now succeeds after avoiding repeated JS bridge rebuilds for unobserved lifecycle events. The first bridge initialization still takes about 9 seconds in the measured run, and this does not prove broad public-site parity or anti-bot behavior.
- Real-provider post-usage waiting remains unresolved in issue #64. Privacy-safe diagnostics now bracket Core frame reads and queueing, consumer dequeue, host-tool dispatch, worker critical-event handoff, and durable settlement persistence/snapshot publication; they record static frame classes and hashed correlation IDs, not prompt or tool content. A deterministic filesystem-lock fixture demonstrates one possible UI delay: applying Core's settled event waits for durable run-phase classification before publishing the Idle snapshot. Thread-registry exclusive lock acquisition is now bounded to two seconds with a 20 ms retry interval and an explicit timeout error; a held-lock regression verifies it returns at the bound, while the existing 180 ms settlement-lock fixture confirms the UI still publishes Idle only after persistence succeeds. This prevents indefinite waits on another registry writer without inventing settlement. It does not establish that lock contention caused the reported live wait; no provider retries were used.
- The installed-launcher fake-Core browser PTY emitted seven native host-tool requests under the new trace: Core reader queue sends stayed below 0.3 ms, while full host dispatch spans topped out at 9.74 s for local fixture calls. This validates that the trace distinguishes transport handoff from work performed by the host; it does not reproduce the configured-provider pre-tool or post-usage wait.
- Core provider diagnostics additionally emit privacy-safe request-preparation begin/complete, provider request begin, first consumer-observed stream event, stream completion, tool validation, intent recording, tool-batch begin/complete, and request completion phases to the mode-0600 `*.provider.jsonl` trace selected by `GLASS_RUNTIME_DIAGNOSTIC_TRACE`. Phases share a monotonic process-local trace ID and hashed runtime-turn correlation; host wait records also carry a hashed request-ID correlation. First-stream-event timing includes bounded-channel queue delay but is captured before host input pumping. Request workflows are labeled as agent, compaction, or cache warmer. Matched pre-tool hook execution now has begin/complete phases, and request-summary append plus last-request snapshot persistence have separate begin/complete/error phases after provider completion. The trace contains no prompt, tool arguments, provider text, session ID, or credentials. The focused Core runtime suite passes 12/12, including bounded backpressure recovery and matching cancellation acknowledgment without a provider. The provider reader’s 64-item stream queue now uses cancellation-aware sends: a full queue still applies backpressure, checks cancellation every 10 ms, and emits privacy-safe correlated queue-wait completion/cancellation phases. A deterministic one-slot fixture verifies wait, delivery after drain, and producer exit within one second on cancellation (1/1), without a provider call. This prevents a dropped consumer from leaving the provider reader blocked forever in SyncSender::send; it does not reproduce the reported real-provider wait. A deterministic Linux hook regression reproduced a second pre-tool stall class: after the three-second hook deadline killed its process group, an escaped `setsid` descendant retained stdout/stderr, so the parent blocked joining drain threads. Pipe drains are now cancellable and nonblocking on Unix, and poll available bytes with `PeekNamedPipe` on Windows; the hook timeout path cancels drains before joining. The regression exercises a 150 ms deadline and returns in under two seconds while an escaped child remains alive, which its test cleanup then terminates. All 10 hook tests pass and strict Core binary Clippy passes. This confirms and fixes the escaped-descendant cleanup hang when a hook is configured; it does not reproduce the reported provider wait or establish that this was the exact live incident. Provider wait evidence remains incomplete without a configured-provider trace.

Validated in this working tree:

- `cargo test --offline --locked -p glass-dev --lib`: 694 passed, 0 failed; `--bin glass --test cli_forward`: 2 binary tests and 2 CLI forwarding integration tests passed. The current `development_runtime` integration file passes 6/6, including the Glass-only help pruning and daemon-error presentation regressions.
- Focused `tui_pty` runs through `target/debug/glass`: fresh startup/first send with no implicit file context, slash discovery and model selection, taskGraph refresh, named Terminal process start/exit/log retrieval, browser host-router recovery/action, literal first typing, and Agent/Browser/Code/Terminal/Tasks/Git/Debug navigation passed. The navigation journey verifies reachability, not every mutation on every surface.
- `5cec11d8` / `e5b5e2c7`: focused local diagnostics redaction/rotation/reader, provider schema fingerprint, Core binary resolver, expected-stop, unexpected-disconnect, and native browser lifecycle regressions passed. Strict scoped Clippy passed for Glass Browser lib, Core binary, and Glass Dev lib/bins. Fresh `glass`, `glass-core`, `glass-browser`, and native content worker binaries were built; direct Glass selected adjacent monorepo Core 0.3.14 and negotiated runtime protocol v2/mixed tools.
- `c5c80645` / `038bcc9e`: a poisoned MCP cancellation-map regression verifies that cancellation and task cleanup recover without panic. Duplicate notifications remain response-free, internal fallback IDs wrap with active-map collision checks, and impossible session-initialization / native elicitation states now return safely rather than panic. The focused test, `cargo check -p glass-browser --lib --locked`, and strict Glass Browser Clippy passed. The canonical MCP source audit is still open for the separately listed daemon interleavings, policy paths, and remaining production panic/error sites.
- A configured resident-provider Glass TUI smoke used one prompt to call `glass.browser.state`, `web_fetch`, and `web_search` (3 calls, 0 transport retries); fetch returned HTTP 200 and search returned 8 sources. A separate read-only native incognito PTY journey completed browser start, `example.com` navigation, semantic observation of `Example Domain` at revision 2, and explicit stop without a Recovery modal. These are one-run acceptance results, not general provider or website parity.
- `cargo test --offline --locked -p glass-core --bin glass-core web_tools::tests`: 9 passed.
- After the MCP lifecycle changes, `target/debug/glass` and `target/debug/glass-native-content-worker` were rebuilt separately to avoid the workspace's duplicate `glass-browser` binary name. Direct and installed wrapper versions both report Glass 0.3.14.
- Native CSS rule-limit, 4,096-rule bounded/computable, and navigation-deadline tests passed with the `native-engine` feature.
- The reported quoted `text/html; charset="utf-8"` failure now has a response-header-to-document regression: quoted label extraction is asserted and malformed UTF-8 bytes are replaced per HTML decoding instead of being misreported as an unsupported charset. The focused `glass-browser-core` native-engine test passes. The bounded `glass logs` journal is empty for the captured session, while its durable resident transcript remains in the separate session store; the original session was not replayed.
- Documentation inventory (1,822 pages/catalog entries), depth, coverage, shortcuts, wiki tests (7), formatting, and diff checks passed. Strict Clippy passed for `glass-dev` lib/bins/tests, `glass-core`, `glass-browser`, and `glass-browser-core` with the native engine feature. Rebuilt Glass, Core, browser, and native content worker; `~/.local/bin/glass --version` and `target/debug/glass-core --version` report 0.3.14. The launcher wrapper points at the rebuilt workspace binaries.

