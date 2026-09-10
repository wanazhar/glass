---
id: native-engine-browser-142
scope: glass-browser/native-engine/navigation/external-link-activation
status: completed
depends-on: [native-engine-browser-141]
---

# Native engine browser slice 142: direct external-link activation

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make an ordinary semantic click on an anchor in an external HTTP(S) native
document follow the asynchronous native navigation path while preserving the
content-process click event contract and fail-closed cancellation behavior.

## Contract

- direct native anchor clicks remain in the content-process event bridge;
- click listeners can cancel the default navigation through `preventDefault`;
- the default-action decision crosses the bounded IPC response explicitly;
- an allowed external link resolves relative to the committed document and is
  handed to the asynchronous native navigation owner;
- the click mutation and resulting navigation each retain their existing
  revision/lifecycle/history/request-accounting semantics;
- no content worker is discarded merely because the clicked link is external;
- no CDP fallback, hidden browser process, or third crate is introduced.

## Tradeoffs

The content process continues to own click dispatch and the parent continues
to own document navigation. This preserves the hostile-content boundary and
keeps network policy, history, and request accounting in one parent owner, at
the cost of two bounded revision transitions for a successful link click: one
for the click event and one for navigation. A canceled click still records the
observable event mutation but never starts the navigation request.

This slice handles the default same-target anchor path only. Download
attributes, popup or new-target behavior, frame-targeted navigation, and
per-resource network events need separate owners and are not silently inferred
from a normal link.

## Implementation surface

- `browser/native_engine/content_process.rs`: transfer the click default-action
  decision in the typed mutation response;
- `browser/native_engine/engine.rs`: preserve content-process ownership and
  route allowed links through asynchronous navigation;
- `tests/native_engine.rs`: direct external semantic-click regression;
- synchronized architecture, analysis, and plan records.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_direct_click_owns_external_link_navigation -- --exact --nocapture
```

Completed evidence:

- formatting completed successfully;
- the feature-gated `glass-browser` test-target check passed;
- direct external anchor activation passed against a local HTTP server, with
  the destination committed and no Chromium endpoint;
- the existing script-driven external-link and content-process regression
  coverage remained available;
- no CDP fallback, browser process, or third crate was introduced.

Download targets, popup/new-target behavior, child-frame ownership,
per-resource lifecycle events, universal workflow parity, and native
production promotion remain issue #40 work.
