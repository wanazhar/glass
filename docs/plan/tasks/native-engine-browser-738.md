id: native-engine-browser-738
scope: glass-browser/one-shot-native-cli-dialog-host
status: done
depends-on: [native-engine-browser-737]

# Glass native-engine browser slice 738: one-shot CLI dialog host

## Objective

Make process-backed JavaScript `alert`, `confirm`, and `prompt` usable from
one-shot commands selected with `--browser-runtime native`, without blocking
the browser command queue or changing non-interactive automation behavior.

## Contract

- Enable modal dialogs only when the selected runtime is native and stdin is a
  terminal. Non-terminal and persistent-session commands keep their existing
  non-modal behavior.
- Poll the session's cloneable dialog controller while the original command
  future is suspended. Resolve the exact pending dialog ID before allowing the
  command to continue.
- `alert` acknowledges with Enter; `confirm` accepts on `y`/`yes` and dismisses
  on `n`/`no`/empty input; `prompt` submits the exact edited line, preserves the
  page default on empty input, and dismisses on EOF or `:cancel`.
- Bound prompt input to 256 UTF-8 bytes. Bound and drain oversized input without
  retaining the full line. Keep JSON command output on stdout and dialog UI on
  stderr.
- Escape page-controlled terminal control characters and do not print the
  dialog's source URL, which may contain credentials or tokens.
- Do not claim `beforeunload`, persistent-session, MCP, or general native
  browser parity as completed by this slice.

## Tradeoffs

The host polls the already thread-safe controller at a 50 ms interval instead
of introducing a second notification mechanism into the browser worker. This
adds a small amount of idle wakeup during one-shot commands and bounds dialog
detection latency. Interactive input is only enabled for terminal stdin;
automation and piped invocations cannot hang waiting for a human response.

## Path

- `crates/glass-browser/src/cli/runner.rs`
- `docs/cli.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-738.md`

## Verification

- Unit coverage for accept/dismiss/prompt/default and invalid confirm input.
- Byte-bound and UTF-8/CRLF line-reader coverage; terminal-control escaping.
- Scoped `glass-browser` test-target check and affected tests, plus formatting
  and documentation inventory/truth gates.
- Remote CI and cross-platform native certification are not part of this local
  slice unless explicitly run and recorded.

## Result

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- The three `native_cli_dialog` unit tests passed. The existing
  `modal_dialog_wait_pauses_the_content_operation_deadline` regression also
  passed (21.92 s).
- `cargo build -p glass-dev --bin glass --locked --quiet` passed.
- A local-HTTP PTY run displayed prompt, confirm, and alert in sequence,
  accepted `Grace`, `y`, and Enter respectively, displayed `Saved Grace`, and
  completed navigation with exit code 0. An earlier manually stepped PTY run
  hit the content-process deadline; the same sequence passed with its three
  responses queued before navigation. The deadline regression above passed;
  the first-run timing discrepancy is retained here as evidence, not hidden.
- `cargo fmt --all -- --check`, `git diff --check`, documentation coverage
  (1,366 Markdown files), documentation depth (93 guides/19 contracts), TUI
  shortcuts (15 keys/63 markers), and release-documentation truth (zero
  current-claim failures) all passed.
- Remote CI and cross-platform certification were not run. The slice remains
  local and does not close issue #40.
