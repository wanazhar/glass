---
id: native-engine-browser-744
scope: glass-browser/linux-content-worker-nested-userns-hardening
status: done
depends_on: [native-engine-browser-743]
---

# Glass native-engine browser slice 744: deny nested Linux user namespaces

## Objective

Prevent Linux page-content workers from creating another user namespace
inside the Bubblewrap boundary. Such a nested namespace can provide additional
ways to manipulate mount and process isolation; the sandbox must both disable
that operation and assert that the restriction is active before executing
untrusted content.

## Contract

- Linux Bubblewrap launch includes `--disable-userns` and
  `--assert-userns-disabled` alongside the existing isolated user/PID/UTS/IPC
  namespaces. If Bubblewrap does not support or cannot enforce either option,
  worker startup fails closed; never retry unsandboxed.
- Keep outbound networking available in the current worker boundary: the
  content process still owns the bounded native resource loader. Moving that
  loader into a separate browser-process network broker is a distinct security
  workstream, not something this slice can imply.
- Prove the host can create the baseline nested namespace, then prove the
  restricted sandbox denies it. Also assert the production worker command
  includes both deny and assert controls.
- Do not claim complete Linux sandboxing, origin/site isolation, or macOS and
  Windows hardening from this defense-in-depth change.

## Tradeoffs

Bubblewrap 0.8.0 introduced these controls. Linux installations with an older
Bubblewrap will now fail the native content-worker startup rather than run
with a weaker namespace boundary. Packaging and clean-install validation must
provide a compatible Bubblewrap version. macOS Seatbelt and Windows Job Object
policies are unchanged.

## Paths

- `crates/glass-browser/src/browser/native_engine/sandbox.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-744.md`

## Verification

- Run `cargo check -p glass-browser --lib --locked --quiet` before the focused
  Linux sandbox unit test.
- The focused test must verify baseline namespace creation, denial inside the
  hardened sandbox, and the production command-line controls.
- Keep the existing process-backed browser test evidence for normal worker
  startup/navigation, so hardening does not silently break resource loading.
- Run formatting, `git diff --check`, and the repository documentation truth,
  depth, shortcut, and coverage checks. Record Linux-local evidence separately
  from remote CI and other-OS certification.

## Evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed on Linux.
- `cargo test -p glass-browser --lib --locked --quiet linux_content_sandbox_blocks_nested_user_namespaces -- --nocapture` passed. Its unrestricted control created a nested user namespace; the Bubblewrap deny-and-assert launch prevented it, and the production worker arguments contain both controls.
- `cargo test -p glass-browser --test native_engine --locked --quiet native_content_process_transfers_computed_style_for_layout -- --nocapture` passed through the hardened worker launch, preserving ordinary process-backed loading/layout.
- Formatting, documentation truth/depth/coverage, TUI shortcut inventory, and `git diff --check` passed locally.
- Evidence is Linux-local only. Remote CI, Windows/macOS sandboxing, complete origin/site isolation, and browser-profile completion remain issue #40 gates.
