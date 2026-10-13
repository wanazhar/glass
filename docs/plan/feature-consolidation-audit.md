# Feature consolidation audit

Date: 2026-10-12. Scope: current source, 79 builtin slash entries, CLI discovery,
viewers, project knowledge, navigation and execution boundaries. This is a design
audit; no runtime changes or performance claims are implied.

## Decision

Three primary navigation entries: **Conversations**, **Work**, **Settings**.
Chat is the default surface, not a fourth destination. `/` and Ctrl+P open one
searchable action palette. With an empty query it shows only these three entries;
with a query it searches all supported actions by intent, label and legacy alias.
The visible query hint says "Search any action…" with an available-action count;
this discloses direct actions without adding more permanent menu rows. A browse
mode inside the same palette must expose the complete catalogue without requiring
users to already know command names.
Existing exact slash invocations keep working. This is not a proposal to remove
79 capabilities or replace them with a model interpreting every UI action.

The reduction must remove duplicate implementations, not merely put the old
menus under three new menus. Opening an item shows its details immediately, with
object-specific actions. Use one level of list -> detail, not list -> action
picker -> another list -> reader -> another action picker.

## What the source actually contains

1. Three launchers: `/dashboard` routes twelve workflow choices; `/hub` routes
   six; `/workspace` opens the task/evidence board. They overlap but have distinct
   content. Retire the first two menu implementations; expose their content in
   the three actual destinations.
2. Three conversation browser implementations: `polish::browser` calls
   `polish::sessions`, which opens every candidate JSONL before filtering by
   workspace. `resume_picker::index` reads metadata first, with a bounded 64 KiB JSONL
   prefix fallback for untitled sessions. `navigation::view` adds branch/unread
   receipts; its 512-row output limit does not bound the initial whole-directory
   indexing/sorting work. Unread counts represent recorded events, not necessarily
   messages. Consolidate
   onto a metadata-first index plus one browser and explicit paging/search.
   Preserve direct resume of old IDs, current-writer handling and read-only
   previews. Ordinary resume, browse mode and /sessions currently have different
   selection/preview behavior; define one predictable open/preview contract.
3. Three output browsers: `workflows::responses` enumerates assistant responses
   and code blocks; `structured::outputs` enumerates responses/tool results and
   parses JSON after selection (latest 100 raw records of relevant kinds);
   `polish::details` supplies tool commands, truncation/spill
   access and exact copying. Share selection, detail rendering and copy/range
   actions; retain formatted-vs-raw JSON toggling and every exact-copy/range action
   in one item inspector.
4. Multiple transcript controllers: inline `reader::Reader` and overlay
   `live::Live` each own a Terminal and Transcript; Live additionally parks
   separate transcript states for tools/thinking. `Painter` owns the streaming
   presentation. They already share some markdown/tool formatting. This is
   controller/state duplication, not proof every renderer is unrelated. Make
   one controller own selection, expansion, search, follow and viewport state;
   preserve the cheap streaming path until parity/performance gates pass.
5. Two project-knowledge stores: `project_memory` stores opt-in reviewed facts;
   `neuralnets` stores pending/confirmed/forgotten attributed learning. Both are
   appended by `polish::prompt_context`. The same fact can therefore be supplied
   twice. Consolidate storage/retrieval with explicit migration, not an extra
   combined screen over both stores.
6. Independent action metadata: main's slash catalogue/dispatch, help's CLI
   catalogue, completions' command list and guidance recipes have separate
   ownership. Current palette search considers command names, only the first
   query token, not descriptions or intent tags. Grouping alone does not fix
   discovery. Share action IDs/effects/availability/aliases across surfaces;
   maintain CLI-specific argument contracts and curated lazy model recipes.
7. Activity and attention already join several sources. Attention already uses
   recovery classifications; usage already uses request information. Do not
   rebuild them as another event store. Add a shared typed event projection and
   item inspectors over existing session/request/job/team records.
8. Exact aliases: compact/compaction and security/permissions share dispatch.
   `checkpoints` and bare `restore` share a picker but choose different actions:
   they are related entry points, not equivalent commands.

9. Undiscovered dispatch paths: `/tools` changes card presentation and `/prewalk`
   handles model handoff, but neither appears among the 79 palette entries. Their
   capabilities must migrate too. A registry needs explicit internal/hidden
   metadata rather than accidentally leaving handlers undocumented.
10. `/details` is inconsistent by execution phase: idle invokes
    `polish::details`, while the busy input handler opens the inline Reader from
    Live snapshots. The busy palette only directly supports queue, agents,
    details, thinking and help; most other actions say "After this turn". This
    is a dispatch/availability split to fix, not just duplicate labels. The new
    recap also has a themed notice path and a separate generic transcript entry;
    include it in shared presentation parity rather than add another renderer.

## Three destinations

### Conversations

