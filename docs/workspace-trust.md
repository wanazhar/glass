# Workspace trust

Glass treats repository configuration as untrusted input until a local user
makes an explicit decision. Opening an unfamiliar repository is therefore a
static inspection operation, not permission to execute its code.

## States

```text
Untrusted
├─ static files, search, project detection, and Git metadata
├─ configuration and project-skill inspection
└─ manual browser use that does not start project code

TrustedOnce
└─ executable project configuration for this workspace lifetime only

TrustedProject
└─ executable project configuration after an identity-bound external record
```

`TrustedOnce` is never written to disk. `TrustedProject` is stored under the
platform-local Glass data directory in `glass/trust/workspaces-v1.json`, not
inside the repository. The record binds the canonical root to filesystem
identity and the observed Git remote when present. Replacing the directory at
the same path fails closed and requires a new decision. If the platform cannot
provide enough identity evidence, persistent trust is rejected and the user
may choose trust-once.

A repository cannot trust itself through `glass.toml`, `.glass.toml`, a skill,
an environment value, or a custom tool. Unknown trust fields in project
configuration are rejected.

## Before trust

Untrusted mode blocks repository-controlled execution, including:

- `workspace.opened` and other project hooks;
- project commands and shell-backed custom tools;
- configured tests, LSP commands, and DAP commands;
- PTYs, kernels, Pi workers, Neovim, and experiment worktrees;
- file/Git mutations through development-agent routes; and
- compatibility routes exposed by the legacy project CLI/MCP catalog.

A custom tool's `mutating = false` value affects post-trust confirmation UX;
it never makes an arbitrary shell command safe to run before trust. `--yolo`
removes per-operation confirmations only after trust and cannot elevate an
untrusted workspace.

Read-only trust APIs are available through the authoritative tool router:

```text
glass.workspace.trust.status
glass.workspace.trust.inspect
```

Inspection reports the exact source and command plus `static`, `agentContext`,
or `executable` risk and `userGlobal`, `trustedProject`, or
`untrustedProject` authority. MCP, daemon clients, Pi workers, kernels, custom
tools, and repository hooks can inspect but cannot mutate trust.

## TUI decision

When executable repository configuration is present, desktop, compact, and
phone layouts open on the Trust surface before activation:

```text
┌─ Workspace Trust ─────────────────────────────────────┐
│ This repository contains executable Glass settings.  │
│                                                       │
│ [I] Inspect configuration                             │
│ [O] Open untrusted                                    │
│ [1] Trust once                                        │
│ [T] Trust this project                                │
└───────────────────────────────────────────────────────┘
```

The command palette equivalents are `trust inspect`, `trust untrusted`,
`trust once`, and `trust project`. Only this local-human path can apply a new
decision. Opening untrusted keeps safe file/browser review available without
leaving the TUI.

## Skills and experiments

User-global skills are labelled `userGlobal`. Project skills remain visible
as `untrustedProject` instructions but are excluded from Pi's privileged
system context until trust. Once active, they are labelled `trustedProject`
with their exact source path.

Creating experiment worktrees requires a trusted parent. The explicit current
policy grants each child `TrustedOnce` for the owning experiment-manager
lifetime; it does not persist a new project trust record or cause unrelated
clones/worktrees to become trusted.

Daemon reconnect preserves the workspace's existing state. It has no protocol
operation that upgrades trust. A new daemon open re-evaluates the external
identity-bound store.

Workspace trust governs Glass capabilities but is not an operating-system
sandbox. Trusted project commands, language servers, debuggers, Pi, Python,
JavaScript `vm`, shell kernels, and kernel-issued Glass bindings execute with the launching user's OS
permissions. Use a container, VM, or restricted OS account for hostile code.
