# Native Glass `navigate-to` navigation policy (411)

status: done
scope: native-engine/csp-navigate-to
issue: 40

## Objective

Make the native browser enforce Glass's explicit `navigate-to` navigation
policy for top-level navigation. A denied destination must not issue a network
request or replace the current document, regardless of whether the request was
started by the host, a page script, a link, a download, a popup, history, a
content-process page handoff, or a final URL returned by the content process.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-410.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)

The existing loader parsed CSP response and meta policies inside the native
content process, while the parent engine owned top-level navigation and
history. Without an explicit transfer, the parent could not enforce a page's
navigation policy on a link or `location` handoff. Glass names this directive
as an extension because it is not a normative CSP Level 3 directive.

## Contract

- The shared CSP policy owner recognizes the typed `navigate-to` policy kind.
- Only an explicit `navigate-to` source list contributes to this policy;
  omitting it does not inherit `default-src`. Multiple enforced declarations
  intersect, including declarations from separate response headers and the
  initial head meta policy set.
- The content process transfers bounded source groups with each loaded
  document. The parent stores those groups with the committed document and
  applies the same shared source matcher as the resource loader.
- Direct navigation, same-document navigation, page-script/location
  navigation, link activation, download and popup destinations, history
  traversal, and content-process final URLs are checked before a request or
  commit. A denied decision preserves the current document and URL.
- URL validation, credential rejection, source-expression matching, and the
  existing form-action policy remain separate typed decisions. `form-action`
  governs form destinations; `navigate-to` governs top-level destinations.
- Report-only navigation observations remain owned by the loader that parsed
  the response. This slice does not claim report-only delivery through every
  navigation path.

## Non-goals

This slice does not add new navigation methods, browser targets, CSP grammar,
or a normative interpretation of `navigate-to`. Later dynamic CSP meta-policy
mutation is still owned by the content process and is not re-exported as a
parent navigation policy in this checkpoint. It does not change redirects,
forms, popup ownership, downloads, history, or lifecycle behavior beyond
putting the new policy check at their existing navigation boundaries.

## Implementation path

- Add `NavigateTo` to the shared navigation-policy enum and retain explicit
  source groups alongside the existing `form-action` and frame projections.
- Serialize and validate bounded navigation source groups over the typed
  content-process load response, reusing the frame-source transfer limits and
  validation shape without weakening the process boundary.
- Store the groups in `PreparedNavigation` and the committed engine document
  state, including normal navigation, same-owner history activation, local
  links, and async content-process commits.
- Centralize parent checks so silent network preflights do not duplicate
  report-only records, while synchronous/local decisions still use the normal
  reporting path.
- Check both the requested destination and the final content-process URL so a
  redirect cannot bypass the owner policy.
- Add a real HTTP content-process witness for link and `location` navigation
  blocked by `navigate-to 'none'` before a second request.

## Tradeoffs

- Transferring parsed source groups keeps the content process as the response
  policy owner and avoids exposing raw response headers across IPC, at the cost
  of a small bounded copy per loaded document.
- Parent-side enforcement is required because the parent owns targets, history,
  downloads, popups, and commits. The content process cannot be the sole
  guard for those effects.
- Final-URL validation is deliberately conservative: a redirect must satisfy
  the original document's policy before commit, even when the response body
  itself is valid and the destination policy will later become the new owner.
- This slice does not invent a standards claim for a custom directive; the
  extension remains explicitly labeled in the architecture and plan.

## Delivered

- Added explicit enforced and report-only-aware `NavigateTo` policy plumbing
  to the shared resource loader.
- Added bounded content-process serialization and decoding for response and
  initial meta navigation source groups.
- Added committed parent document policy state and checks across direct,
  script, link, download, popup, history, same-document, and final-url paths.
- Added the HTTP content-process regression witness proving that both link and
  `location` navigation leave the current URL in place and issue no follow-up
  request when `navigate-to 'none'` is active.
- Synchronized the architecture, active plan, analysis, and task evidence.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_form_action_is_explicit_and_intersected_across_headers --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_navigate_to_blocks_link_and_location_navigation --locked -- --exact` (1 passed)
- Documentation validators for release metadata, documentation depth,
  shortcut coverage, and documentation coverage.

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
