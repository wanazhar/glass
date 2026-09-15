# Native timer-probe host-clock transport (383)

```yaml
id: native-engine-browser-383
scope: native-engine/timer-probe-transport
status: done
depends-on:
  - native-engine-browser-382
```

## Objective

Remove the remaining dynamic host-clock interpolation from native timer-delay
inspection. The page and worker owners must receive the current monotonic clock
through the existing host boundary and run fixed inspection programs, so a
clock value cannot expand or alter generated JavaScript source.

## Contract

- The page and worker probes compute the same bounded minimum delay as before.
- The host clock remains monotonic, realm-local, and owned by Rust.
- Missing or invalid timer records remain ignored by the existing probe rules.
- The page and worker timer queues, ordering, timeout bounds, and serialized
  JavaScript owner are unchanged.
- The direct numeric clock handoff is internal host state; it is not a new
  page API or a promise of browser-wide task-source scheduling.

## Delivered behavior

- `next_timer_delay_ms()` passes the current clock as a native numeric value and
  evaluates a fixed page probe.
- `next_worker_timer_delay_ms()` uses the same handoff and a fixed worker probe.
- The source formatter that embedded `{now_ms}` in both probes is removed.
- A focused unit witness covers page and Service Worker timer-delay inspection
  and asserts that the probe source has no interpolation marker.
- Current architecture, analysis, and delivery-plan checkpoint references are
  synchronized to slice 383.

## Tradeoffs

The owner performs one small host-property write before each delay probe. This
adds a negligible boundary operation, but removes source construction and
avoids treating a host clock as authored JavaScript. The internal property uses
the existing realm namespace and is overwritten on every probe; it does not
change page-observable timer behavior or protect page-owned timer maps from
page mutation. Full task-source arbitration, resident background scheduling,
and Core Web Profile certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-383.md`

## Verification

The following checks passed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib native_timer_probe_tests::timer_delay_probes_receive_host_clock_without_source_interpolation --locked -- --nocapture` — 1 passed
- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-383.json >/dev/null`
- `python3 scripts/check-documentation-depth.py >/dev/null`
- `python3 scripts/check-tui-shortcuts.py >/dev/null`
- `python3 scripts/check-documentation-coverage.py >/dev/null`
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
