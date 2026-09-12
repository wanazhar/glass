# Native engine browser slice 247: file-input actions

Status: completed locally.

## Objective

Make file selection a real native browser action across local documents,
HTTP(S) content workers, frame-aware runtime routing, CLI, and MCP. A caller
must be able to select bounded regular files on a native `input[type=file]`
and observe the same browser-owned file objects and form events from the page
realm.

## Contract

- `NativeFile` copies a regular file into an engine-owned object containing a
  leaf filename, bounded media type, modification time, and bytes. One action
  accepts 1..=16 files, no file exceeds 4 MiB, and the total payload is capped
  at 8 MiB.
- File names and bytes cross the content-process boundary as validated typed
  data; filesystem paths never enter the page realm or worker IPC.
- Native preflight resolves a unique enabled `input[type=file]`. Hidden file
  inputs remain valid upload targets, disabled controls reject, and more than
  one file is rejected unless the control has `multiple`.
- The local and HTTP(S) native owners commit file selection once, advance the
  document revision once, and dispatch `input` followed by `change` through
  the existing JavaScript event transaction/content-worker bridge.
- The page realm exposes `FileList` and `File` objects with `length`, indexed
  access, `item()`, iteration, `name`, `type`, `size`, `lastModified`,
  `text()`, `arrayBuffer()`, `bytes()`, and `slice()` behavior for selected
  files. `input.value` exposes the browser-style `C:\\fakepath\\name` value,
  while non-empty script assignment remains an `InvalidStateError`.
- Constructing `FormData` from a form containing a selected file returns a
  file-valued entry with the selected name, metadata, and bytes. The existing
  bounded multipart serializer is prepared to consume those entries.
- CLI and MCP upload paths require the upload policy capability, validate the
  authorized regular paths before reading, preserve optional expected
  revisions, and use the same exact frame route as other native actions.

## Implementation

`NativeFile` lives in the native interaction contract and owns path loading,
MIME inference, printable leaf-name validation, base64 serde, and aggregate
limits. `NativeElementState` and both document wire snapshots retain selected
files without projecting their contents into semantic text or attributes.
`NativeDocument::apply_upload` owns target validation, multiple-file policy,
fake-path value state, and event ordering.

The engine adds upload to synchronous local actions and asynchronous
content-worker form mutations. The native backend and runtime perform
revision-checked, unique preflight and preserve root, parked-frame, and
content-process effect handling. CLI and MCP convert authorized paths into
`NativeFile` objects at their public boundaries.

The JavaScript host installs bounded `FileList` and `File` projections and
refreshes them from every committed snapshot. File-valued `FormData` entries
retain binary bytes, while ordinary file input script assignment can only
clear the control through the typed host command.

## Tradeoffs and follow-up

The engine copies files into memory to avoid leaking host paths and to make
local and worker-backed documents share one deterministic contract. The
per-file and aggregate limits bound memory, IPC, snapshot, and page-realm
cost, at the expense of rejecting very large uploads until a streaming owner
is added. File selection works for hidden controls because a browser file
picker is not a pointer hit-test; disabled and malformed targets still fail
before mutation.

This slice makes file objects and `FormData` observation real, but does not
yet claim that native form navigation or `fetch` can deliver binary multipart
bodies over the resource loader. The next transport slice must move bounded
binary request bodies through navigation/fetch, preserve content type and
redirect policy, and add server-observed upload coverage.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine upload -- --nocapture` (2 passed, 0 failed)
- `git diff --check`

Remote CI, push, release, tag, registry publication, and final production
parity claims are not made by this local checkpoint.
