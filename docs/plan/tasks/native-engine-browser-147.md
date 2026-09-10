---
id: native-engine-browser-147
scope: glass-browser/native-engine/csp-frame-navigation
status: completed
depends-on: [native-engine-browser-146]
---

# Native engine browser slice 147: CSP frame navigation

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make the parent-owned frame registry enforce the effective response policy for
documents loaded into nested browsing contexts. A content worker must transfer
the bounded frame source policy to its parent, and the parent must decide
whether a frame request can initialize or later navigate without consulting
Chromium/CDP.

## Contract

- the content worker extracts the effective `frame-src`, `child-src`, or
  `default-src` source list after a successful HTTP(S) document response;
- the worker transfers that list through the versioned framed IPC response and
  the parent validates source count, text, control bytes, and aggregate size
  before retaining it;
- `frame-src` takes precedence over `child-src`, and `child-src` is used when
  `frame-src` is absent before the existing `default-src` fallback;
- the native frame registry evaluates a resolved `iframe`/`frame` URL against
  the parent document policy before initializing the child engine;
- a blocked frame still publishes one live `about:blank` child owner, keeping
  browsing-context topology stable while avoiding a request for the blocked
  document;
- a frame owner retains the embedding policy, so direct navigation, link
  activation, redirects, and history traversal cannot bypass the parent frame
  source decision;
- allowed same-origin frame navigation continues through the existing
  sandboxed content worker and is published with its loaded document;
- the shared CSP source matcher is used by both existing subresource checks and
  frame checks, preventing the worker and parent registry from making
  different decisions;
- the content-worker protocol advances to version 4 because the loaded
  document response has a new policy descriptor;
- local/data/fixture documents without response headers retain the existing
  unrestricted frame behavior; this slice does not invent HTTP response
  headers for those resources;
- frame lifecycle event parity, inherited CSP reporting, `srcdoc` policy
  nuance, full source-expression grammar, and browser-wide CSP conformance
  remain later issue #40 gates.

## Tradeoffs

The parent receives only the effective bounded source list rather than the raw
response headers. This keeps policy decisions private to the loader and avoids
leaking arbitrary header data through the IPC contract, but it does not yet
provide CSP violation reports or a complete policy inspector. The current
matcher intentionally supports the engine's existing bounded `none`, `*`,
`self`, scheme, and origin expressions; unsupported source grammar remains
fail-closed rather than silently widening access.

Blocked frames remain initialized `about:blank` owners instead of disappearing
from `listFrames`. That preserves stable frame identity and permits later
selection, while the retained embedding policy prevents a caller from using
that owner to navigate around the block. The frame registry still spends one
bounded engine allocation for the observable browsing context.

## Implementation surface

- `browser/native_engine/resource_loader.rs`: shared CSP source matcher,
  `child-src` precedence, and effective frame-policy projection;
- `browser/native_engine/content_process.rs`: version-4 loaded-response
  descriptor, bounded decode, and worker-side policy transfer;
- `browser/native_engine/engine.rs`: committed-document policy ownership,
  embedding-policy checks across navigation/link/history paths, and prepared
  navigation propagation;
- `browser/native_backend.rs`: policy-aware child initialization and retained
  parent policy for selected frame owners;
- `tests/native_engine.rs` and resource-loader unit tests: blocked-request,
  allowed-request, later-navigation, source-precedence, and topology evidence;
- synchronized architecture, analysis, plan, and package README records.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_blocks_csp -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_allows_csp_same_origin_frame -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --lib csp_frame_src_takes_precedence_over_child_src_and_default_src -- --nocapture
```

The focused blocked-frame group passes with 2 tests, the allowed same-origin
frame test passes with 1 test, and the source-precedence unit test passes.
Formatting, full native integration/library coverage, strict affected-package
lint, workspace gates, documentation validators, and remote CI remain
required before issue #40 production promotion and closure.