A searchable metadata-first conversation list. Resume directly; show a selected
conversation's recap/preview, branch lineage and last status in place. Current
conversation actions: rename, new, branch, export, portable handoff. Read a
message -> quote/copy/branch/context-edit. Read the context strip -> attachments,
loaded instructions/knowledge, request history, compact. These are contextual
actions, not additional permanent navigation destinations.

### Work

A typed, searchable list defaulting to current-project work objects with lightweight filters
(Changes, Runs, Tasks, Evidence). Open a row for details. A running process exposes
output/input/stop; a teammate exposes messages/transcript/follow-up; a file exposes
hunks/review/stage; an error exposes its safe recovery action and diagnostics.
Task rows expose dependencies, ownership, findings, artifacts and handoffs.
Attention is a filter/badge over this list, not another inbox requiring a scan.
Retain a visible All projects scope and running-work indicator, so project changes
do not hide other sessions or jobs. Findings, handoff questions, provider/tool
failures and teammate events must all have resolvable rows before replacing
Attention; historical/unresolved attention is distinct from currently running work.

Preserve object scope visibly: session, workspace, agent and process owner IDs
must not become interchangeable because their rows share a screen. Stable typed
IDs prevent a live update from changing the selected target under the user.

### Settings

Searchable settings rows for model/account, appearance/input, knowledge/skills,
integrations, permissions and environment. Selecting a row edits or inspects it
directly. Effective value and provenance remain visible. Help and model guidance
are available through palette search and inline explanation. Doctor checks appear
at the affected connection/environment row; explicit probes retain their label.
A selected request/failure still opens the same relevant diagnostics inspector.

## All 79 slash entries accounted for

A command below is an alias/action, not necessarily a top-level menu entry.

| Current commands | Canonical home/action |
| --- | --- |
| sessions, branches, name, clear, branch | Conversations: resume, lineage, rename, new, fork |
| history, handoff, recap, export | Selected conversation: boundaries, transfer, recap, export |
| search, quote, prompts, editor, btw | Chat/message/composer actions and global intent search |
| context, attach, forget, unforget, rewrite, compact, compaction | Context inspector and selected-message context actions |
| outputs, responses, details, thinking | Shared transcript/item inspector, typed filters |
| review, turns, diff, git, pr | Work: change overview, intervals, hunks, staging, PR |
| code-review, advisor | Work: explicit model-assisted reviews with separate accounting |
| checkpoint, checkpoints, undo, restore, rewind | Conversation or change item: save/restore boundary, explicit affected scope |
| activity, attention, agents, jobs, job | Work: unified object list, attention filter, typed inspectors |
| bg, loop, send | Work: start run/schedule, message selected session/agent |
| queue, queue-run, plan, constraints | Task/composer: follow-ups, run queue, propose plan, task reminders |
| workspace, dashboard, hub | Replace launchers with Conversations / Work / Settings |
| status, requests, usage, budget | Status/context strip; selected request accounting and spending setting |
| retry, recovery, diagnostics, doctor | Selected failure/setup row: recovery, inspect, export, check |
| settings, profiles, theme | Settings: effective values, profiles, appearance |
| model, login, logout | Model/account setting and direct status-strip action |
| neuralnets, skills, capture, skill-doctor, init | Knowledge/skills settings; selected workflow capture; project instruction draft |
| mods, extensions, security, permissions | Settings: integrations and permissions, distinct runtime types |
| help, keys, guidance, redraw | Global intent search, inline reference, support repaint action |

Additional supported slash actions outside the palette: `tools` becomes the
selected tool/transcript presentation setting; `prewalk` becomes a model-transition
handoff action with its existing context/approval contract. The 79-entry table
counts catalogued entries, not every dispatch path.

CLI-only capabilities remain accessible for automation (native file operations,
transactions, result contracts, worktrees, memory migration, plugins/MCP,
serve/attach, ACP and companion). They do not each need a slash landing entry.
CLI help/completions and model discovery should project the same capabilities
with their own argument schemas, not pretend every slash action is a shell tool.

## Actual implementation mergers, ordered by value

1. **Action registry:** stable ID; human label and synonyms; aliases; input schema;
   execution scope; effects (local/model/network/files/auth); availability during
   turns; required permission; handler. Presentation selects an action ID,
   not a synthetic prompt or fragile chain of slash substitutions. No automatic
   model request to search or classify actions. Surface-specific contracts stay
   explicit; unsupported CLI/UI forms are not generated.
2. **Conversation index/browser:** reuse the metadata-first index, add branch,
   unread and status receipts, lazy preview. Retire full-log enumeration and the
   parallel UI browsers after parity. Explicit pagination/search for older threads.
3. **Item inspector + transcript controller:** shared record identity, rich
   markdown/JSON/diff views, exact-copy actions, lazy spill ranges, one search,
   selection and expansion model. Remove the three output menus after parity.
   Keep one terminal surface owner; don't add another redraw/overlay controller.
