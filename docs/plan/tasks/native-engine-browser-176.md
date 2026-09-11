# Native engine browser slice 176: animation frames and intersection observation

Status: completed locally.

## Objective

Extend the existing native page-realm scheduler and Rust-owned layout snapshot
with the browser-facing animation-frame, performance-timing, and viewport
intersection observation surfaces used by interactive pages.

## Contract

- `requestAnimationFrame(callback)` queues a bounded callback for the next
  native host frame turn and passes a monotonic realm timestamp.
- `cancelAnimationFrame(id)` removes a queued callback without affecting timer
  IDs or callbacks already running.
- `performance.now()` reports the current monotonic realm clock and
  `performance.timeOrigin` identifies the same navigation-scoped clock.
- `IntersectionObserver` accepts bounded numeric thresholds, `Element` or
  `Document` roots, and pixel `rootMargin` values; invalid options fail with a
  typed JavaScript error.
- Intersection entries expose `time`, target/root/bounding/intersection
  DOMRects, `isIntersecting`, `intersectionRatio`, `isVisible`, and
  `IntersectionObserverEntry` identity.
- Initial observations and threshold crossings are queued at the existing
  Promise-job checkpoint. Root scrolling re-evaluates target visibility while
  unchanged threshold state does not create duplicate records.
- Observer registrations, callback queues, and animation-frame queues remain
  bounded by the existing native effect limits and survive same-document host
  refreshes.
- Local, content-worker, and same-origin projected realms use the same
  Rust-owned geometry snapshot and existing content-worker scroll
  synchronization; no JavaScript layout owner is introduced.

## Implementation

- Added persistent animation-frame state beside the existing bounded timer
  maps, with a 16 ms next-frame deadline and cancellation.
- Added the realm `performance` projection and routed frame timestamps through
  the existing host clock.
- Added bounded intersection rectangle, root-margin, threshold, and entry
  construction over the layout geometry map.
- Added constructor/prototype methods for `IntersectionObserver` and
  `IntersectionObserverEntry`, including `takeRecords`, `unobserve`, and
  `disconnect`.
- Added intersection delivery to the existing script/module Promise-job
  checkpoint and covered local initial/scroll transitions plus content-worker
  timer/geometry regressions.

## Tradeoffs

The host runs queued frame callbacks when a native evaluation turn pumps the
realm, matching the current explicit host-turn model used by bounded timers;
there is not yet an independent rendering thread. Intersection geometry is
the integer CSS-pixel layout snapshot, root margins are pixel-only, and
threshold state is sampled at host checkpoints. Fractional/composited layout,
independent vsync, full root clipping semantics, resource observers, and
browser-wide Web IDL/conformance remain issue #40 promotion gates.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_ -- --nocapture`
  (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine timer -- --nocapture`
  (3 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed)
- implementation commit `47102dd8`
