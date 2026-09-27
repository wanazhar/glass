---
id: native-engine-browser-772
scope: glass-browser/native-image-map-keyboard-focus
status: completed
depends-on: [native-engine-browser-771]
---

# Glass native-engine browser slice 772: image-map keyboard focus and activation

## Objective

Make linked image-map areas reachable through native sequential keyboard focus
and activate them with Enter through the existing click and hyperlink owners.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  keyboard focus and hyperlink activation across native browser workflows.
- [Slice 771](native-engine-browser-771.md) implements live image-map geometry,
  pointer targeting, area link semantics, and shared navigation defaults.
- The [HTML Standard focus model](https://html.spec.whatwg.org/multipage/interaction.html#sequential-focus-navigation)
  models an image-map area as focusable shapes anchored to their associated
  rendered image. Keyboard events route to that DOM anchor; the synthesized
  link click activates the selected `area`.
- The [HTML Standard image-map processing model](https://html.spec.whatwg.org/multipage/image-maps.html#image-maps)
  keeps maps live and permits one map to be associated with multiple images.

## Contract

- A linked `area[href]` with a non-empty shape and at least one rendered,
  associated image participates in forward and reverse sequential focus.
  Empty shapes, dead areas, hidden areas, and unassociated maps do not become
  keyboard link targets. Each associated image contributes its own focus
  anchor for the linked area.
- Sequential ordering honors the area's `tabindex` convention, places positive
  values before natural-order links, and skips negative values. Forward and
  reverse traversal wrap within the existing native focus owner.
- The focused state remains associated with the `area` shape while
  `document.activeElement` resolves to its selected image DOM anchor. Native
  `keydown` and `keyup` events are dispatched to that image; Enter's synthesized
  cancelable `click` targets the `area` itself. Space does not activate it.
- Canceling keydown suppresses click dispatch; canceling the click suppresses
  the link default. After listeners run, revalidate focus, map association,
  attachment, shape, and live `href` before using the normal URL-policy,
  target, opener, and download owners.
- Local, HTTP(S) content-process, and same-origin-frame documents share the
  same focus, event, and activation behavior.

## Boundaries

This is the focused keyboard-area contract, not complete keyboard, focus-chain,
CSS `:focus`, inertness, focus-ring rendering, WPT, cross-platform, or issue #40
completion. Those broader profile requirements remain tracked independently.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-772.md`

## Verification

Both focused regressions pass. The local fixture verifies Tab/Shift+Tab,
positive and explicit-zero tabindex ordering, exclusion of dead/negative/empty
areas, repeated focus anchors for two associated images, area `:focus` and
image `activeElement`, keyboard event routing to the image, cancelable keydown
and click, live `href`/`target` after click listeners, `_blank` opener behavior,
and Space non-activation. The HTTP(S) test verifies process-backed parent
`_blank` activation, `<area download>` filename and bytes, frame-local key/click
targets under cancellation, and uncanceled same-origin-frame navigation.

An initial frame-test probe tried to read its event trace through a parent
global array; the final regression keeps that trace in the frame and separates
canceled-event assertions from navigation.

Checks passed:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo check -p glass-dev --lib --bins --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_image_map_keyboard_focus_and_enter_activation --exact --test-threads=1`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_http_image_map_keyboard_focus_and_same_origin_frame_activation --exact --test-threads=1`

Formatting and documentation checks also pass:

- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-772.json` (1,400 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py` (93 current guides routed/audited)
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py` (1,400 Markdown files; 346 full-product MCP tools; 17 examples; 22 public modules)
- `git diff --check`

Workspace/all-targets tests, WPT, remote CI, platform certification, and issue
#40 completion remain out of scope for this bounded slice.
