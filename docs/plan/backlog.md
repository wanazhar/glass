# Backlog

## Issue #40: reconcile native-engine slice evidence

The latest locally completed parser/script slice is `native-engine-browser-722`.
Slices 709–710 preserve request-URL module identity (including query and
fragment) separately from response-URL base resolution across page, dedicated,
shared, and service-worker module graphs. See the task records for scope and
local evidence. Slice 711 adds safe prefetch discovery for static string
concatenations in dynamic imports and corrects rooted-file module-script URL
resolution. Slice 712 prevents member methods named `import` from being
misclassified as dynamic `ImportCall`s and avoids speculative module fetches.
It also corrects the profile: HTML import maps are Document-scoped, not a
worker-module capability. Slice 713 adds runtime-valued computed imports for
HTTP(S) page classic/module scripts through the asynchronous host fetch path,
using active response bases, Document import-map scopes, and nested computed
imports. Slices 714–715 add runtime-valued imports for dedicated and shared
classic/module workers. Slice 716 rewrites computed imports in statically
preloaded classic `importScripts()` sources against each source's final
response URL, before graph concatenation. Redirected dedicated imports, nested
module imports, SharedWorker port settlement, and local fixtures pass. Slice
717 settles runtime-valued imports from initial rooted-file classic/module
scripts and dynamically attached classic/module scripts, honors the installed
Document import map, retains existing module identities within graph bounds,
and rejects out-of-root requests through the import promise. Its focused tests
cover nested imports, duplicate module identity, and the file-root boundary.
Slice 718 registers parser-sourced Document maps in source order on initial
rooted-file navigation and applies them to mapped static and runtime module
imports through the configured-root loader. The focused fixture covers an
initial classic dynamic import, mapped static and nested module loading, a
later accepted mapping, a malformed map that leaves prior mappings intact,
conflict preservation, and out-of-root rejection. The scoped check and focused
tests passed locally. Slice 719 now enforces the HTML Standard's Service Worker
dynamic-import `TypeError` rejection across classic `importScripts()` sources
and module entry/static dependencies. It rejects imports even when their
target is already in the static module graph and avoids requests for uncached
dynamic targets; process-backed coverage asserts the exact request set.
Import options, broader file-origin propagation, and full module scheduling
remain open. See `tasks/native-engine-browser-719.md`.
Slice 720 implements static JSON imports across page, Worker, Service Worker,
and configured-root file module graphs. JSON MIME validation and default-only
exports are enforced, and request URL plus module type remains the graph
identity even when a JavaScript import uses the same URL. The QuickJS
compatibility rewrite separates typed static requests before parse and restores
the original specifier before import-map resolution. The scoped check, focused
unit tests, process-backed JSON tests, and documentation gates passed locally;
remote CI was not run. Dynamic import options, broader file-origin propagation,
and full module scheduling remain open. See
`tasks/native-engine-browser-720.md`.
Slice 721 replaces literal dynamic-import prefetch with invocation-driven
loading and implements dynamic JSON options across page, dedicated/shared
Worker, classic `importScripts()` dependency, and configured-root file owners.
It verifies that unexecuted/invalid calls issue no request, preserves
specifier/options conversion order, and keeps Service Worker rejection
fetch-free. The scoped target check, seven unit tests, six process-backed
regressions, formatting, and whitespace checks passed locally; remote CI was
not run. Other module types, broader file-origin propagation, and full module
scheduling remain open. See `tasks/native-engine-browser-721.md`.
Slice 722 installs parser-sourced head CSP meta policies before rooted-file
Document scripts load. Inline scripts, classic/module roots, static
dependencies, and invoked dynamic imports obey script-src-elem/script-src/
default-src fallback. File `'self'` is scoped to the most-specific admitted
root, explicit `file:` sources remain root-bounded, same-URL reload replaces
meta policy, and denied target bytes are not read. The focused test and scoped
check passed. Of 16 CSP unit and 19 CSP integration tests, 16 and 18 passed;
one report-only network report-URI test timed out, including on standalone
replay, and is outside this slice. Maintainer documentation gates passed;
remote CI was not run. Other file CSP resource classes, report-only file policy,
and full CSP conformance remain open. See `tasks/native-engine-browser-722.md`.

### Slice 723 completed locally: rooted-file stylesheet CSP

