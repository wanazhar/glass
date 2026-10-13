# Permissions and security

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Permission model

The default permission profile is `unrestricted`, and the default `sandbox` policy
is `off`. Model-requested shell commands therefore run with the user's access
unless another configured policy restricts them.

Security controls have separate owners:

| Control | Scope |
| --- | --- |
| Global policy | Provider, endpoint, and shell-text denials that project settings cannot relax |
| Administrator requirements | Required settings above user, project, and runtime choices |
| Project trust | Consent for loading repository instructions |
| Permission profile | Base filesystem confinement for supported child processes |
| Session command approval | An explicit exception for one exact command in the current session |
| Sandbox policy | Additional configured execution and network restrictions |
| Extension isolation | Validated capabilities applied before extension startup |

Open `/security` or `/permissions` to inspect the effective policy. `gc policy
explain COMMAND` evaluates configured rules without executing the command. Shell
rules match command text, not a complete shell syntax tree; use OS confinement
for filesystem and network boundaries.

`workspace-edit` and `read-only` are opt-in permission profiles. Explicit profiles
refuse execution if their required confinement cannot be installed. Session
approvals are held in memory and cleared when changing conversations. Headless,
delegated, and extension host-tool calls do not inherit an interactive exception.

Read [Confinement details](security-confinement.md) before relying on a profile or
capability declaration. It documents pinned paths, platform support, credential
filtering, and protocols that network rules do not cover.

## Optional sandbox

The separate `sandbox` setting accepts `off`, `workspace`, and `strict`:

```toml
sandbox = "workspace"
sandbox_ports = [443, 80]
```

`workspace` narrows child writes to configured workspace, temporary, application,
and runtime paths. `strict` also narrows reads and execution. Commands requiring
paths outside those rules can fail; inspect the active policy before granting an
exception. Explicit permission profiles have their own failure behavior and path
rules, documented in [Confinement details](security-confinement.md).

Linux uses Landlock. TCP destination-port rules require ABI 4; the sandbox reports
when network confinement is unavailable. Port rules do not filter destination
hosts, local binds, UDP, or Unix sockets. macOS uses `sandbox-exec`; its network
support differs. Strict requests refuse unsupported confinement rather than
silently executing without it.

Environment filtering removes common credential variables from relevant child
processes. `sandbox_env = "inherit"` requests inheritance. Filtering does not hide
credentials in readable files. Extensions are trusted local programs unless the
applicable isolation policy constrains them; strict extension startup validates
capability declarations before launching executable code.

New session logs and spills are private on Unix. Existing files retain their
permissions. Output redaction is best-effort and may leave original data in spill
files. Review diagnostics and exports before sharing them.
