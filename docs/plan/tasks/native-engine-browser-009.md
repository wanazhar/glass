---
id: native-engine-browser-009
scope: glass-browser/native-engine/content-process-sandbox-launch
status: done
depends-on: [native-engine-browser-008]
---

# BE-01h: OS-specific content-process sandbox launch

## Objective

Make the native content worker launch through an explicit operating-system
containment policy. The worker no longer starts as an ordinary unsandboxed
child: Linux uses Bubblewrap, macOS uses Seatbelt, and Windows uses a Job
Object with lifetime and process-count limits. If the required policy cannot
be constructed, startup fails with `SandboxUnavailable` rather than silently
running the helper with weaker privileges.

This is a launch and containment baseline, not the final hostile-web security
milestone. Origin/site isolation, network policy, syscall/file capability
minimization, quotas, and a full cross-platform security review remain open.

## Contract

- Linux requires `bwrap` discoverable on `PATH`. It creates a parent-death,
  new-user/new-PID/new-UTS/new-IPC sandbox, mounts the worker and required
  system/runtime roots read-only, gives `/tmp` a private tmpfs, applies
  `PR_SET_NO_NEW_PRIVS`, and shares only the network namespace needed for the
  current HTTP(S) loader.
- macOS requires `/usr/bin/sandbox-exec` and launches a deny-by-default
  Seatbelt profile with read-only system/runtime access and outbound network
  permission for document loading.
- Windows creates a Job Object before spawn, applies
  `KILL_ON_JOB_CLOSE | ACTIVE_PROCESS_LIMIT`, assigns the worker immediately
  after spawn, and retains the job handle for the engine lifetime.
- Sandbox setup failure is a typed `NativeWorkerFailureKind::SandboxUnavailable`
  error. There is no implicit unsandboxed fallback or CDP retry.
- The existing worker protocol, timeout poisoning, and explicit navigation
  recovery remain unchanged; sandbox state is coupled to each fresh worker.

## Tradeoffs and missed behavior

- Linux still shares the network namespace because the current child owns
  external HTTP(S) loading. Network mediation, DNS policy, TLS/origin rules,
  and per-origin capability isolation belong to BE-02 rather than being hidden
  inside this launcher.
- Bubblewrap and macOS Seatbelt are host/runtime prerequisites. This makes
  failure explicit and safer, but packaging and clean-install gates must ship
  or document those prerequisites on every supported platform.
- The Windows Job Object bounds process lifetime/count but is not a complete
  filesystem, token, network, or syscall sandbox. A later Windows containment
  task must add the missing restricted-token and capability policy.
- No sandbox can compensate for the current bounded parser and absence of
  JavaScript/event-loop, subresource, storage, CORS/CSP, or origin isolation.
  Production promotion still requires BE-02 through BE-09 and the M1-M9 gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/sandbox.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The sandbox-launch slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_transfers_computed_style_for_layout --locked -- --nocapture`

The affected check passed without warnings, and the helper-backed external
layout test passed 1/1 through the Linux Bubblewrap launch path. Full
cross-platform sandbox execution, origin/site isolation, network mediation,
restricted Windows tokens, diagnostics transfer, script execution, WPT,
security review, and browser promotion remain open.