Enforced parser-meta CSP for initial and dynamically attached/updated file
stylesheet links and recursive local CSS imports. Style-source fallback,
configured-root `'self'`, link nonce metadata, SRI and resource bounds are
preserved; disallowed stylesheet bytes are rejected before read. The loader
tests pass 2/2, the new process-backed tests pass 2/2, and the existing
unrooted-import and rooted stylesheet regressions pass 1/1 each. At the end of
slice 723, inline styles, blob/data stylesheets, CSS image/font loads,
report-only file policies, and complete CSP conformance remained separate;
slice 724 closes the inline-style gap. Remote CI was not run. See
`tasks/native-engine-browser-723.md`.

### Slice 724 completed locally: rooted-file inline style CSP

Inline style blocks and attributes in configured-root file Documents now use
their CSP directive fallback chains and shared nonce/hash checks. Parser
content, initial-script mutations, dynamically attached scripts, and committed
DOM style changes are rechecked. Changed style blocks rebuild the authorized
CSS sources; style-attribute changes invalidate computed style. The focused
loader and process-backed tests passed, as did the existing HTTP inline-CSP and
rooted-file stylesheet regressions. Release-documentation truth, documentation
depth, shortcut, formatting, and whitespace checks passed. Local links passed;
the full CLI inventory check lacked `target/debug/glass`. Remote CI was not run.
See `tasks/native-engine-browser-724.md`.

### Follow-up observed during slice 722

`native_content_process_delivers_report_only_csp_report_uri_network_reports`
timed out waiting for its loopback report POST, both in the 19-test CSP batch
and when replayed alone. Its root cause is unestablished; do not attribute it
to slice 722 or call it an established pre-existing failure. It is outside the
rooted-file enforced-script contract and remains an issue #40 report-delivery
follow-up.
At the 715 checkpoint, computed imports in classic `importScripts()`
dependencies remained open. Slice 714 adds runtime-valued computed imports in
dedicated classic and module Workers through the worker-owned fetch queue,
including nested module imports and response-base resolution. Slice 715 adds
classic and module SharedWorkers, including nested graphs, rejected fetches,
and settlement delivered through connected ports. At that checkpoint,
rooted-file page imports, computed imports in classic `importScripts()`
dependencies, import options/attributes, and full module scheduling remained
open. Slice 716 closes the classic imported-script response-base gap; see
`tasks/native-engine-browser-716.md` for current evidence.
Detailed records 650–671 remain missing and must be recovered from authoritative
commits rather than inferred from summary prose. Keep issue #40 as the remote
status mirror and refresh its current-checkout summary after each local slice
checkpoint; the local branch remains unpushed.
Remote CI, release, registry, and cross-platform promotion evidence are not
claimed.

## Page JavaScript import maps

