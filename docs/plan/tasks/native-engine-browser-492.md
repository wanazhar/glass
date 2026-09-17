# Native engine browser-complete slice 492: ordered local font sources

- Status: complete
- Scope: ordered `@font-face` `local()` and URL source resolution
- Issue: #40
- Depends on: [native-engine-browser-491](native-engine-browser-491.md)

## Objective

Honor the ordered source list in an `@font-face` rule. A locally installed
matching face must be usable without a network request, while a missing local
face must leave later URL candidates available as the normal fallback path.

## Contract

- CSS retains up to eight admissible source candidates in declaration order.
- `local("Family")` and `local(Family)` are represented distinctly from
  `url(...)` candidates and may be followed by URL, data, file, or Blob
  sources.
- Local lookup is case-insensitive for the family name and selects the best
  deterministic system-book face for the requested normal/bold and
  normal/italic pair.
- A local match is admitted directly as bounded font bytes; it does not use
  the resource loader, cookies, CSP, CORS, cache, or network policy.
- A missing local face does not reject the rule. The owner tries subsequent
  source candidates in their CSS order, preserving existing URL admission and
  object-URL handling.
- The CSS parser remains capability-free: it only records bounded source
  values, and the inline/content owners retain responsibility for resolution
  and byte admission.

## Implementation

- Replace the single font source string with a bounded ordered
  `NativeFontFaceSource` list.
- Share the named CSS-function boundary helper between `url()` and `local()`
  parsing.
- Add case-insensitive, weight/style-aware lookup to `NativeFontBook`.
- Use the same ordered source resolver in the inline engine and content
  process, stopping after the first admitted local or URL face.
- Add parser, local lookup, unknown-family, and existing font behavior tests.

## Tradeoffs and remaining scope

Local lookup intentionally uses Glass's deterministic system-font candidate
book rather than scanning every installed font or exposing arbitrary host
filesystem paths. This keeps rendering reproducible and the content boundary
small, but additional platform font discovery remains a future issue #40
capability. The slice does not claim FontFace/FontFaceSet loading events,
`font-display` timing, variable/color fonts, format descriptors,
language/script matching, or complete browser text/Web IDL parity.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_face` (2 passed)
- `cargo test --quiet -p glass-browser --lib --locked font` (24 passed)
- process/listener audit found no stale Glass, Cargo, rustc, Chromium,
  Firefox, or native-content-worker targets
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
