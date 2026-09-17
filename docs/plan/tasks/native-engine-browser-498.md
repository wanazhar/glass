# Native engine browser-complete slice 498: FontFace source-list fallback

- Status: complete
- Scope: page-realm `FontFace` source-list selection and `format()` filtering
- Issue: #40
- Depends on: [native-engine-browser-497](native-engine-browser-497.md)

## Objective

Let script-created `FontFace` sources use the ordered CSS source-list contract
instead of treating only the first `url()` or `local()` expression as the
source. A candidate that is unavailable, rejected by the native loader, or
declares an unsupported format must allow the next bounded candidate to run.

## Contract

- The page parser accepts a bounded comma-separated list of `url()` and
  `local()` candidates while preserving source order and quoted commas inside
  functions.
- A `format()` descriptor is parsed per candidate. A candidate is eligible when
  at least one declared format is supported by the native font loader; a list
  containing only unsupported formats is rejected as a network failure.
- Candidates are tried in order. Missing local faces, failed URL responses,
  unsupported response media types, empty bodies, oversized bodies, and native
  inline-resource failures advance to the next candidate. The final candidate
  error is returned when no candidate succeeds.
- Successful bytes still cross the existing bounded `FontFaceInstall` command
  and host acknowledgement path. Admission rejection therefore retains Slice
  497's transactional error behavior and never partially mutates the font book.
- The source-list parser and candidate loader remain bounded at 32 entries;
  malformed functions, empty entries, malformed `format()`, and unsupported
  trailing descriptors fail explicitly.

## Implementation

- Add quote- and parenthesis-aware source-list splitting with a bounded entry
  count and explicit malformed-list errors.
- Parse normalized `format()` names against the formats supported by the native
  font loader, skipping candidates that have no supported format.
- Extract candidate byte loading into a shared local/inline/HTTP path and use a
  bounded Promise fallback chain for candidate failures.
- Add a Linux system-font witness where a missing first candidate and a
  supported `format('truetype')` fallback load the second candidate and produce
  one admitted native font resource.

## Tradeoffs and remaining scope

Candidate fallback can perform more than one bounded local lookup or network
request, which matches the ordered source contract but may increase latency for
long lists. The 32-entry bound and existing fetch/font byte limits keep that
cost finite. The implementation recognizes the formats currently admitted by
the native loader; `tech()` descriptors, richer CSS escape/tokenization,
ArrayBuffer/ArrayBufferView constructor sources, full variable/color font
format negotiation, installed-font discovery, and complete FontFace/Web IDL
parity remain issue #40 gates.

Source failures are accumulated only as the last native error, so the page
surface does not expose every candidate's diagnostic. The existing page-script
owner handoff, Service Worker interception boundary, final URL and response
metadata fidelity, and standalone content user-event network boundary remain
unchanged.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Observed results: the scoped library check passed; the focused native FontFace
group passed 7 tests; the broader font-related group passed 33 tests on a
serial run; and the content-process destination witness passed 1 test.
Formatting and diff checks passed before the task documentation was added.
The documentation gates are run after this task metadata is added.

The implementation remains local-only at this checkpoint: it is not pushed,
run in remote CI, released, tagged, or published.
