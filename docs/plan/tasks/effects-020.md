id: effects-020
scope: bounded browser effect witnesses
status: done
depends-on: [verify-020]

## objective

Populate generic action verification with bounded observed route, URL/title,
popup, dialog, and download effects while preserving compatibility JSON when
no new effect is observed.

## path

- `src/browser/session/action.rs`
- `src/browser/session/types.rs`
- GitHub issue #20

## verification

- Existing compact action-result serialization remains stable when all effects
  are false.
- Newly observed effects serialize in camelCase and contain no page contents.
- No remote push, tag, or publication occurs.
