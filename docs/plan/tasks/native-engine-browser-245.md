# Native engine browser slice 245: frame-aware semantic ownership

Status: completed locally.

## Objective

Make native semantic inspection and execution preserve browsing-context
ownership across the selected frame subtree. A target discovered inside an
embedded document must retain the frame identity needed for preflight,
action dispatch, and popup causality.

## Contract

- Native semantic inspection returns the selected frame and all attached
  descendants under one backend reconciliation boundary, with bounded,
  deterministic regions, text, and target counts.
- Every native target from a child frame carries its owning `frameId`; the
  optional field keeps existing single-document observation payloads
  compatible.
- The aggregate observation revision changes when any attached frame's local
  revision or frame topology identity changes, so intent resolution remains
  bound to one observed frame tree.
- Native preflight searches the selected frame subtree, rejects duplicate
  matches, and returns the exact owning frame on a unique match. A reference
  that is valid only in another frame is a structured stale result rather
  than a hard error in the wrong frame.
- Semantic `Click`, `Type`, `Clear`, `Check`, `Uncheck`, and `Select` execute
  through the candidate's exact frame route. Popup clicks use the same fresh
  frame ownership instead of re-resolving into the root document.
- Child action effects continue through the existing native event,
  frame-script, navigation, popup, and browser-effect coordinators; the
  public target context remains unchanged.

## Implementation

`NativeEngineBackend::inspection_snapshots` captures each live frame engine
with its frame identity. `BrowserRuntimeSession` builds one bounded semantic
observation from those snapshots and propagates `frameId` through
`SemanticTarget` and `SemanticIntentCandidate`. A deterministic aggregate
revision covers the complete selected frame tree while each revision-bound
node reference remains local to its owning document.

Backend preflight now walks the reconciled frame subtree, records unique
matches, and annotates the winning `NativeTargetPreflight` with its frame.
`action_in_frame` verifies that frame is still attached before applying the
ordinary native action/effect pipeline. Popup intent execution uses the same
exact-frame route. Engine preflight returns a structured stale result when a
revision-valid node has no semantic control projection, preventing a parent
iframe owner from shadowing a child reference during the walk.

The integration witness discovers a named textbox inside a child frame,
resolves it through the semantic intent resolver, preflights it, executes a
type action without selecting the child publicly, and verifies the mutated
value after selecting that frame.

## Tradeoffs and follow-up

Frame-local references stay compact and revision-bound, so the frame identity
is carried as explicit metadata instead of being encoded into every locator.
The aggregate revision is a bounded observation fingerprint rather than a
single document revision; callers must still reacquire the observation after
any stale or detached result. The selected frame subtree remains the public
scope; other page targets and cross-origin DOM access are not silently
included. Full browser-level focus traversal across browsing contexts,
shadow-tree semantics, and remaining Issue #40 parity gates continue in later
slices.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_semantic_intents_discover_and_route_nested_frame_targets -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_exposes_agent_inspection_and_target_discovery -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (7 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: the local Conventional Commit containing this
task and implementation; see the repository history for its exact hash.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
