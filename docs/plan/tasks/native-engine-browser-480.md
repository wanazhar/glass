# Native engine browser-complete slice 480: escaped CSS URL grammar

- Status: complete
- Scope: `native-engine` / CSS URL tokens in rooted file stylesheets
- Issue: #40
- Depends on: [native-engine-browser-479](native-engine-browser-479.md)

## Objective

Make rooted file stylesheets resolve ordinary CSS escape sequences before the
existing URL and file-root owners interpret a resource target.

## Contract

- Hexadecimal CSS escapes, simple escaped characters, and line continuations
  in `url(...)` and literal `@import` targets decode before URL resolution.
- Escaped closing parentheses do not terminate an unquoted `url(...)` token.
- Invalid trailing escapes fail closed; decoded NUL values become the CSS
  replacement character and cannot be used to smuggle a filesystem path.
- Non-file stylesheet owners retain their existing loader and parser path.

## Implementation

- Add one bounded CSS URL escape decoder shared by URL-token rewriting and
  rooted-file stylesheet import resolution.
- Make the URL-function scanner honor escaped delimiters outside quotes.
- Exercise escaped stylesheet imports and background-image URLs through a
  rooted-file integration page, alongside focused decoder coverage.
- Update the native-engine architecture, plan, analysis, and issue evidence.

## Tradeoffs and remaining scope

The decoder is intentionally limited to URL-token grammar and does not change
general CSS selector/string parsing. CSS import media/layer/supports
evaluation, file fonts and other CSS resource types, network stylesheet
URL-base parity, complete file-origin semantics, and full Web IDL parity remain
issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked css_url_escape_decoder`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_document_loads_rooted_script_stylesheet_and_image`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
