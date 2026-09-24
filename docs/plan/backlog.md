# Backlog

## Issue #40: reconcile native-engine slice evidence

The latest locally completed parser slice is `native-engine-browser-699`.
Detailed records 650–671 remain missing and must be recovered from authoritative
commits rather than inferred from summary prose. Keep issue #40 as the remote
status mirror and refresh its current-checkout summary after each local slice
checkpoint; the local branch remains unpushed.
Remote CI, release, registry, and cross-platform promotion evidence are not
claimed.
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
