---
id: native-engine-browser-044
scope: glass-browser/native-engine/form-ownership
status: done
depends-on: [native-engine-browser-043]
---

# BE-03v/BE-04z: bounded external form ownership

## Objective

Make the bounded form contract observe the HTML `form` attribute for controls
that are not descendants of their owning form, including external submitters.

## Contract

- `button`, `input`, `select`, and `textarea` controls with `form="id"` are
  associated with the form element whose `id` matches exactly.
- An explicit `form` attribute takes precedence over ancestor ownership; a
  missing or unresolved reference does not silently select an ancestor form.
- Associated controls participate in the existing bounded required-control
  validation and successful-control serialization in document order, whether
  they occur before, inside, or after the form element.
- An external submit button is accepted by `requestSubmit(button)` and by the
  typed parent/content-process navigation handoff.
- The existing method and encoding boundary remains unchanged: GET and
  `application/x-www-form-urlencoded` POST are supported; multipart,
  `text/plain`, target contexts, image coordinates, and full constraint
  validation remain separate gates.

## Deliberate boundary and tradeoffs

The implementation derives ownership from the current parsed document rather
than adding a second association index. This keeps the two-crate boundary and
the typed owner path unchanged, at the cost of a bounded document scan per
form submission/validation. The scan is capped by the existing form-control
limit and preserves the arena's document order.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_form_attribute_associates_external_controls_in_document_order`
- `native_content_process_form_attribute_associates_external_controls`
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
