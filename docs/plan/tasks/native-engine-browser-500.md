# Native engine browser-complete slice 500: FontFace technology descriptors

- Status: complete
- Scope: page-realm `FontFace` `tech()` source descriptors
- Issue: #40
- Depends on: [native-engine-browser-499](native-engine-browser-499.md)

## Objective

Make source selection explicit about font technologies. A `tech()` requirement
the native renderer does not implement must not be silently accepted as if the
font were supported; the source may be skipped so a later ordinary candidate
can load.

## Contract

- Source entries may contain bounded `format()` and `tech()` function
  descriptors after `url()` or `local()`, in either supported descriptor order
  where the syntax is otherwise valid.
- A candidate is eligible only when every declared technology is currently
  supported by the native font loader and at least one declared format is
  supported. The current native technology set is intentionally empty because
  variable/color technology negotiation is not implemented yet.
- Unsupported format or technology candidates are skipped without a fetch or
  local-font lookup. If every candidate is skipped, `FontFace.load()` rejects
  with a bounded `NetworkError`; a later eligible source can still succeed.
- Empty values, unbalanced/quoted functions, unknown trailing descriptors, and
  malformed descriptor lists reject explicitly as bounded syntax errors.
- Eligible candidates retain Slice 498's ordered fallback and Slice 497's
  transactional host acknowledgement. No unsupported technology is admitted
  into the native font book.

## Implementation

- Parse a bounded sequence of `format()`/`tech()` descriptors with
  quote/parenthesis-aware closing and list handling.
- Keep the native supported-technology set explicit and fail closed for
  technologies not yet implemented by the renderer.
- Extend the source-list witness with an unsupported format and a color-font
  technology requirement before the valid TrueType fallback.

## Tradeoffs and remaining scope

Fail-closed technology filtering can skip a font that a future native renderer
could support, but it prevents incorrect rendering and preserves fallback
behavior. The empty technology set is a truthful capability boundary, not a
claim that color or variable fonts work. Technology aliases, richer CSS
escape/tokenization, `tech()` negotiation for variable/color tables, installed-
font discovery, and complete FontFace/Web IDL parity remain issue #40 gates.

The parser still accepts only the bounded descriptor grammar used by this
source-list path. Service Worker interception, final URL/response metadata,
and the existing content-process event/network boundaries remain unchanged.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Observed results: the scoped library check passed; the focused native FontFace
group passed 8 tests; and the broader font-related group passed 34 tests on a
serial run. Formatting and diff checks passed before the task documentation
was added. The documentation gates are run after this task metadata is added.

The implementation remains local-only at this checkpoint: it is not pushed,
run in remote CI, released, tagged, or published.