4. **Work projection:** adapt current activity/attention/board sources using typed
   IDs and on-demand details. No new history database. Optional roles/tasks stay
   usable without requiring the user to create a board for normal chat.
5. **Knowledge store:** migrate legacy reviewed facts into neuralnets with source,
   review time and original key. Preserve key-update behavior (legacy keys are
   overwritten, neuralnets entries have revision/lifecycle semantics) and
   original source-store inclusion policy. Preserve enabled/disabled state, pending review,
   forgotten status and read-only rules. Add per-source/context inclusion controls
   before migration; confirmed must not automatically mean always included.
   Legacy retrieval allows up to 16 facts/8 KiB; neuralnets currently permits up
   to 8 entries/2 KiB. Preserve effective inclusion/bounds or explicitly expose
   the change for review; a migration must not silently drop active facts.
   Deduplicate only exact normalized text+provenance matches; conflicting facts
   stay visible for review. Keep skills as standard files and guidance as product
   documentation; they may share discovery, not trust or storage semantics.
6. **Execution/review lifecycle:** inspect common Agent setup/usage/cancellation
   across planning, code-review, advisor, one-shot delegates and persistent
   teammates. Share setup/helpers where contracts match. Do not claim they are
   duplicate engines or route every review through a writing teammate. A review
   is a task purpose; persistence, context, workspace and permission are options
   with explicit defaults. Unification here needs a separate behavioral audit.

## Boundaries that must survive

- Local review summary vs model-assisted code review/advisor: costs and writes
  differ; browsing a diff must never silently start paid inference.
- Context checkpoint/rewind vs workspace checkpoint/restore: different state and
  ID namespaces; branch/context hiding does not mean files were undone.
- Recap vs compaction vs handoff: display, model context, transfer. Share bounded
  extraction utilities only when their visibility and trust rules match.
- Permissions, global denials, admin requirements and sandbox: one explanation
  UI, separate enforcement layers; consolidation cannot relax precedence.
- Jobs, schedules, teammates and foreground turns: one object list, separate
  lifecycle/ownership and cancellation rules. Opening an inspector never owns
  or terminates the underlying process.
- Planning and execution remain explicit. A user opening a saved plan has not
  authorized execution; configured approval policies remain in force.
- Pending knowledge never becomes trusted; disabled knowledge stays disabled.
- Native read/edit/write keep direct schemas and CAS/read guards. They already
  share the file engine with `gc file`; don't hide them behind a large router.

## Interaction and performance acceptance criteria

- Empty slash palette has three destinations; search reaches every legacy action
  using plain terms such as "copy code", "why failed", "change model", "undo file".
  Rank exact legacy aliases first; show one canonical result per action. Bounded
  result lists and cheap deterministic matching, no provider or filesystem scan
  on every keystroke. Zero automatic inference to use navigation.
- Common actions take one selection after reaching their object; direct aliases
  remain single-step shortcuts. Deep modal stacks are eliminated, not relocated.
- Inspectors remain usable during a running turn; mutation/model/auth effects
  have explicit availability. Background status updates preserve selected ID,
  query, draft, scroll anchor, unread position and expansion state.
- Resume startup scans metadata, not all transcripts; selected preview/ranges are
  bounded. Layout caches key on content revision, width and expansion state;
  closed views perform no parsing/redraw. Avoid rebuilding unchanged history.
- Theme, status tones, headers, spacing, truncation labels and recovery actions
  are the same during streaming, normal history and scroll views. Test semantic
  styles and exact text, not just screenshots or whether a modal opens.
- Release PTYs: 20x8, 40x24, 80x30, large resize; keyboard and wheel; SSH escape;
  active-turn action access, context updates, errors, spills, non-Git directories,
  aliases and drafts; unchanged existing latency/RSS gates. No timing improvement
  is claimed before measuring the implementation.
- At 20x8 retain the query, selected action identity and close/back cue. Primary
  rows use short labels; selected hints occupy a separate available line. Long
  descriptions cannot be the only distinction between clipped commands.
- Keep Ctrl+P toggle, slash on empty composer, exact command+arguments, Tab and
  all existing navigation shortcuts. Enter sends/steers when the palette is
  closed; a modal cannot strand the composer or prevent interrupting a turn.
- Knowledge migration is reversible and idempotent; preserve original files,
  validate resulting inclusion policy and inspect the first provider payload.
- Generated help, completions and model guidance cannot drift from actual action
  handlers; effects and availability are asserted in registry contract tests.

## Delivery strategy

Implement the registry and metadata browser first, then item/controller parity,
then replace launchers/output menus and unify Work. Migrate knowledge only after
inclusion controls exist. Preserve exact-command compatibility throughout. Remove
superseded UI/controller code after parity, rather than leave a second system
running behind aliases. Existing saved sessions and automation remain readable.