Slices `native-engine-browser-704` and `native-engine-browser-705` connect
bounded inline import-map `imports` and `scopes` to page module graph prefetch
and QuickJS resolution. Slice `native-engine-browser-706` adds URL-keyed
integrity metadata enforcement for descendant module responses and default
CORS for external module roots and descendants. Parser normalization preserves
JSON key order: later normalized collisions replace earlier entries within one
map, while map merges preserve older entries. Exact and prefix mappings,
most-specific nested scope selection, less-specific/global fallback, nested
literal-dynamic imports, and the two-origin SRI/CORS integration pass locally.
Slice `native-engine-browser-707` completes source-order map processing and
resolved-specifier locks, bounded at 1,024 successful resolutions and shared
with QuickJS. The locked check, 14 resolver/module unit tests, 11
process-backed module tests, formatting, and four documentation gates passed
locally; remote CI was not run. At the 713 checkpoint, runtime-valued computed
imports had just been added for page classic/module scripts through the
asynchronous host fetch path. Slices 714–715 now cover dedicated and shared
classic/module workers. Rooted-file page and `importScripts()`-dependency
coverage, import options, broader file-origin coverage, and full module
scheduling remain separate Core Web Profile gates. See
`tasks/native-engine-browser-704.md` and `tasks/native-engine-browser-705.md`.
See `tasks/native-engine-browser-706.md` and
`tasks/native-engine-browser-707.md` for the latest completed verification.
Slice `native-engine-browser-708` routes dynamically attached import maps and
inline module roots through existing runtime-map, CSP, and dependency-loader
paths, preserving attachment order even when creation order differs. The
opposite-creation/attachment-order test, 16 library tests, 4 process-backed
tests, rooted-file fixture, scoped check, formatting, and all four docs gates
passed across 1,336 Markdown files with zero stale-current-claim failures;
remote CI was not run. Computed dynamic imports, worker maps
and broader worker/file-origin
propagation, module fragment identity, and complete module scheduling remain
separate gates.
Slice `native-engine-browser-709` completes the main-page module-identity/base
split for HTTP and rooted-file graphs. Request URLs, including queries and
fragments, are module-source and deduplication keys; the final response URL is
used to resolve descendants after redirects. Import-map targets and referrer
records preserve fragments. The scoped check, 23 library/19 integration
fragment tests, and 17 library/4 integration import-map tests passed locally.
At the 709 checkpoint, worker module identity, computed dynamic imports,
complete scheduling, and remote CI remained open.
Slice `native-engine-browser-710` completes worker module graph identity/base
handling. Focused process-backed redirect, fragment/query, dedicated/shared,
service-worker, and rooted-file tests passed with the scoped check and all local
documentation gates. At the 710 checkpoint, computed dynamic imports and
complete module scheduling remained open; remote CI was not run. See
`tasks/native-engine-browser-710.md`.
Slice `native-engine-browser-711` admits fully static quoted-string
concatenations in dynamic-import arguments through the shared bounded graph
discovery path, and resolves ordinary relative external module-script URLs for
rooted file documents separately from import-specifier resolution. The scoped
check, parser tests, process-backed HTTP/worker tests, rooted-file test,
formatting, and all four local documentation gates pass; remote CI was not run.
At the 711 checkpoint, runtime-valued expressions and complete module
scheduling remained separate gates. Slice 712 also removes non-standard worker import maps from the open
profile and prevents member methods named `import` from triggering prefetch;
see `tasks/native-engine-browser-712.md`.
Recover verifiable scope and test evidence for slices 650–671 from authoritative
commits; do not infer passing checks from summary prose. Keep the issue body and
this record explicit about local versus remote evidence.

## Native HTML parser-route parity

Slices 673–685 align bounded table tree-construction behavior across
`NativeDocument::parse`, Rust `NativeDocument::apply_script_inner_html`, the
shared JavaScript `populateDetachedFragment` same-turn projection, and the
independent `nativeHtmlParseDocument` route for XHR
`responseType="document"`. Slices 673–675 cover foster insertion; slice 676
adds implicit `tbody`/`tr`, and slice 677 adds implicit `colgroup`
construction; slice 678 handles nested table starts and fragment context;
slice 679 applies HTML table scope to `</table>` and excludes the fragment
context from the open-element stack; slice 680 scopes row-group and row end
tags to their matching HTML elements without crossing nested-table boundaries;
slice 681 extends scoped end-tag closure to `td` and `th` cells; slice 682
recovers structural starts inside open cells while preserving the active row,
row group, table, namespace, and fragment boundaries.
Fixtures check parentage, authored order, explicit-section preservation,
whitespace/comments, resource bounds, and namespace behavior across document,
fragment, same-turn projection, and XHR routes. Continue expanding the corpus
for other tree-construction rules; do not describe these slices as general
HTML parser parity.

Slices 678–682 complete additional bounded WHATWG in-table recovery rules across
document parsing, Rust fragment commit, same-turn JavaScript projection, and
XHR HTML parsing. Start-tag recovery preserves cell/caption and
template/foreign-content boundaries; end-tag scope closes the nearest open
HTML table and ignores out-of-scope tokens. Fragment context alone does not
make a table open. Slices 680–681 add scope-aware closing for row-group, row,
and cell tags; slice 682 adds structural starts inside cells, including
foreign-namespace, nested-table, template, and fragment boundaries. The
focused route fixture passed 1/1 and the table-focused batch passed 45/45.
Other insertion modes and general parser conformance remain open.

Slice 683 completes the in-cell ignored end tags `body`, `caption`, `col`,
`colgroup`, and `html` before generic ancestor matching, with the same
actual-cell/table scope across document, fragment, same-turn projection, and
XHR parsing. The focused route fixture passed 1/1, the table batch passed
45/45, and the locked package check passed. Full in-cell parser conformance
remains open; see `tasks/native-engine-browser-683.md`.

