# Reasoning reader and unrestricted file access

## Findings and changes

The inline conversation reader could repaint while the fullscreen live reader was open. Input and resize routing also allowed the underlying reader to act first. Fullscreen live views now own drawing, navigation and geometry until closed; the inline reader restores its frame afterwards. Reasoning uses a padded layout on wider screens, with matching navigation bounds. Ctrl+C closes the live view before cancellation.

Native file traversal previously stripped every absolute path against the workspace, regardless of the permission profile. Unrestricted mode with sandboxing off now accepts outside-workspace paths. Descriptor-relative traversal, no-follow opens, bounded reads and revision-checked replacement remain in place. CLI file operations load the active profile and administrator requirements; mutation and sandbox checks are shared at the execution boundary.

Bash writes to `/proc`, `/sys` and `/dev` are control operations rather than document replacement. Only unrestricted mode with sandboxing off exempts them from the document read-before-write guard. Shell policy and OS permissions still apply. Tool activity labels outside-workspace native calls and system-control writes.

Restricted native access remains a denial with a permission explanation. This change does not add a per-path approval dialog or silently weaken a restrictive profile.

## Review and verification

One GPT-6 Luna reviewer at medium reasoning reviewed the implementation and a follow-up pass. A stale write-schema description was corrected. No blocking issue remained in the final review.

The workspace suite passed 923 unit tests; one existing doctest remained ignored. Clippy passed with warnings denied. PTY fixtures cover reasoning opened over a scrolled conversation, navigation, resize, continued tools, preserved drafts and cancellation at five sizes down to 20×8. Isolated CLI fixtures cover external absolute/relative paths, revision-guarded writes, restrictive confinement, read-only mutation denial and symlink refusal.

Kernel-control behavior is checked without executing a cache-drop or another host-control command.
