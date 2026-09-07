# Experimental capabilities

Experimental capabilities are intentionally opt-in. They are available for
evaluation, but Glass does not promise a stable API, complete platform
coverage, or unchanged behavior between releases.

## Discover status first

Inspect the negotiated capability manifest without starting a browser:

```console
glass capabilities
```

Check both fields:

- `capabilities` says whether a capability is currently usable;
- `capabilityStatuses` explains why, such as `disabledByPolicy`, `experimental`,
  or `blockedBySecurityGate`.

Clients should use `capabilityStatuses` when deciding whether to offer an
experimental feature. A boolean alone does not explain whether a feature is
disabled by policy, missing a runtime dependency, or blocked for security.

## Enable experimental extensions

Pass the explicit global flag for the process that will use the capability:

```console
# Inspect the resulting manifest
glass --experimental-extensions capabilities

# Start the TUI with the experimental capability negotiated
glass --experimental-extensions tui

# Start MCP with the experimental capability negotiated
glass --experimental-extensions --mcp
```

The flag is not persistent. Omitting it on the next process start disables the
experimental capability again. Glass prints a warning when the flag is used.

On Linux ARM64, the flag reports extensions as
`experimental` only when all of these conditions hold:

- the host is Linux ARM64;
- Bubblewrap is available and can start; and
- the extension uses the sandboxed host path.

On another OS or architecture, or when the sandbox is unavailable, the status
remains `blockedBySecurityGate`. Glass must not silently fall back to an
ordinary unsandboxed subprocess.

## What to expect

Experimental means more than “the code might have bugs.” Expect all of the
following:

- interfaces, manifest fields, and behavior may change in a later release;
- an invocation may fail, time out, or be unavailable on another platform;
- browser and extension behavior may differ from the stable capabilities;
- documentation and compatibility guarantees are limited to the stated
  target and runtime evidence; and
- an experimental capability must not be used as a production security
  boundary.

Extension code is untrusted code. The sandbox reduces its operating-system
authority, but does not make it trusted. Run only extensions you have reviewed,
avoid sensitive browser profiles while evaluating them, and do not provide
secrets merely because an extension declares a host or action permission.
Guarded browser actions remain subject to Glass policy, revision checks, target
permissions, and bounded verification.

## Why a capability remains experimental

A capability can remain experimental when one or more of these are true:

1. its public contract is still evolving;
2. required runtime dependencies are not available everywhere;
3. native security behavior has only been verified on a limited target set;
4. cross-interface compatibility is incomplete; or
5. failure, recovery, migration, or resource-limit behavior needs more
   evidence.

The status is a deliberate compatibility and safety signal, not a claim that
the implementation is unusable. Experimental capabilities can be useful for
local evaluation while remaining unsuitable for general support claims.

## Guidance for future experimental capabilities

Every new experimental capability should provide:

- a default-off configuration or explicit global opt-in;
- a stable capability ID and `experimental` status in the manifest;
- a fail-closed status when its platform or security dependency is missing;
- a visible warning that explains risk and expected instability;
- CLI, TUI, MCP, and library behavior that agrees on the status;
- deterministic tests for opt-in, refusal, and dependency absence; and
- documentation covering enablement, scope, limitations, and disablement.

Do not change a capability from `experimental` to `available` merely because
one local smoke test passes. Promote it only after its contract, security
boundary, applicable target environments, and compatibility evidence justify
the broader claim.

## Native browser engine

