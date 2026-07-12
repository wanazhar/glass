# Review: target-009 deterministic element targeting

Reviewed commit: `ee61ba6` (`feat: enforce deterministic element targeting`)
against main `14c589b`.

Conclusion: **blocked**. Exact accessible-name/reference resolution and
pre-input rejection are materially safer than the old first-match behavior,
but action-point verification is not stable through dispatch, text lookup is
not actually a text strategy, and MCP discards the ambiguity outcome agents
need to recover safely.

## Findings

### P1 — blocking: geometry and hit ownership are stale before pointer dispatch

`verified_action_point` runs once before the pointer path is generated or sent
(`src/browser/session.rs:755-767`). Inside its JavaScript, the two geometry
samples are consecutive synchronous calls with no animation frame, observer,
or delay between them (`src/browser/session.rs:39-55`). Except for the explicit
`getAnimations()` check, they cannot detect layout movement over time.

More importantly, human mode can then spend many movement-delay intervals
travelling to that old point before `mousePressed`, and sleeps again between
press and release (`src/browser/session.rs:782-825`). There is no attachment,
geometry, viewport, or hit-owner revalidation immediately before the press.
A page can reflow, detach the target, show an overlay, or replace it during
pointer movement/mouseover and Glass will still press the previously verified
coordinates. Fast mode also does not revalidate after its `mouseMoved`, whose
page handler can synchronously change the hit target before `mousePressed`.

The fixture catches only a continuously CSS-animated element *before* input;
it does not exercise reflow on pointer move/hover or between press/release
(`tests/fixtures/basic.html:21`, `tests/browser_smoke.rs:794-795`). This violates
the requirement to verify immediately before dispatch and the architecture's
bounded re-resolution/explicit-failure rule. Revalidate ownership and geometry
at the actual press boundary (and define release behavior), failing or
re-resolving within a bound before any button event.

### P1 — blocking: MCP erases typed ambiguity and candidate diagnostics

Resolution constructs at most eight candidate summaries, but
`resolve_element` immediately flattens them into an untyped error string
(`src/browser/session.rs:873-885`). The hardened MCP layer replaces every
browser/tool error with the generic text `browser tool failed`
(`src/mcp/server.rs:461-473`), so an MCP agent cannot distinguish ambiguous,
not-found, stale, disabled, or blocked outcomes and receives no candidate
summaries at all. `TargetResolution` itself is private
(`src/browser/session.rs:176-181`).

This conflicts with the task objective to return typed unique, ambiguous, and
not-found results, its bounded-candidate requirement, and the architecture
contract. CLI receives a string while MCP receives no detail, so the frontends
are not coherent despite accepting the same locator syntax. Introduce a typed,
bounded, non-secret browser action error surface that MCP can serialize safely;
ambiguity candidates should use bounded public labels/references without
echoing raw caller arguments.

### P1 — blocking: `text=` uses generic DOM search, not deterministic text matching

`text=` delegates to CDP `DOM.performSearch` and trusts its `resultCount`
(`src/browser/cdp.rs:376-421`, `src/browser/session.rs:957-972`). That CDP API is
a combined plain-text/CSS/XPath search, not a defined exact or visible-text
locator. It can return element and text nodes, hidden DOM matches, selector-like
matches, and user-agent shadow results. The code comment calling it “visible
DOM text” is therefore unsupported. Queries that are valid CSS/XPath syntax
can resolve based on structure rather than text, and a single hidden match can
be selected then fail actionability instead of producing text-not-found.

Define text semantics (exact versus substring, whitespace normalization,
visibility, element ownership), implement that traversal explicitly with a
bounded result set, and add adversarial hidden/duplicate/nested/selector-like
text cases. Until then the advertised explicit text strategy is not
deterministic or separate from CSS as required.

### P2 — blocking: locator discovery allocates unbounded page-controlled collections

CSS lookup asks `DOM.querySelectorAll` for every matching node and collects the
entire returned array before truncating only the diagnostic candidates
(`src/browser/cdp.rs:356-373`, `src/browser/session.rs:1128-1150`). Accessible
name and role resolution similarly builds a `Vec` of every matching interactive
AX element from the full snapshot (`src/browser/session.rs:909-925`). A hostile
page with hundreds of thousands of matching nodes can force large CDP payloads
and allocations even though only uniqueness and eight summaries are needed.

This violates the automation architecture's explicit bound on every retained
collection and undermines the memory-efficiency objective. Count with a bounded
projection/early stop (enough to distinguish zero, one, and many), retaining at
most the candidate limit plus uniqueness metadata. Document unavoidable CDP
payload bounds if Chrome offers no bounded selector API.

### P2 — non-blocking: adversarial failures do not assert zero pointer events

The real-browser test checks returned error strings for ambiguity, disabled,
overlay, animation, and detachment, but resets `window.pointerEvents` only
after all those checks and never asserts it stayed empty
(`tests/browser_smoke.rs:773-805`). The implementation currently resolves or
verifies before calling `dispatch_pointer_events`, so these cases appear safe,
but the task explicitly requires no pointer input on failures. Assert the event
ledger after each failure class, including both fast and human modes, to make
that safety property regression-proof.

### P3 — non-blocking: evidence does not substantiate allocation claims

The completion reports latency, round trips, final RSS, payload size, and
binary size, but says allocator-level peak instrumentation is unavailable
(`docs/plan/tasks/target-009.md:66-71`). Verification explicitly requires fast
reference-path allocations not to regress materially. Final process RSS is not
an allocation measurement and the benchmark uses only 20 click samples. Record
an allocation proxy/profile or revise the verification contract with a
documented, reproducible substitute before making an allocation claim.

## Positive observations

- Bare and explicit revision references retain revision checks and backend-node
  resolution; stale revisions fail before CDP node resolution
  (`src/browser/session.rs:889-906`).
- Accessible name and role+name use exact case-insensitive equality; role-only
  syntax and zero ordinals are rejected (`src/browser/session.rs:909-925`,
  `1076-1122`).
- CSS uniqueness and text result counts no longer silently select the first
  returned match, and candidate labels are limited to eight UTF-8-safe
  160-byte strings (`src/browser/session.rs:34-35`, `936-972`, `1153-1164`).
- Legacy MCP `selector` is explicitly prefixed with `css=` while `target`
  preserves the shared CLI syntax (`src/mcp/server.rs:698-708`).
- Attachment, basic visibility, disabled state, viewport center, and current
  center hit ownership are checked before entering pointer dispatch
  (`src/browser/session.rs:36-64`, `981-1002`).

## Focused verification

I reused the task's recorded full test, browser, scorecard, and benchmark runs.
Independent inexpensive checks passed:

- `cargo test browser::session::tests --lib` — 16 passed;
- `cargo test browser::cdp::tests --lib` — 8 passed;
- `git diff --check 14c589b ee61ba6`.

No implementation changes were made during this review.