Slice 684 ignores `</body>`, `</caption>`, `</col>`, `</colgroup>`, and
`</html>` in open HTML row/row-group contexts without an active cell, before
generic ancestor matching. The four parser routes share the scope probe, while
caption, column-group, nested-table, template, and fragment-root behavior stays
separate. The focused test passed 1/1, the table batch 45/45, and the locked
package check passed. Full insertion-mode conformance remains open; see
`tasks/native-engine-browser-684.md`.

Slice 685 completes end-tag recovery in direct table, caption,
and column-group contexts across document parsing, Rust fragment commit,
same-turn JavaScript projection, and XHR HTML parsing. It complements the
existing cell, row/row-group, and table-structure handlers while preserving
each context's distinct ignore, close, and reprocess behavior. Actual table,
namespace, template, and fragment-context boundaries remain explicit. The
four-route fixture passed 1/1, the table-focused batch passed 46/46, and the
locked package check passed; documentation validation reported 1,313 Markdown
files and zero current-claim failures. See `tasks/native-engine-browser-685.md`.

Slice 686 is complete: active-formatting-list reconstruction, marker and
Noah's Ark handling, and the bounded adoption-agency algorithm now run in all
four parser routes. The focused active-formatting tests pass 2/2 and the table
regression batch passes 46/46; general HTML parser conformance remains open.

Slice 690 is complete locally: select HTML RAWTEXT/RCDATA states only for
HTML-namespace elements, and parse nested markup inside same-named SVG/MathML
elements using the foreign/integration context across all four parser routes.
The exact route test passed 1/1 and the HTML parser batch passed 27/27; complete
parser conformance remains open. See `tasks/native-engine-browser-690.md`.

Slice 691 is complete locally: apply HTML void-element stack and serialization
rules only to HTML-namespace elements across all four parser routes. Same-named
foreign SVG/MathML elements retain descendants unless explicitly self-closing;
fragment `innerHTML` still parses when its target has an HTML void-element
name. The exact four-route test passed 1/1 and the HTML batch passed 28/28;
remote CI was not run. See `tasks/native-engine-browser-691.md`.

Slice 692 is complete locally: implement the WHATWG foreign-content breakout
start-tag set and conditional `font` trigger across all four parser routes.
Stop stack unwinding at HTML/integration boundaries, preserve foreign
fragment targets while switching to the synthetic HTML context, and ignore a
`/>` flag for all HTML-namespace elements, including at integration points;
explicit self-closing applies to foreign-namespace elements. Its foreign batch
passed 4/4, HTML batch 31/31, and local package/documentation gates passed.
Remote CI was not run; end-tag breakout and full foreign-content conformance
remain open. See
`tasks/native-engine-browser-692.md`.

Slice 693 is complete locally: implement foreign-content `</br>` and `</p>`
breakout across all four parser routes. Unwind to HTML/integration boundaries,
then reprocess under the active HTML insertion mode; run table-mode consumers
before in-body `br` conversion and button-scope `p` close/insertion. Preserve
the foreign fragment target and test projection/commit parity. The foreign
batch passed 6/6 and the scoped HTML batch 33/33; local package and
documentation gates passed. Remote CI was not run. General end-tag and parser
conformance remain open. See `tasks/native-engine-browser-693.md`.

Slice 694 is complete locally: implement ordinary foreign-content end-tag
matching, stack popping, HTML-boundary reprocessing, and root-fragment
preservation across all four parser routes. Restrict the reprocessed HTML
any-other-end-tag path to HTML-namespace targets and stop at special
tree-builder elements. The locked package check passed; the foreign batch
passed 8/8 and the HTML batch passed 34/34. Formatting, whitespace, and local
release-truth, depth, and shortcut validators passed. The full documentation
inventory/link check was not run because no CLI/MCP/module inventory changed
and debug CLI binaries were not built. Remote CI was not run. General HTML
parser conformance remains open. See `tasks/native-engine-browser-694.md`.

Slice 695 is complete locally: apply WHATWG SVG/MathML attribute-name
adjustment and XLink/XML/XMLNS foreign-attribute mappings across all four
parser routes. Keep the prefix, local name, namespace URI, and serialization
consistent; preserve unnamespaced attributes in HTML elements and
first-duplicate-wins tokenization. The locked package check passed, the
foreign batch passed 9/9, and the scoped HTML batch passed 34/34. Local
documentation, formatting, and whitespace gates passed; remote CI was not
run. General parser conformance remains open. See
`tasks/native-engine-browser-695.md`.

