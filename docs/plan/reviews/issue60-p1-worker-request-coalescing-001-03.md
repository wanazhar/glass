# Issue #60 P1 TUI selection-preview review 03

## Scope

Reviewed `a713d899a72108d06b01dbca2288336527f81846` against parent
`69297ede0266e6a6920f514cbee3741cd1ce9776`. Rechecked the review 02
recommendation only: the new regression for an existing absolute DAP source
outside the workspace, its state-preservation assertions, and compatibility
with the previously reviewed path guard. No implementation code was changed.

## Assessment

The regression is valid. It creates a real source file beside the temporary
workspace, so the absolute DAP path exists but is outside the workspace
(`crates/glass-dev/src/tui/state.rs:13326-13350`). It then verifies the jump
reports the containment error, leaves the surface on Debug, keeps the editor
path empty and file inventory unchanged, and creates no project buffer
(`state.rs:13351-13374`). The prior `debug_frame_source_path()` guard
canonicalizes the workspace and source and rejects a source that does not
strip the workspace prefix (`state.rs:5481-5493`); the new test exercises that
exact rejection branch without changing this reviewed code.

The task record reports the focused test
`CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib --locked debug_jump_rejects_absolute_source_outside_workspace`
passed (1 test). I did not rerun Cargo, as requested. `git diff HEAD^ HEAD
--check` passed.

## Conclusion

**PASS.** The prior review 02 coverage recommendation is addressed. No
conflict with the reviewed containment implementation or remaining blocker
was found.