## Everyday interaction paths

| Intent | Direct path after consolidation |
| --- | --- |
| Resume | `/` -> Conversations -> choose thread; exact `gc resume ID` remains |
| New / rename / branch | Conversation header actions; also direct palette search |
| Change model | Current-model status action -> provider/model picker |
| Search this chat | Existing Ctrl+F; palette search "search conversation" |
| Copy reply/code | Select message/code -> Copy; same inspector as history |
| Understand failure | Select error -> cause/attempts/safe actions in place |
| Retry | Selected failure Retry; no hidden inference from opening the item |
| Review changes | Work -> selected change opens hunks, local summary and stage actions |
| Request code review | Explicit action labelled model request; accounting retained |
| Inspect/stop run | Select activity badge or Work run -> output and owned stop action |
| Inspect/follow up teammate | Select teammate -> live transcript and Send |
| Inspect loaded data | Select context/status strip -> attributed sources and attachments |

Inspection itself must not invoke a model. Contextual affordances are keyboard
accessible and labelled; no mouse-only requirement. Palette intent search routes
directly to the object/setting inspector instead of forcing a detour through a
landing destination. Existing aliases remain useful shortcuts, not root rows.

## Audit review

Two independent GPT-6-Luna medium reviewers checked the source and proposal.
Corrections incorporated: metadata index's transcript-prefix fallback and unbounded
initial scan, recorded-event unread semantics, legacy memory key/inclusion/bounds,
formatted/raw JSON preservation, global work visibility, action-search disclosure,
20x8 interaction details, phase-specific access and explicit everyday paths.
All 79 catalogue commands are mapped exactly once; `/tools` and `/prewalk` are
additionally accounted for. No runtime implementation or tests were performed in
this audit.

## Implementation checkpoint

Implemented in this change:
- Shared slash registry with canonical identities, aliases, effects, grouping,
  busy availability, human guidance references, intent search and all-action browse.
- Three primary palette entries. Work/Settings route through the registry;
  duplicate dashboard/hub launcher implementations removed. Task board is `/tasks`.
- One output list and item inspector for outputs/replies/tools, bounded selected
  previews, formatted/raw toggle, exact clipboard actions and saved spill ranges.
- Markdown modal and rich item inspection share one cached native renderer.
- Session browser indexes metadata before loading only the selected transcript.

Still pending, and not claimed complete:
- Fully shared conversation chooser/controller with resume and branch navigation.
- Lifecycle behavioral audit before sharing setup paths.

These require behavior and migration parity checks; moving launchers alone does
not complete those implementation consolidations.

### Verification of this checkpoint

- One GPT-6-Luna medium reviewer; bounded code extraction finding fixed.
- Workspace tests: 844 passed; one intentionally ignored doctest.
- Strict Clippy: `cargo clippy --workspace --all-targets -- -D warnings` passed.
- Actual `target/release/glass-core`: consolidation PTYs passed at 120x36,
  40x24 and 20x8; exact command OSC52 bytes verified; local navigation creates
  no provider requests. Reader regression PTYs passed at 40x24, 20x12, 20x8.
- Offline performance gate passed on the release binary: worst median prompt
  startup 6.22 ms, key visibility 0.107 ms, redraw 0.319 ms, 12,000-record reader
  frame 117.05 ms. These are local measurements, not cross-machine guarantees.
- No PATH installation or remote publication performed.

### Continuation: shared owners

Implemented the shared inline/fullscreen/reasoning reader, typed Work projection
with focused Activity/Attention views, and native reviewed-memory migration into
neuralnets. Migration preserves opt-in state, keys, sources, dates and legacy
backup files; reads do not migrate data. Both retrieval budgets count escaped
wire bytes. General tasks and teammates appear in Work, while Attention stays
focused. Interrupted tool cards use canonical turn/compaction pairing.

One Luna medium reviewer checked implementation and fixes. Final release evidence
is recorded below after verification. Conversation chooser and lifecycle audit
remain explicitly pending above.

#### Continuation verification

- 848 workspace tests passed, one intentionally ignored doctest; serial execution
  avoids a parallel session-lock flake observed during the first test run.
- Strict Clippy passed on the final source; optimized release build completed.
- Final release binary passed consolidation navigation, live-turn reader (five
  sizes), reader/search/jobs/diff (including 20x8), neuralnets lifecycle and native
  provider-wire PTYs, and explicit knowledge migration/backup/privacy checks.
- Offline performance gate passed: worst median prompt startup 6.29 ms, key
  visibility 0.090 ms, redraw 0.311 ms, 12,000-record reader frame 126.08 ms.
  Local measurements only; report `/tmp/gc-consolidation2-performance.json`.
- No PATH installation, service interruption or remote publication performed.