Slice 696 is complete locally: initialize RCDATA from `title`/`textarea`
fragment context, including SVG `title`, and resume ordinary fragment parsing
after the appropriate context end tag. The regression covers decoded text,
null replacement, the closing-tag slash boundary, EOF handling, and suffix
namespace/parentage parity between Rust commits and same-turn projection.
Full-document parsing retains the SVG `title` HTML-integration behavior from
slice 690. The HTML batch passed 35/35 and the foreign batch 9/9 after the
locked package check. Remote CI was not run. See
`tasks/native-engine-browser-696.md`.

Slice 697 is complete locally: initialize fragment parsing in RAWTEXT for
`style`, `xmp`, `iframe`, `noembed`, `noframes`, and scripting-enabled
`noscript`. Preserve literal source through the matching context end tag,
resume ordinary fragment parsing afterward, and align all parser routes plus
serialization. The focused RAWTEXT tests passed 2/2 and the foreign namespace
route test passed 1/1 after the locked package check. Remote CI was not run;
script-data and `plaintext` initialization are outside slice 697 and are
handled separately by slices 699 and 698. See
`tasks/native-engine-browser-697.md`.

Slice 698 is complete locally: initialize a `plaintext` fragment target in
the PLAINTEXT state and consume all source as one literal text node through
EOF, including markup-looking suffixes after `</plaintext>`. Preserve
references, replace nulls, normalize newlines, and serialize literally across
same-turn projection and Rust commit. The locked package check and six
focused RCDATA/RAWTEXT/PLAINTEXT fragment-context tests passed. Formatting and
local documentation gates passed; inventory/link coverage was skipped because
its debug binaries were absent, and remote CI was not run. Document/XHR
start-tag parsing remains separate. See `tasks/native-engine-browser-698.md`.

Slice 699 is complete locally: distinguish Script Data, escaped, and
double-escaped tokenizer states when finding the appropriate HTML `script`
end tag. A double-escaped `</script>` remains text and only returns the
tokenizer to the escaped state. The covered document, Rust fragment,
same-turn JavaScript/frame, and XHR routes agree. General tokenizer and script
execution conformance remain open; remote CI was not run. See
`tasks/native-engine-browser-699.md`.

Slice 700 is complete locally: dynamically created classic inline scripts
execute once when connected through `appendChild` or `insertBefore`; detached
preparation and reinsertion of an already-started script do not cause execution
or errors. Parser-only active-formatting state was removed from the runtime
mutation path. The focused regression passed 1/1 after the locked package
check. Bounded process-backed dynamic external/module loading remains as
implemented in slices 279–281; full parser/task timing, async/defer semantics,
and general script conformance remain separate. Remote CI was not run. See
`tasks/native-engine-browser-700.md`.

Slice 701 initially added five JavaScript MIME types to connected dynamic
inline classic-script handling but also incorrectly ignored `type` parameters.
Slice 703 corrects that behavior and expands to all 16 exact essences. See
`tasks/native-engine-browser-701.md` and the correction in
`tasks/native-engine-browser-703.md`.

Slice 702 briefly reused response Content-Type parameter handling for script
`type` attributes, incorrectly accepting parameterized values. Slice 703
separates those policies and expands the accepted essence list. See
`tasks/native-engine-browser-702.md` and the correction in
`tasks/native-engine-browser-703.md`.

Slice 703 is complete locally: initial and dynamically inserted classic
scripts recognize all 16 JavaScript MIME type essences case-insensitively,
with no parameters or surrounding whitespace for the script `type` attribute.
External response Content-Type parameters remain ignored. The focused
MIME-policy batch passed 4/4 after the locked package check; remote CI was not
run. See `tasks/native-engine-browser-703.md`.

Slice 689 is complete locally: select child element namespaces using SVG and MathML
HTML integration-point rules across all four parser routes. Preserve the
`mglyph`/`malignmark` exceptions, the `annotation-xml` SVG special case, and
foreign-parent namespaces elsewhere. SVG `title` tokenizer state and complete
foreign-content dispatch remain separate. See
`tasks/native-engine-browser-689.md`.

