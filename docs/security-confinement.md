# Opt-in command and extension confinement

All settings are opt-in. Omitted settings preserve the unrestricted shell and
legacy extension startup behavior.

```toml
permission_profile = "workspace-edit" # unrestricted | restricted / workspace-edit | read-only
extension_isolation = "strict"       # off | strict
```

`workspace-edit` uses the host kernel to permit child writes only under the
workspace and temporary directories. Its shell network ports default to none,
unless explicitly configured. Native regular-file reads can reach outside the
workspace; outside mutations require a paused, explicit approval. `read-only` uses a read/execute allowlist
and grants no child filesystem write rights. Both `bash` and background jobs
use the same initialized OS ruleset. If an explicitly selected profile cannot
be installed, the host refuses those commands rather than running them bare.
The existing `sandbox` setting remains separate and keeps its old default.
On Linux, each base-profile path is opened with symlink resolution disabled
from a pinned root, and Landlock receives those open descriptors. Workspace
rules stay attached to the originally pinned workspace directory if its name
is later replaced. Missing optional cache or runtime directories are skipped;
an existing path that cannot be safely pinned makes an opt-in profile refuse.

`sandbox::SessionApprovals` is the host integration hook for exact command
approvals. An approval lasts only for that in-memory session and matches the
whole command string. The UI must request an interactive decision before
recording it; `decision(command, true)` returns a headless refusal until the
same command has been explicitly approved. `harden_command` is the execution
hook: it skips the base jail only for that approved child and never changes
the process-wide profile. Requests that need a decision are refused before
spawn until the UI gets an answer. `security_summary` plus the label helpers
provide value-free profile/isolation status for doctor and status surfaces.
Interactive model commands receive an explicit snapshot of the current session's
grant. Headless, delegated and extension host-tool calls do not inherit it.
Exceptions use the session workspace as their working directory; changing the
tool's `cwd` requires using the normal confined policy instead. `/bg` accepts
the same explicit session grants, and changing conversations clears them.

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

On Linux, the entrypoint and sidecar are opened beneath pinned directory
descriptors with `openat2` symlink resolution disabled. The sidecar is parsed
from a bounded in-memory snapshot of that already-open regular-file descriptor;
metadata is checked around the read, and concurrent modification is refused.
The validated entrypoint inode is included in the read rules, so replacing its
pathname with a different file does not itself grant access to the replacement
(a separate directory capability may already cover it). Relative capability
paths are resolved beneath the pinned workspace descriptor;
absolute declarations are resolved beneath a pinned filesystem root. Landlock
rules are installed from the pinned filesystem descriptors themselves. Replacing a
pathname with a symlink after validation therefore cannot redirect the rule to
another object. If safe `openat2` resolution or the required Landlock ABI is
unavailable, strict mode refuses startup. The interpreter/runtime and common
system runtime directories are readable; extension-specific files and data
must be declared.

Strict isolation constrains what the extension process can access; it does not
authenticate extension code against in-place modification of an already
declared entrypoint. Use the extension registry's separate integrity checks
when code authenticity is required.
Extensions and their MCP grandchildren have sensitive environment variables
removed. Linux Landlock provides filesystem rights and TCP destination-port
rules when ABI 4 is available. Port rules do not filter hosts or local bind
addresses. Landlock cannot prevent process creation; grandchildren inherit the
same jail. Strict mode refuses platforms without the required confinement.
These network rules constrain outbound TCP connections only; UDP, Unix sockets
and other protocols are not filtered by this policy.

The host logs the declared paths and ports plus these enforcement limits at
extension startup. Capabilities announced by the extension handshake are too
late to authorize startup; sidecar declarations are the pre-execution source.
Permission profiles constrain shell writes, and extension sidecars additionally
constrain declared extension reads. Environment scrubbing does not hide
secrets stored in readable files. In particular, the shell read allowlist
includes runtime/build directories such as the user's Cargo and cache trees.
Treat filesystem credentials as readable unless separately protected by OS
ownership and permissions.