The Glass-owned native engine is an experimental `glass-browser` backend from
[issue #40](https://github.com/wanazhar/glass/issues/40). It is compiled only
when explicitly requested:

```console
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
```

Phase 2 and the initial Phase 3 layout/display-list/software-surface seeds are
headless and deterministic. It accepts `about:blank`, bounded percent-decoded
or standard padded-base64 `data:text/html` URLs, and registered `fixture://`
documents through the Rust API. It exposes lifecycle, navigation, one context,
bounded URL/title/visible-
text evidence, semantic click/type actions for local controls, and a
revision/changed effects signal through the existing backend dispatcher.

The action surface focuses supported controls, toggles checkbox/radio state,
selects an option in a single-select control, and replaces private text state
for `input`/`textarea` controls. Its bounded visibility gate excludes
`hidden`, `aria-hidden="true"`, and inline `display:none`/`visibility:hidden`
subtrees from visible text and rejects those action targets before mutation.
Explicit references are revision-bound; ambiguous, stale, disabled, read-only,
and unsupported targets fail before mutation. It provides only bounded
presentation, normal-flow outer/content box geometry with bounded physical
four-side padding/margin shorthands and longhands plus explicit box sizing and
bounded physical min/max width/height constraints with bounded
case-insensitive 15-layer/unlayered local `revert-layer` rollback for finite
non-negative integer-pixel `width`, `height`, `min-width`, `max-width`,
`min-height`, and `max-height` with absent local fallbacks,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `box-sizing`, physical padding and margin edges, including bounded
`margin:auto`, with content-box/zero local fallbacks,
bounded vertical viewport
scrolling, inherited text color,
bounded `overflow:hidden` clips shared by paint, viewport projection, and point
hit-testing, bounded axis-specific `overflow-x`/`overflow-y` `hidden`/`clip`
clips through the same owner, plus bounded case-insensitive 15-layer/unlayered
local `revert-layer` rollback for `overflow`, `overflow-x`, and `overflow-y`,
preserving independent visible/no-clip fallbacks and the existing paint,
projection, point-hit, root-overflow, capture, and semantic/source-order owners;
nested scrolling, scrollbars, and browser-wide overflow semantics remain
outside the boundary. The same bounded local cascade path also accepts
standalone, case-insensitive 15-layer/unlayered local
`border-radius:revert-layer` for the one-to-four-value integer shorthand,
preserving the zero-corner fallback and the existing rounded layout, fill,
border, point-hit, capture, raster, overflow, and semantic/source-order owners;
elliptical, percentage, corner-longhand, nested-clip, anti-aliasing,
multiple-origin, and browser-wide border-radius conformance remain outside the
boundary. The same bounded local cascade path also accepts standalone,
case-insensitive 15-layer/unlayered `opacity:revert-layer` for the existing
local 8-bit opacity owner, preserving the full-opacity (`255`) fallback,
reduced-opacity group markers and software compositing, and unchanged layout,
point-hit, capture, raster, overflow, and semantic/source-order owners;
inherited opacity, stacking-context/blending parity, filters, animation,
multiple origins, and browser-wide opacity conformance remain outside the
boundary. The same bounded local cascade path also accepts standalone,
case-insensitive 15-layer/unlayered `revert-layer` for the existing local
`display` and `visibility` owners. It resolves through lower concrete
candidates or the normal-flow `display:auto` and visible fallbacks, preserving
`display:none`, `visibility:hidden`, and `display:contents` behavior across
hidden-subtree layout, point hit testing, display-list, capture, raster, and
semantic/source-order owners. Inherited visibility, display decomposition,
formatting-context parity, table/ruby/flow-root details, animation, multiple
origins, and browser-wide CSS display/visibility conformance remain outside
the boundary. The native border surface also accepts bounded case-insensitive
15-layer/unlayered local `revert-layer` for the physical `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left` owners.
Rollback resolves each side through lower concrete candidates or the existing
zero-width/no-paint fallback, preserving shorthand/longhand precedence and
the existing box-model inset, border display-list, capture, raster, point-hit,
and semantic/source-order owners. Logical sides, border-image, gradients,
other border styles, animation, multiple origins, and browser-wide CSS border
conformance remain outside the boundary. It also provides side-specific
solid/dashed/dotted-border paint,
bounded physical circular border radii, bounded inline-box line placement, bounded fixed pixel line-height
flow with bounded inherited line-height, bounded direct-text flow fragments and
source-order text paint, bounded
word-aware wrapping, bounded source-whitespace boundaries across supported
inline flow, bounded inherited `white-space: nowrap` collapsed one-line flow,
bounded root horizontal scrolling, bounded local opacity subtree groups with
inside-out transparent-layer compositing, bounded inherited physical
`text-align:left|center|right` fixed-cell line placement, bounded functional
`rgba(R, G, B, A)` alpha colors for background, border, and text paint, plus
bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback for
`background-color` and inherited `color`, preserving the existing
`None`/parent-root fallbacks and fill/text display-list, clipping, opacity,
capture, and raster owners; `currentColor`, gradients, system colors,
border-color, percentages, color spaces, and multiple origins remain outside
the boundary, bounded
inherited fixed-cell `text-decoration:none|underline|overline|line-through`
paint including distinct shorthand combinations, bounded local
`text-decoration-color` using the existing fixed palette and alpha grammar
with separate glyph and line paint, bounded `text-decoration-line` longhand
combinations sharing the same line-state owner, bounded inherited
`text-decoration-style:solid|dashed|dotted|double|wavy` presentation, where
`double` paints two thickness-preserving solid bands separated by one pixel and
`wavy` repeats a fixed eight-pixel phase `[0,1,2,1,0,-1,-2,-1]`,
bounded PNG
`text-decoration-thickness:1px|2px|3px|4px` positive-y bands with
thickness-scaled integer dash/dot periods, bounded inherited
`text-decoration-skip-ink:auto|none` same-run glyph intersection skipping for
underline and overline while preserving line-through, bounded inherited signed
`text-decoration-skip-spaces:none|all` same-run ASCII-space interval skipping
including adjacent letter, word, and final-line justification spacing across
underline, overline, and line-through, and bounded inherited signed
fixed-pixel
`text-underline-offset:-4px..=4px` that moves only the underline toward
decreasing or increasing y, bounded PNG capture, and bounded
inherited ASCII
`text-transform:none|uppercase|lowercase` layout,
bounded non-negative fixed-pixel first-line `text-indent` for block flow,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `text-indent` with a finite `0px` fallback,
bounded inherited non-negative fixed-pixel `word-spacing` across collapsed and
supported preformatted ASCII spaces, bounded inherited non-negative fixed-pixel
`letter-spacing` after every rendered fixed-cell character in each emitted
fragment, composed with word spacing,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `word-spacing` and `letter-spacing` with finite parent/root fallbacks,
bounded inherited `font-weight:normal|bold|400|700` fixed-cell raster
presentation with unchanged advances and clipped one-pixel bold dilation,
bounded inherited `font-style:normal|italic` fixed-cell raster presentation
with unchanged advances and clipped row-dependent italic shear,
bounded inherited `word-break:normal|break-all` collapsed fixed-cell wrapping,
bounded local `text-overflow:clip|ellipsis` on eligible clipped single-line
direct text,
plus bounded case-insensitive 15-layer/unlayered local `revert-layer` rollback
for `text-overflow` with a finite `clip` fallback,
bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for
fixed-cell inline and inline-block line items within the existing line box,
plus bounded case-insensitive 15-layer/unlayered inherited `revert-layer`
rollback for `vertical-align` with finite parent/root fallbacks,
bounded block-level `display:flex` single-row placement for eligible direct
element children with fixed widths and margins, with normal-flow fallback for
unsupported child shapes,
one non-negative fixed-pixel `gap` between visible items in eligible flex rows,
bounded `justify-content:flex-start|center|flex-end|space-between` free-space
placement for eligible fixed-width flex rows,
bounded non-inherited signed flex-item `order` in `-1024..=1024` with stable
source-order ties,
bounded non-inherited `flex-grow:0..=1024` and `flex-shrink:0..=1024`
allocation plus `flex-basis:auto|Npx` base-size selection, including
case-insensitive 15-layer/unlayered `revert-layer` rollback for the components
and standalone `flex:revert-layer`, `flex-flow:revert-layer`, and
`place-content:revert-layer` shorthand rollback with native fallbacks and
finite shorthand expansion, plus bounded case-insensitive gap-family
`revert-layer` rollback for `gap`, `row-gap`, and `column-gap` with independent
finite-pixel components,
bounded case-insensitive 15-layer/unlayered inherited `revert-layer` rollback
for `text-transform`, `font-weight`, `font-style`, and `word-break` with finite
parent/root fallbacks,
bounded case-insensitive 15-layer/unlayered rollback for non-inherited
`flex-wrap`, `justify-content`, `align-items`, `align-self`, `align-content`,
and `flex-direction` owners with their native fallbacks,
bounded non-inherited `align-items:flex-start|center|flex-end` cross-axis
placement using explicit content height or the auto row's maximum item outer
height,
bounded non-inherited `flex-direction:row|row-reverse` physical placement of
the order-sorted visual sequence with item-attached margins, existing
gap/justification/alignment, shared subtree artifacts, bounded overflow
translation, root horizontal scrolling, and unchanged semantic/source order,
and
Rust-only
display-list/software-surface artifacts;
Rust callers can inspect bounded revisioned diagnostics for unsupported CSS
without raw stylesheet echo;
it does not
provide network access,
filesystem navigation, general CSS/nested/stacking layout, logical
writing-mode sides, negative, percentage, or auto box-model values,
positioned layout or general flex/grid layout beyond the bounded single-row
`display:flex` subset,
screen-shot-containing evidence, JPEG/PDF capture, physical pixels,
font/image fidelity, JavaScript, storage,
prompts, downloads, remote navigation or relative navigation from unsupported
opaque bases, or raw form-value
evidence. Fragment navigation percent-decodes bounded UTF-8 `%HH` sequences,
then uses one exact, case-sensitive visible non-empty `id` target or, when no
`id` matches, one exact visible legacy `<a name>` target. It positions the
root viewport at the selected target's clamped document-space top and stores
one bounded root-scroll point per history entry for traversal restoration.
For ID/name targets, literal `+` remains a plus; missing, hidden, duplicate,
malformed, and invalid-UTF-8 targets preserve the current offset. IDs take
precedence and duplicate legacy names fail closed. Simple `#:~:text=start` and
`#:~:text=start,end` fragments also match the first visible, non-truncated
layout text run after independently decoding each term as bounded UTF-8;
bounded exact prefix/suffix affixes are supported when directly adjacent in
that same run; cross-run ranges, multiple directives, highlights, Unicode
normalization, and browser text-fragment parity remain unsupported.
Fragment-only, fixture-relative, and absolute local link activation uses the
existing bounded navigation path; fixture-relative links retain the current
registered fixture host. The
backend is explicit-only and never silently falls
back to Chromium or the semantic proof backend. A `native-engine` feature
build also exposes the local one-shot `--browser-runtime native` path and the
explicit Rust `BrowserRuntimeSession::connect_native` constructor. The CLI
default configuration supports only `about:blank` and bounded
`data:text/html`; registered fixtures remain a Rust configuration path.
Because it is in-process and incomplete, it is not a security boundary and
must not be used to claim safe handling of hostile remote content.