Slice 688 is complete locally: route literal U+0000 through the existing HTML data,
RAWTEXT/RCDATA, comment, attribute-value, and foreign-content handling rules
across all four parser routes. Ordinary HTML data ignores it; foreign data,
raw/RCDATA, comments, and attribute values replace it with U+FFFD. Numeric
references to U+0000 also resolve to U+FFFD. Preserve HTML/foreign integration
point selection; decode references before tree-builder NUL handling so removed
NUL cannot join characters into a synthetic reference. Leave tag/attribute
names, doctypes, and unsupported tokenizer states explicit for follow-up. See
`tasks/native-engine-browser-688.md`.

Slice 687 completes CRLF/lone-CR normalization to LF before tokenization in all
four HTML parser routes, including text, attribute values, comments, script raw
text, and RCDATA. Rust parse errors keep original-source offsets. The locked
package check and 24-test HTML regression batch passed; all maintainer docs
gates passed over 1,315 Markdown files. Do not globally replace U+0000; its
behavior is context-sensitive and remains a separate tokenizer/tree-
construction task. See `tasks/native-engine-browser-687.md`.

## Input: popup-opening mouse release completion

The compare-018 diagnostic proves that an ordinary pointer click whose authored
handler calls `window.open` receives the `mousePressed` response but not the
`Input.dispatchMouseEvent(mouseReleased)` response before the 30-second CDP
deadline. The operation route is already task-local and response delivery is
request-ID keyed, so do not mask this with fire-and-forget input or an unrelated
route rewrite. Establish the Chromium/Target-auto-attach protocol behavior and
add a real-browser regression before changing input completion semantics. The
same-process frame scorecard path remains unassessed because it follows the
poisoned popup route.

## Comparative acceptance: broader corpus and complete resource scopes

After the local v1 gate, add a versioned representative workflow corpus rather
than treating deterministic fixtures as external task success. Define portable
process-tree accounting for Node clients, MCP servers, and Chrome so competitor
memory can be compared without nullable scopes. Add a callable Codex adapter
only if Codex exposes a versioned black-box automation contract to the harness;
do not infer behavior from an interactive product surface.

The released Playwright MCP baseline exposes complex workflows through a tool
explicitly named `browser_run_code_unsafe`. Add a second agent baseline with a
non-RCE task surface, and separate privileged-tool usability and safety
evidence before making broad agent-friendliness claims.

## Lifecycle: cleanup after implicit incognito-session drop

`BrowserSession::close()` correctly stops a Glass-owned Chrome process before
removing its disposable incognito profile directory. An implicit
`BrowserSession`/`ChromeProcess` drop can only initiate process termination,
so cleanup can race Chrome-held files on platforms such as Windows. Keep the
explicit close contract for library callers, and add an abnormal-shutdown
cleanup mechanism plus a live-session drop regression test in a follow-up.

## Scorecard: cold lifecycle and exhaustive side-effect oracles

Corpus v1 deliberately defines only a warm single-session comparison. Before a
cold scorecard is emitted, define the same process/profile/cache lifecycle for
every adapter and add it to the final comparative acceptance work in
`compare-018`.

The current declarative `forbidden` outcomes catch known wrong-target side
effects and make every such outcome a hard failure. A future corpus should
capture selected target identity or a complete fixture side-effect ledger so
`wrong_actions` is exhaustive rather than limited to enumerated forbidden
values.

## Targeting: remote-handle cleanup on intermediate CDP errors

Bounded CSS/text discovery releases its array and child remote objects on the
successful path. If `Runtime.getProperties` or `DOM.requestNode` fails midway,
the remaining remote handles are left for Chrome's execution-context cleanup.
Add an actor-owned remote-object guard so every partial-error path releases all
objects without increasing the fast reference path's request count.

## Waits: richer diagnostics and network stress instrumentation

Promote Network event lag/domain failures to the same typed wait-error surface
as timeouts, define whether visible-text substrings may span adjacent text
nodes, and add a synthetic CDP stress driver that records Network lease memory
and event-lag behavior under thousands of concurrent requests.

## Topology: typed errors and parallel smoke-test ports

Promote no-selection, stale target/frame, topology-budget, and routing-loss
failures to a bounded typed error that MCP can safely serialize without URLs
or raw oversized IDs. The current explicit behavior is correct but generic
errors give agents less recovery guidance than target and wait failures.

Opt-in browser smoke tests pass serially. Their release-and-reclaim ephemeral
port helper can collide when the suite runs in parallel; replace it with a
process-wide test port lease before making parallel E2E execution a gate.
