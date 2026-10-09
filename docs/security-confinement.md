# Opt-in command and extension confinement

All settings are opt-in. Omitted settings preserve the unrestricted shell and
legacy extension startup behavior.

```toml
permission_profile = "workspace-edit" # unrestricted | workspace-edit | read-only
extension_isolation = "strict"       # off | strict
```

`workspace-edit` uses the host kernel to permit child writes only under the
workspace and temporary directories. `read-only` uses a read/execute allowlist
and grants no child filesystem write rights. Both `bash` and background jobs
use the same initialized OS ruleset. If an explicitly selected profile cannot
be installed, the host refuses those commands rather than running them bare.
The existing `sandbox` setting remains separate and keeps its old default.

`sandbox::SessionApprovals` is the host integration hook for exact command
approvals. An approval lasts only for that in-memory session and matches the
whole command string. The UI must request an interactive decision before
recording it; `decision(command, true)` returns a headless refusal until the
same command has been explicitly approved. `harden_command` is the execution
hook: it skips the base jail only for that approved child and never changes
the process-wide profile. Requests that need a decision are refused before
spawn until the UI gets an answer. `security_summary` plus the label helpers
provide value-free profile/isolation status for doctor and status surfaces.

Strict extension isolation reads `<entrypoint>.capabilities.json` before it
starts the extension. Entry points, declared paths, and descriptor content
must be present and resolvable. The host's `inspect_extension_capabilities`
API validates the declaration without launching the extension, including
while isolation is off, so enable flows can review needs first. Example:

```json
{
  "filesystem": { "read": ["extensions/index"], "write": [".glass-index"] },
  "network": [],
  "subprocess": false
}
```

Paths are resolved to canonical paths before installing Landlock rules, so a
path declaration cannot be redirected later through a symlink. The descriptor
itself may not be a symlink. The interpreter/runtime and common system runtime
directories are readable; extension-specific files and data must be declared.
Extensions and their MCP grandchildren have sensitive environment variables
removed. Linux Landlock provides filesystem rights and TCP destination-port
rules when ABI 4 is available. Port rules do not filter hosts or local bind
addresses. Landlock cannot prevent process creation; grandchildren inherit the
same jail. Strict mode refuses platforms without the required confinement.

The host logs the declared paths and ports plus these enforcement limits at
extension startup. Capabilities announced by the extension handshake are too
late to authorize startup; sidecar declarations are the pre-execution source.
Permission profiles constrain shell writes, and extension sidecars additionally
constrain declared extension reads. Environment scrubbing does not hide
secrets stored in readable files. In particular, the shell read allowlist
includes runtime/build directories such as the user's Cargo and cache trees.
Treat filesystem credentials as readable unless separately protected by OS
ownership and permissions.
