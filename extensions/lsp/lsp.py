#!/usr/bin/env python3
"""glass lsp extension: diagnostics from a real language server after edits.

The compiler is the cheapest reviewer there is, and opencode proved that
putting its verdict in the tool result — before the model reads it — is
worth the plumbing. This extension manages language servers per workspace,
opens the file the model just wrote, waits for `publishDiagnostics`, and
appends the result:

    [lsp] src/app.py: +1 new, -1 resolved (2 open)
    [lsp] + src/app.py:12:5 error: undefined name 'valu'
    [lsp] - src/app.py:9:1 warning: unused import 'os'
    [lsp] src/app.py — current set:
    [lsp] src/app.py:12:5 error: undefined name 'valu'
    [lsp] src/app.py:40:1 warning: unused import 'os'

Only what changed is reported. The full set follows the delta as context so
a model is not handed the same list every turn, and an unchanged file costs
one line (`no change, 2 problems still open`) instead of the list. Every
written file in the tool result is checked, up to a stated bound.

Config at `~/.config/glass-core/lsp.json` (or `GC_LSP_CONFIG`):

    {
      ".py": {"command": ["pylsp"], "root": ".", "timeout_s": 20, "debounce_s": 0.6},
      ".rs": {"command": ["rust-analyzer"], "timeout_s": 30}
    }

`rename` and `code_actions` return previews carrying a token. Applying one
(`operation: "apply"`) writes the server's text edits, but only after every
affected file still hashes to the revision captured when the action was
computed — the same SHA-256 the host's revision-checked file edits take as
`expected`. A file that moved refuses the whole action with a reread hint
and nothing is written. This extension never sends `workspace/applyEdit` or
`executeCommand`: no unchecked channel and no server command runs on the
model's behalf. Only text edits and file creation are applied; a rename or
delete action is refused rather than half-performed. Atomic replacement
does not promise a compare-and-swap against an external writer in the final
check/rename interval.

Deliberately: servers start lazily on the first write of that language, stay
warm for the session, and are shut down on exit. A server that cannot start
is reported once and then ignored — a missing language server must never
break the edit loop. The `/lsp` command shows what is running; the `lsp`
tool asks for diagnostics explicitly.
"""

import hashlib
import json
import os
import select
import subprocess
import sys
import tempfile
import time
import urllib.parse
from collections import Counter

PROTOCOL = 1
WRITE_MARKER = '[wrote '
DIAG_CAP = 20
OUTPUT_CAP = 4096
# A tool result may name several written files; check the newest ones and say
# what was left out rather than truncating in silence.
MAX_CHECK_FILES = 5
# Total wall clock one tool_result rewrite may spend on servers. The host
# gives the rewrite 50s and a late reply is discarded, so the bound is ours.
CHECK_BUDGET_S = 25.0
FILE_CAP = 1024 * 1024
MAX_EDIT_FILES = 8
MAX_ACTIONS = 16
REREAD_HINT = 'nothing was written; reread the file and request the action again'


def as_dict(value):
    return value if isinstance(value, dict) else {}


def as_index(value, default=0):
    return value if isinstance(value, int) and not isinstance(value, bool) else default


def config_path():
    override = os.environ.get('GC_LSP_CONFIG')
    if override:
        return override
    xdg = os.environ.get('XDG_CONFIG_HOME') or os.path.expanduser('~/.config')
    return os.path.join(xdg, 'glass-core', 'lsp.json')


def load_config():
    try:
        with open(config_path(), encoding='utf-8') as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def frame(payload):
    """LSP framing: Content-Length header, CRLF, body."""
    body = json.dumps(payload).encode('utf-8')
    return b'Content-Length: %d\r\n\r\n' % len(body) + body


def uri_for(path):
    return 'file://' + urllib.parse.quote(os.path.realpath(path))


def position_index(text, position):
    """LSP (line, UTF-16 character) → index into `text`."""
    line, character = as_index(as_dict(position).get('line'), -1), as_index(as_dict(position).get('character'), -1)
    if line < 0 or character < 0:
        raise ValueError('edit positions must be non-negative integers')
    start = 0
    for _ in range(line):
        newline = text.find('\n', start)
        if newline < 0:
            raise ValueError('edit position is past the end of the file')
        start = newline + 1
    end = text.find('\n', start)
    row = text[start:] if end < 0 else text[start:end]
    if row.endswith('\r'):
        row = row[:-1]
    units = 0
    for index, char in enumerate(row):
        if units == character:
            return start + index
        units += 2 if ord(char) > 0xFFFF else 1
    if units != character:
        raise ValueError('edit position is past the end of the line')
    return start + len(row)


def apply_text_edits(text, edits):
    """Apply LSP TextEdits to `text`, or raise before anything is written.

    Ranges must not overlap and each operation needs a range and a newText;
    everything is validated up front so a rejected edit leaves the file alone.
    """
    if not isinstance(edits, list) or not edits or len(edits) > 128:
        raise ValueError('an edit list must hold 1..128 operations')
    spans = []
    for index, edit in enumerate(edits):
        if not isinstance(edit, dict) or 'range' not in edit or not isinstance(edit.get('newText'), str):
            raise ValueError(f'edit {index + 1} needs a range and newText')
        span = as_dict(edit.get('range'))
        start = position_index(text, span.get('start'))
        end = position_index(text, span.get('end'))
        if start > end:
            raise ValueError(f'edit {index + 1} has an inverted range')
        spans.append((start, end, edit['newText']))
    spans.sort(key=lambda span: (span[0], span[1]))
    for (_, previous_end, _), (start, _, _) in zip(spans, spans[1:]):
        if start < previous_end:
            raise ValueError('edits overlap')
    out = text
    for start, end, new_text in reversed(spans):
        out = out[:start] + new_text + out[end:]
    return out


def write_atomic(path, data, create=False):
    """Replace `path` with `data` through a temp file in the same directory."""
    if create:
        if os.path.lexists(path):
            raise ValueError('the path appeared while the action was being applied')
    else:
        if os.path.islink(path):
            raise ValueError('refusing to replace a symlink')
        if os.path.exists(path) and os.lstat(path).st_nlink > 1:
            raise ValueError('refusing to replace a hard-linked file')
    directory = os.path.dirname(path) or '.'
    handle, temporary = tempfile.mkstemp(prefix='.glass-lsp-', dir=directory)
    try:
        with os.fdopen(handle, 'wb') as stream:
            stream.write(data)
        os.chmod(temporary, 0o644 if create else (os.stat(path).st_mode & 0o777))
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def severity_name(value):
    return {1: 'error', 2: 'warning', 3: 'info', 4: 'hint'}.get(value, 'info')


def diagnostic_message(diagnostic):
    return ' '.join(str(diagnostic.get('message', '')).split())


def diagnostic_sort_key(diagnostic):
    start = as_dict(as_dict(diagnostic.get('range')).get('start'))
    return (diagnostic.get('severity', 3), as_index(start.get('line')), as_index(start.get('character')),
            diagnostic_message(diagnostic))


def diagnostic_key(diagnostic):
    """Identity of a problem: the same place, severity, code and source is the
    same diagnostic, however the server chose to word it."""
    span = as_dict(diagnostic.get('range'))
    start, end = as_dict(span.get('start')), as_dict(span.get('end'))
    code = diagnostic.get('code')
    if isinstance(code, dict):
        code = code.get('value')
    return (
        diagnostic.get('severity', 3),
        as_index(start.get('line')), as_index(start.get('character')),
        as_index(end.get('line'), as_index(start.get('line'))), as_index(end.get('character'), as_index(start.get('character'))),
        '' if code is None else str(code),
        str(diagnostic.get('source') or ''),
    )


def diagnostic_row(diagnostic, rel):
    """`path:line:col severity: message` — the shape every row here uses."""
    start = as_dict(as_dict(diagnostic.get('range')).get('start'))
    line = as_index(start.get('line')) + 1
    column = as_index(start.get('character')) + 1
    text = diagnostic_message(diagnostic)[:200]
    return f'{rel}:{line}:{column} {severity_name(diagnostic.get("severity"))}: {text}'


def plural(count, noun):
    return f'{count} {noun}' if count == 1 else f'{count} {noun}s'


def format_diagnostics(path, diagnostics, workspace):
    """The whole current set, errors first."""
    rel = os.path.relpath(path, workspace)
    ordered = sorted(diagnostics, key=diagnostic_sort_key)
    rows = [diagnostic_row(d, rel) for d in ordered[:DIAG_CAP]]
    if len(ordered) > DIAG_CAP:
        rows.append(f'… {len(ordered) - DIAG_CAP} more diagnostics elided')
    return '\n'.join(rows)


def diagnostic_delta(previous, current):
    """What changed between two published sets for one file.

    Returns `{'added': [...], 'resolved': [...], 'changed': [(new, old)]}`.
    A diagnostic present in both with the same wording is unchanged and is
    reported nowhere; one that kept its place but changed wording is a
    `changed` pair rather than an add plus a resolve.
    """
    before = Counter((diagnostic_key(d), diagnostic_message(d)) for d in previous)
    after = Counter((diagnostic_key(d), diagnostic_message(d)) for d in current)
    added_pairs, resolved_pairs = after - before, before - after
    reworded = {key for key, _ in added_pairs} & {key for key, _ in resolved_pairs}
    added, resolved = [], []
    for diagnostic in current:
        pair = (diagnostic_key(diagnostic), diagnostic_message(diagnostic))
        if added_pairs[pair] > 0:
            added_pairs[pair] -= 1
            added.append(diagnostic)
    for diagnostic in previous:
        pair = (diagnostic_key(diagnostic), diagnostic_message(diagnostic))
        if resolved_pairs[pair] > 0:
            resolved_pairs[pair] -= 1
            resolved.append(diagnostic)
    changed = []
    if reworded:
        for new, old in zip([d for d in added if diagnostic_key(d) in reworded],
                            [d for d in resolved if diagnostic_key(d) in reworded]):
            added.remove(new)
            resolved.remove(old)
            changed.append((new, old))
    return {'added': added, 'resolved': resolved, 'changed': changed}


def delta_rows(delta, rel):
    rows = []
    for new, old in sorted(delta['changed'], key=lambda pair: diagnostic_sort_key(pair[0])):
        rows.append(f'~ {diagnostic_row(new, rel)} (was: {diagnostic_message(old)[:120]})')
    for diagnostic in sorted(delta['added'], key=diagnostic_sort_key):
        rows.append(f'+ {diagnostic_row(diagnostic, rel)}')
    for diagnostic in sorted(delta['resolved'], key=diagnostic_sort_key):
        rows.append(f'- {diagnostic_row(diagnostic, rel)}')
    return rows


def render_diagnostics(path, current, previous, workspace):
    """Delta-first text for one file.

    `previous` is the set last shown to the model, or None the first time this
    file is reported. The delta leads; the current set follows it as context.
    An empty delta says so in one line instead of reprinting the list.
    """
    rel = os.path.relpath(path, workspace)
    if previous is None:
        if not current:
            return [f'{rel}: no problems (first report)']
        return [f'{rel}: {plural(len(current), "problem")} (first report)',
                *format_diagnostics(path, current, workspace).splitlines()]
    delta = diagnostic_delta(previous, current)
    added, resolved, changed = delta['added'], delta['resolved'], delta['changed']
    if not added and not resolved and not changed:
        return [f'{rel}: no change, {plural(len(current), "problem")} still open']
    parts = []
    if added:
        parts.append(f'+{len(added)} new')
    if resolved:
        parts.append(f'-{len(resolved)} resolved')
    if changed:
        parts.append(f'~{len(changed)} reworded')
    rows = [f'{rel}: {", ".join(parts)} ({len(current)} open)']
    moved = delta_rows(delta, rel)
    rows.extend(moved[:DIAG_CAP])
    if len(moved) > DIAG_CAP:
        rows.append(f'… {len(moved) - DIAG_CAP} more changes elided')
    if current:
        rows.append(f'{rel} — current set:')
        rows.extend(format_diagnostics(path, current, workspace).splitlines())
    return rows


class LanguageServer:
    """One language server, spoken to over stdio with LSP framing."""

    def __init__(self, language, spec, workspace):
        self.language = language
        self.workspace = workspace
        self.config = spec if isinstance(spec, dict) else {}
        self.timeout_s = float(self.config.get('timeout_s') or 20)
        self.debounce_s = float(self.config.get('debounce_s') or 0.6)
        self.dead = None
        self.next_id = 0
        self.buffer = b''
        self.diagnostics = {}
        self.published = {}
        self.versions = {}
        command = self.config.get('command')
        if not isinstance(command, list) or not command:
            raise ValueError('`command` must be a non-empty argument list')
        root = self.config.get('root') or '.'
        self.root = os.path.realpath(os.path.join(workspace, root))
        self.proc = subprocess.Popen(
            [str(part) for part in command],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            cwd=self.root,
        )
        try:
            initialization = self.request(
                'initialize',
                {
                    'processId': os.getpid(),
                    'rootUri': uri_for(self.root),
                    'capabilities': {'textDocument': {'publishDiagnostics': {}}},
                },
                self.timeout_s,
            )
            self.capabilities = initialization.get('capabilities', {})
            self.notify('initialized', {})
        except BaseException:
            self._stop(wait_timeout=0.1)
            raise

    def send(self, payload):
        self.proc.stdin.write(frame(payload))
        self.proc.stdin.flush()

    def notify(self, method, params):
        self.send({'jsonrpc': '2.0', 'method': method, 'params': params})

    def read_message(self, timeout):
        deadline = time.monotonic() + timeout

        def read_chunk():
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.proc.stdout], [], [], max(0, remaining))[0]:
                raise TimeoutError('no reply')
            chunk = self.proc.stdout.read1(65536)
            if not chunk:
                raise EOFError('server closed the stream')
            return chunk

        while b'\r\n\r\n' not in self.buffer:
            if len(self.buffer) > 8192:
                raise RuntimeError('LSP header exceeds 8 KiB')
            self.buffer += read_chunk()
        header, _, rest = self.buffer.partition(b'\r\n\r\n')
        if len(header) > 8192:
            raise RuntimeError('LSP header exceeds 8 KiB')
        length = None
        for line in header.decode('ascii', 'replace').split('\r\n'):
            if line.lower().startswith('content-length:'):
                length = int(line.split(':', 1)[1].strip())
        if length is None or length < 0 or length > 4 * 1024 * 1024:
            raise RuntimeError('invalid LSP frame length (maximum 4 MiB)')
        while len(rest) < length:
            rest += read_chunk()
            # Retain an incomplete frame if the deadline expires. A later
            # read can continue it without losing bytes already consumed.
            self.buffer = header + b'\r\n\r\n' + rest
        message = json.loads(rest[:length].decode('utf-8', 'replace'))
        self.buffer = rest[length:]
        return message

    def request(self, method, params, timeout):
        self.next_id += 1
        self.send({'jsonrpc': '2.0', 'id': self.next_id, 'method': method, 'params': params})
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('no reply')
            message = self.read_message(remaining)
            if message.get('method') == 'textDocument/publishDiagnostics':
                self.store_diagnostics(message.get('params') or {})
                continue
            if message.get('id') != self.next_id:
                continue
            if 'error' in message:
                raise RuntimeError(message['error'].get('message') or 'lsp error')
            return message.get('result') or {}

    def store_diagnostics(self, params):
        uri = params.get('uri')
        if isinstance(uri, str) and uri in self.versions:
            diagnostics = params.get('diagnostics') or []
            if not isinstance(diagnostics, list):
                return
            self.diagnostics[uri] = [row for row in diagnostics[:DIAG_CAP] if isinstance(row, dict) and len(json.dumps(row)) <= 8192]
            while len(self.diagnostics) > 128:
                del self.diagnostics[next(iter(self.diagnostics))]

    def remember_published(self, uri, diagnostics):
        """The set last handed to the model — the baseline for the next delta.

        Kept beside `diagnostics` rather than inside it: `diagnostics` is the
        server's latest word and is dropped on every re-check, while the model
        needs to be told what changed since the last thing it read.
        """
        self.published[uri] = list(diagnostics)
        while len(self.published) > 128:
            del self.published[next(iter(self.published))]

    def open_file(self, path):
        uri = uri_for(path)
        try:
            with open(path, encoding='utf-8', errors='strict') as fh:
                text = fh.read(1024 * 1024 + 1)
            if len(text.encode('utf-8')) > 1024 * 1024:
                raise ValueError('file exceeds 1 MiB LSP limit')
        except (OSError, ValueError):
            return None
        version = self.versions.get(uri)
        if version is None:
            self.versions[uri] = 1
            self.notify(
                'textDocument/didOpen',
                {'textDocument': {'uri': uri, 'languageId': self.language, 'version': 1, 'text': text}},
            )
        else:
            self.versions[uri] = version + 1
            self.notify(
                'textDocument/didChange',
                {
                    'textDocument': {'uri': uri, 'version': version + 1},
                    'contentChanges': [{'text': text}],
                },
            )
        return uri

    def diagnostics_for(self, uri, budget=None):
        """Wait until the server has published for `uri`, then return them.

        `budget` caps this call below the server's own timeout, so a rewrite
        that checks several files cannot outrun the host's reply deadline.
        """
        self.diagnostics.pop(uri, None)
        deadline = time.monotonic() + (self.timeout_s if budget is None else min(self.timeout_s, max(0.0, budget)))
        quiet_until = None
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            # read_message may have consumed a chunk containing notifications
            # for several URIs. Drain complete/buffered messages before
            # waiting on the OS pipe, which is no longer readable for them.
            if self.buffer:
                try:
                    message = self.read_message(min(0.2, remaining))
                except TimeoutError:
                    continue
                except (EOFError, RuntimeError, ValueError):
                    break
                if message.get('method') == 'textDocument/publishDiagnostics':
                    params = message.get('params') or {}
                    self.store_diagnostics(params)
                    if params.get('uri') == uri:
                        quiet_until = time.monotonic() + self.debounce_s
                continue
            if uri in self.diagnostics:
                if quiet_until is None:
                    quiet_until = time.monotonic() + self.debounce_s
                elif time.monotonic() >= quiet_until:
                    return self.diagnostics.get(uri) or []
            wait_for = min(0.2, remaining)
            if not select.select([self.proc.stdout], [], [], wait_for)[0]:
                continue
            try:
                message = self.read_message(wait_for)
            except TimeoutError:
                continue
            except (EOFError, RuntimeError, ValueError):
                break
            if message.get('method') == 'textDocument/publishDiagnostics':
                params = message.get('params') or {}
                self.store_diagnostics(params)
                if params.get('uri') == uri:
                    quiet_until = time.monotonic() + self.debounce_s
        return self.diagnostics.get(uri) or []

    def shutdown(self):
        try:
            self.request('shutdown', None, 3)
            self.notify('exit', {})
        except Exception:
            pass
        self._stop()

    def _stop(self, wait_timeout=3):
        """Close pipes and terminate/reap a server that did not exit cleanly."""
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=wait_timeout)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        for stream in (self.proc.stdin, self.proc.stdout):
            try:
                if stream:
                    stream.close()
            except OSError:
                pass


class PendingAction:
    """A code action or rename held with the revisions it was computed against.

    The revisions are SHA-256 digests of the file bytes, the same value the
    host's revision-checked file edits take as `expected`. Applying is a
    compare-then-write: every file must still hash to what the server saw, or
    the whole action is refused.
    """

    def __init__(self, token, kind, label, plan, revisions):
        self.token = token
        self.kind = kind
        self.label = label
        self.plan = plan
        self.revisions = revisions


class Manager:
    def __init__(self, workspace):
        self.workspace = workspace
        self.config = load_config()
        self.servers = {}
        self.reported = set()
        self.actions = {}
        self.sequence = 0

    def extension_for(self, path):
        return os.path.splitext(path)[1].lower()

    def server_for(self, path):
        ext = self.extension_for(path)
        spec = self.config.get(ext)
        if not spec:
            return None, None
        server = self.servers.get(ext)
        if server is None and ext not in self.reported:
            try:
                server = LanguageServer(ext, spec, self.workspace)
            except Exception as error:
                self.reported.add(ext)
                return None, f'lsp: {ext} server unavailable ({error})'
            self.servers[ext] = server
        return server, None

    def rel(self, path):
        return os.path.relpath(path, self.workspace)

    def rows_for(self, path, budget=None):
        """Delta-first diagnostics rows for one file, or None when no server applies."""
        server, problem = self.server_for(path)
        if problem:
            return [problem.removeprefix('lsp: ')]
        if server is None:
            return None
        uri = server.open_file(path)
        if uri is None:
            return None
        previous = server.published.get(uri)
        current = server.diagnostics_for(uri, budget)
        server.remember_published(uri, current)
        return render_diagnostics(path, current, previous, self.workspace)

    def check(self, path, header):
        rows = self.rows_for(path)
        if rows is None:
            return None
        body = '\n'.join(f'[lsp] {row}' for row in rows)
        return f'{header}\n{body}' if header else body

    def query(self, args):
        if not isinstance(args, dict):
            raise ValueError('arguments must be an object')
        operation = args.get('operation', 'diagnostics')
        if operation == 'apply':
            token = args.get('token')
            if not isinstance(token, str) or not token.strip():
                raise ValueError('apply needs the token from a rename or code_actions preview')
            return self.apply_action(token.strip())
        path = args.get('path', '')
        if not isinstance(path, str) or not path.strip():
            raise ValueError('give a workspace-relative file path')
        full = os.path.realpath(os.path.join(self.workspace, path))
        root = os.path.realpath(self.workspace)
        if os.path.commonpath([root, full]) != root or not os.path.isfile(full):
            raise ValueError('path escapes the workspace or is not a regular file')
        if operation == 'diagnostics':
            return self.check(full, f'checking {path}') or 'lsp: no server configured for this file type'
        methods = {'definition': 'textDocument/definition', 'references': 'textDocument/references',
                   'hover': 'textDocument/hover', 'symbols': 'textDocument/documentSymbol',
                   'rename': 'textDocument/rename', 'code_actions': 'textDocument/codeAction'}
        if operation not in methods:
            raise ValueError('unknown LSP operation')
        server, problem = self.server_for(full)
        if problem or server is None:
            return problem or 'lsp: no server configured for this file type'
        uri = server.open_file(full)
        if uri is None:
            raise ValueError('could not open file')
        params = {'textDocument': {'uri': uri}}
        if operation != 'symbols':
            line, column = args.get('line'), args.get('column')
            if isinstance(line, bool) or isinstance(column, bool) or not isinstance(line, int) or not isinstance(column, int) or min(line, column) < 1:
                raise ValueError('line and column must be positive integers (1-based Unicode characters)')
            with open(full, encoding='utf-8') as stream:
                text = stream.read(1024 * 1024 + 1)
            lines = text.split('\n')
            if line > len(lines) or column > len(lines[line - 1]) + 1:
                raise ValueError('position is outside the file')
            position = {'line': line - 1, 'character': len(lines[line - 1][:column - 1].encode('utf-16-le')) // 2}
            params['position'] = position
            if operation == 'references':
                params['context'] = {'includeDeclaration': True}
            elif operation == 'rename':
                name = args.get('new_name')
                if not isinstance(name, str) or not name.strip() or len(name) > 256 or any(ord(c) < 32 for c in name):
                    raise ValueError('rename needs new_name (1–256 characters)')
                params['newName'] = name
            elif operation == 'code_actions':
                params.pop('position')
                params['range'] = {'start': position, 'end': position}
                params['context'] = {'diagnostics': server.diagnostics.get(uri, [])[:DIAG_CAP]}
        result = server.request(methods[operation], params, server.timeout_s)
        body = json.dumps(result, ensure_ascii=False, indent=2)
        if len(body) > OUTPUT_CAP:
            body = body[:OUTPUT_CAP] + '\n[preview truncated; narrow the query before applying edits]'
        if operation not in ('rename', 'code_actions'):
            return f'LSP result (positions returned by server are zero-based UTF-16).\n{body}'
        preview = self.offer_actions(operation, result)
        note = ('PREVIEW ONLY: nothing has been written and no server command has been run. '
                'Apply an action below with operation "apply" and its token; it is checked '
                'against the revisions captured here and refused if any file moved. This '
                'extension never sends workspace/applyEdit or executeCommand.')
        if not preview:
            return f'{note}\n{body}'
        return f'{note}\n{body}\n\n' + '\n'.join(preview)

    def written_paths(self, body):
        """Paths from the host's `[wrote <path>]` note, newest last."""
        found = []
        for line in body.splitlines():
            if WRITE_MARKER not in line:
                continue
            rest = line.split(WRITE_MARKER, 1)[1]
            path = rest.rstrip(']').split(' —')[0].split(':', 1)[0].strip()
            if path:
                found.append(path)
        return found

    def written_files(self, body):
        """Written paths, newest first, deduplicated and confined to the workspace."""
        root = os.path.realpath(self.workspace)
        files = []
        for candidate in reversed(self.written_paths(body)):
            path = os.path.realpath(os.path.join(self.workspace, candidate))
            if path == root or os.path.commonpath([root, path]) != root:
                continue
            if not os.path.isfile(path):
                continue
            if path not in files:
                files.append(path)
        return files

    def handle_tool_result(self, body):
        """Check every file the edit wrote, newest first, within stated bounds.

        One write can touch several files and the model needs all of them
        checked, not just the newest. Both bounds — file count and wall clock,
        the latter because the host discards a rewrite that arrives late — are
        stated in the output instead of truncating it in silence.
        """
        if not self.config:
            return body
        candidates = self.written_files(body)
        if not candidates:
            return body
        deadline = time.monotonic() + CHECK_BUDGET_S
        checked, rows, stopped = [], [], False
        for path in candidates[:MAX_CHECK_FILES]:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                stopped = True
                break
            try:
                found = self.rows_for(path, remaining)
            except (OSError, ValueError, RuntimeError, TimeoutError, EOFError) as error:
                found = [f'{self.rel(path)}: check failed ({error})']
            if found is None:
                continue
            checked.append(path)
            rows.extend(f'[lsp] {row}' for row in found)
        notes = []
        if len(candidates) > MAX_CHECK_FILES:
            rest = candidates[MAX_CHECK_FILES:]
            names = ', '.join(self.rel(path) for path in rest[:4])
            notes.append(f'[lsp] {plural(len(candidates), "written file")}, checked the '
                         f'{MAX_CHECK_FILES} newest; {len(rest)} not checked ({names}'
                         f'{", …" if len(rest) > 4 else ""}) — run the lsp tool on them')
        if stopped:
            rest = [path for path in candidates if path not in checked]
            names = ', '.join(self.rel(path) for path in rest[:4])
            notes.append(f'[lsp] stopped after {len(checked)} file(s): the {CHECK_BUDGET_S:g}s '
                         f'diagnostic budget elapsed; {len(rest)} left unchecked ({names}'
                         f'{", …" if len(rest) > 4 else ""}) — run the lsp tool on them')
        if not rows and not notes:
            return body
        addition = '\n'.join(notes + rows)
        if len(addition) > OUTPUT_CAP:
            addition = addition[:OUTPUT_CAP] + '\n[lsp] … output truncated'
        return f'{body.rstrip()}\n{addition}' if body.strip() else addition

    def shutdown(self):
        for server in self.servers.values():
            server.shutdown()
        self.actions.clear()

    def resolve_uri(self, uri):
        """Filesystem path for a `file:` URI, refused outside the workspace.

        A symlinked target is refused rather than followed: the host's own
        file edits refuse symlinks, and silently editing through one would
        write a file the model never named.
        """
        parsed = urllib.parse.urlparse(uri) if isinstance(uri, str) else None
        if parsed is None or parsed.scheme != 'file' or parsed.netloc not in ('', 'localhost'):
            raise ValueError(f'unsupported URI in edit: {uri}')
        named = urllib.parse.unquote(parsed.path)
        if os.path.islink(named):
            raise ValueError(f'the edit targets the symlink {named}')
        path = os.path.realpath(named)
        root = os.path.realpath(self.workspace)
        if path == root or os.path.commonpath([root, path]) != root:
            raise ValueError(f'edit targets {path}, outside the workspace')
        return path

    def collect_edits(self, edit):
        """[(path, create, textEdits)] for a WorkspaceEdit.

        Destructive operations are refused rather than half-performed: a
        rename or delete changes the identity of files the host tracks, and
        this extension is not the writer that should do that.
        """
        plan, seen = [], set()

        def add(path, create, edits):
            if path in seen:
                raise ValueError(f'the action edits {path} twice')
            seen.add(path)
            plan.append((path, create, edits))

        for uri, edits in sorted(as_dict(edit.get('changes')).items()):
            add(self.resolve_uri(uri), False, edits)
        for change in edit.get('documentChanges') or []:
            change = as_dict(change)
            kind = str(change.get('kind') or '')
            if kind in ('rename', 'delete'):
                raise ValueError(f'a {kind} action is not applied here; use revision-checked file edits')
            if isinstance(change.get('changeAnnotations'), list):
                for annotation in change['changeAnnotations']:
                    if as_dict(annotation).get('needsConfirmation'):
                        raise ValueError('the server marked this change as needing confirmation; ask the user first')
            document = as_dict(change.get('textDocument'))
            uri = document.get('uri') or change.get('uri') or change.get('oldUri')
            if not isinstance(uri, str):
                raise ValueError('a document change carries no URI')
            create = kind == 'create'
            # A TextDocumentEdit keeps `edits` beside `textDocument`, not inside it.
            edits = change.get('edits') if 'edits' in change else document.get('edits')
            if not create and not isinstance(edits, list):
                raise ValueError('a document change carries no text edits')
            add(self.resolve_uri(uri), create, edits)
        if not plan:
            raise ValueError('the action carries no file edit')
        if len(plan) > MAX_EDIT_FILES:
            raise ValueError(f'the action touches {len(plan)} files, over the {MAX_EDIT_FILES}-file apply bound')
        return plan

    def read_target(self, path):
        """File bytes, or None when the path does not exist."""
        try:
            with open(path, 'rb') as stream:
                data = stream.read(FILE_CAP + 1)
        except FileNotFoundError:
            if os.path.lexists(path):
                raise ValueError('the path exists but is not a readable regular file')
            return None
        if len(data) > FILE_CAP:
            raise ValueError(f'the file exceeds the {FILE_CAP // 1024 // 1024} MiB LSP edit limit')
        return data

    def transform(self, data, edits):
        """New file bytes for one target, or raise before anything is written."""
        if data is None:
            return b''
        try:
            text = data.decode('utf-8')
        except UnicodeDecodeError as error:
            raise ValueError(f'not valid UTF-8 ({error.reason})') from error
        if edits is None:
            return data
        return apply_text_edits(text, edits).encode('utf-8')

    def store_action(self, kind, label, edit):
        """Capture a WorkspaceEdit with the revisions it was computed against.

        The edits are run once against the current bytes so a malformed range
        is reported in the preview rather than at apply time; the result is
        discarded and recomputed from verified bytes on apply.
        """
        plan = self.collect_edits(edit)
        revisions = {}
        for path, create, edits in plan:
            data = self.read_target(path)
            if create and data is not None:
                raise ValueError(f'{self.rel(path)} already exists')
            if not create and data is None:
                raise ValueError(f'{self.rel(path)} does not exist')
            self.transform(data, edits)
            revisions[path] = hashlib.sha256(data).hexdigest() if data is not None else None
        self.sequence += 1
        token = f'{kind}:{self.sequence}'
        self.actions[token] = PendingAction(token, kind, label or kind, plan, revisions)
        while len(self.actions) > MAX_ACTIONS:
            del self.actions[next(iter(self.actions))]
        return self.actions[token]

    def offer_actions(self, operation, result):
        """Register the editable actions in a result and describe how to apply."""
        if isinstance(result, dict) and ('changes' in result or 'documentChanges' in result):
            candidates = [result]
        elif isinstance(result, list):
            candidates = [item for item in result if isinstance(item, dict)]
        else:
            return []
        lines = []
        for action in candidates:
            title = str(action.get('title') or '').strip()[:120] or 'action'
            if action.get('command'):
                lines.append(f'{title}: server command {action["command"]!r} — never run here; '
                             'apply it yourself with revision-checked file edits')
            edit = as_dict(action.get('edit'))
            if not edit and ('changes' in action or 'documentChanges' in action):
                edit = action  # a rename result is itself the WorkspaceEdit
            if not edit:
                continue
            try:
                pending = self.store_action(operation, title, edit)
            except (OSError, ValueError) as error:
                lines.append(f'{title}: not applicable — {error}')
                continue
            lines.append(f'{pending.token}: {pending.label} — checked against '
                         f'{plural(len(pending.revisions), "file")}')
            for path, revision in pending.revisions.items():
                lines.append(f'  {self.rel(path)} expected={revision or "absent"}')
            lines.append(f'  apply with operation="apply" token="{pending.token}"; refused if any file moved')
        return lines

    def apply_action(self, token):
        """Write a pending action, but only if every file still matches.

        Verification reads the exact bytes the edits are then applied to, so a
        file that moved since the action was computed refuses the whole action
        with nothing written.
        """
        action = self.actions.pop(token, None)
        if action is None:
            raise ValueError('unknown or already-used action token; request the action again')
        verified, moved, unreadable = {}, [], []
        for path, expected in action.revisions.items():
            try:
                data = self.read_target(path)
            except (OSError, ValueError) as error:
                unreadable.append(f'{self.rel(path)} ({error})')
                continue
            if (hashlib.sha256(data).hexdigest() if data is not None else None) != expected:
                moved.append(self.rel(path))
            verified[path] = data
        if moved or unreadable:
            reasons = []
            if moved:
                reasons.append('changed since the action was computed: ' + ', '.join(moved))
            if unreadable:
                reasons.append('could not be read: ' + ', '.join(unreadable))
            return f'REFUSED — {"; ".join(reasons)}; {REREAD_HINT}.'
        receipts, refused, planned = [], [], []
        for path, create, edits in action.plan:
            try:
                planned.append((path, create, self.transform(verified[path], edits)))
            except (OSError, ValueError) as error:
                refused.append(f'{self.rel(path)} ({error})')
        if refused:
            # Every target was computed before any of them is written, so a
            # rejected edit cannot leave half an action behind.
            return f'REFUSED — {"; ".join(refused)}; {REREAD_HINT}.'
        for path, create, updated in planned:
            try:
                write_atomic(path, updated, create=create)
            except (OSError, ValueError) as error:
                refused.append(f'{self.rel(path)} ({error})')
                continue
            receipts.append(f'{self.rel(path)}: {(action.revisions[path] or "absent")[:12]} → '
                            f'{hashlib.sha256(updated).hexdigest()[:12]}')
        lines = [f'applied {action.token}: {plural(len(receipts), "file")} written after a revision check']
        lines.extend(f'  {receipt}' for receipt in receipts)
        if refused:
            lines.append(f'not written: {"; ".join(refused)} — the files above were written; '
                         're-check their diagnostics')
        if receipts:
            lines.append('re-check diagnostics on the changed files to confirm the fix')
        return '\n'.join(lines)


MANIFEST = {
    'name': 'lsp',
    'version': '1.2.0',
    'system_prompt': (
        'After file writes, configured language servers report only what '
        'changed: `[lsp] path: +1 new, -1 resolved (2 open)` followed by the '
        'delta rows and the current set. Treat them as authoritative compiler '
        'feedback; fix errors before building further. `rename` and '
        '`code_actions` return previews with an apply token; `apply` writes one '
        'only when every file still has the revision captured with it.'
    ),
    'tools': [
        {
            'name': 'lsp',
            'description': 'Code intelligence: diagnostics, definition, references, hover, symbols, rename or code_actions. Rename/actions return previews carrying a token; `apply` writes one only if every affected file still has the revision captured with it. Positions are 1-based Unicode characters.',
            'schema': {
                'type': 'object',
                'properties': {
                    'path': {'type': 'string', 'description': 'File relative to the workspace.'},
                    'operation': {'type': 'string', 'enum': ['diagnostics', 'definition', 'references', 'hover', 'symbols', 'rename', 'code_actions', 'apply']},
                    'line': {'type': 'integer', 'minimum': 1},
                    'column': {'type': 'integer', 'minimum': 1},
                    'new_name': {'type': 'string'},
                    'token': {'type': 'string', 'description': 'Apply token from a rename or code_actions preview; not needed for the other operations.'},
                },
            },
            'concurrency': 'serial',
        }
    ],
    'commands': [{'name': 'lsp', 'description': 'Show language-server status'}],
}


def send(obj):
    sys.stdout.write(json.dumps(obj) + '\n')
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def main():
    manager = None
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get('type')
        if kind == 'hello':
            manager = Manager(req.get('workspace') or os.getcwd())
            reply = {'id': req.get('id'), 'protocol': PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == 'system_prompt':
            send({'id': req.get('id'), 'text': MANIFEST['system_prompt']})
        elif kind == 'authorize':
            send({'id': req.get('id'), 'allow': True})
        elif kind == 'tool_result':
            body = str(req.get('body') or '')
            new_body = manager.handle_tool_result(body) if manager else body
            send({'id': req.get('id'), 'body': new_body})
        elif kind == 'run_tool':
            try:
                args = json.loads(req.get('arguments') or '{}')
            except ValueError:
                output = 'lsp: arguments are not valid JSON'
            else:
                try:
                    output = manager.query(args)
                except (OSError, ValueError, RuntimeError, TimeoutError, EOFError) as error:
                    output = f'lsp: {error}'
            send({'id': req.get('id'), 'output': output})
        elif kind == 'command':
            if req.get('name') == 'lsp':
                active = ', '.join(f'{ext} ({spec.get("command", [""])[0]})' for ext, spec in manager.config.items()) if manager else ''
                running = ', '.join(manager.servers) if manager and manager.servers else 'none running'
                output = f'lsp: configured {active}\nlsp: running {running}'
            else:
                output = 'lsp: unknown command'
            send({'id': req.get('id'), 'output': output})
        elif kind in ('goodbye', 'shutdown'):
            if manager:
                manager.shutdown()
            break


def selftest():
    assert frame({'a': 1}).startswith(b'Content-Length: ')
    rows = format_diagnostics(
        '/w/a.py',
        [
            {'severity': 2, 'range': {'start': {'line': 4, 'character': 2}}, 'message': 'unused'},
            {'severity': 1, 'range': {'start': {'line': 1, 'character': 0}}, 'message': 'real  error\nhere'},
        ],
        '/w',
    )
    assert rows.splitlines()[0] == 'a.py:2:1 error: real error here', rows
    assert rows.splitlines()[1] == 'a.py:5:3 warning: unused', rows
    big = [{'severity': 1, 'range': {'start': {'line': i}}, 'message': 'x'} for i in range(50)]
    assert len(format_diagnostics('/w/a.py', big, '/w').splitlines()) == DIAG_CAP + 1
    # The write-note parser takes the host's exact marker format.
    manager = Manager.__new__(Manager)
    assert Manager.written_paths(manager, '[wrote src/a.py — 3 lines]\nrest') == ['src/a.py']

    # Deltas: added, resolved, reworded and unchanged each land where they
    # belong, and an unchanged set is not re-rendered.
    def problem(line, message, severity=1):
        return {'severity': severity, 'range': {'start': {'line': line, 'character': 0},
                                                'end': {'line': line, 'character': 4}}, 'message': message}

    before = [problem(1, 'undefined name'), problem(4, 'unused import', 2)]
    delta = diagnostic_delta(before, [problem(1, 'undefined name'), problem(7, 'bad indent')])
    assert [d['message'] for d in delta['added']] == ['bad indent'], delta
    assert [d['message'] for d in delta['resolved']] == ['unused import'], delta
    assert delta['changed'] == [], delta
    reworded = diagnostic_delta(before, [problem(1, 'undefined  value'), problem(4, 'unused import', 2)])
    assert [(diagnostic_message(n), diagnostic_message(o)) for n, o in reworded['changed']] == [('undefined value', 'undefined name')], reworded
    assert not reworded['added'] and not reworded['resolved'], reworded
    assert diagnostic_delta(before, before) == {'added': [], 'resolved': [], 'changed': []}

    text = '\n'.join(render_diagnostics('/w/a.py', before, None, '/w'))
    assert text.splitlines()[0] == 'a.py: 2 problems (first report)', text
    delta_text = '\n'.join(render_diagnostics('/w/a.py', [problem(1, 'undefined name'), problem(7, 'bad indent')], before, '/w'))
    assert delta_text.splitlines()[0] == 'a.py: +1 new, -1 resolved (2 open)', delta_text
    assert '[lsp]' not in delta_text and '+ a.py:8:1 error: bad indent' in delta_text, delta_text
    assert '- a.py:5:1 warning: unused import' in delta_text, delta_text
    assert delta_text.index('+ a.py:8:1') < delta_text.index('a.py — current set:'), delta_text
    quiet = render_diagnostics('/w/a.py', before, before, '/w')
    assert quiet == ['a.py: no change, 2 problems still open'], quiet
    assert render_diagnostics('/w/a.py', [], [], '/w') == ['a.py: no change, 0 problems still open']
    assert render_diagnostics('/w/a.py', [], None, '/w') == ['a.py: no problems (first report)']

    # Every written file is checked, and the bound is stated when it bites.
    with tempfile.TemporaryDirectory(prefix='gc-lsp-selftest-') as scratch:
        manager = Manager.__new__(Manager)
        manager.workspace = scratch
        manager.config = {'.py': {'command': ['true']}}
        names = [f'm{index}.py' for index in range(MAX_CHECK_FILES + 2)]
        for name in names:
            open(os.path.join(scratch, name), 'w').close()
        manager.rows_for = lambda path, budget=None: [f'{os.path.basename(path)}: clean']
        body = '\n'.join(f'[wrote {name} — 1 lines]' for name in names)
        out = Manager.handle_tool_result(manager, body)
        assert body in out, out
        assert out.count('clean') == MAX_CHECK_FILES, out
        assert f'{plural(len(names), "written file")}, checked the {MAX_CHECK_FILES} newest' in out, out
        # Newest first: the tail of the note names what was left out.
        assert f'{len(names) - MAX_CHECK_FILES} not checked (m1.py, m0.py)' in out, out
        assert out.index('m6.py: clean') < out.index('m2.py: clean'), out
        # The wall-clock bound is reported too, never silently dropped.
        global CHECK_BUDGET_S
        budget, CHECK_BUDGET_S = CHECK_BUDGET_S, -1.0
        try:
            out = Manager.handle_tool_result(manager, body)
        finally:
            CHECK_BUDGET_S = budget
        assert 'clean' not in out and 'diagnostic budget elapsed' in out and 'm0.py' in out, out
        # A single write still behaves, and paths outside the workspace are dropped.
        assert Manager.handle_tool_result(manager, '[wrote m0.py]\n') == '[wrote m0.py]\n[lsp] m0.py: clean'
        assert Manager.written_files(manager, '[wrote ../outside.py]\n[wrote m0.py]\n[wrote m0.py]\n') == [
            os.path.realpath(os.path.join(scratch, 'm0.py'))]
        empty = Manager.__new__(Manager)
        empty.workspace, empty.config = scratch, {}
        assert Manager.handle_tool_result(empty, body) == body

        # A code action is applied only while every file still hashes to the
        # revision captured with it; a moved file refuses the whole thing.
        manager = Manager.__new__(Manager)
        manager.workspace = scratch
        manager.actions, manager.sequence = {}, 0
        target = os.path.realpath(os.path.join(scratch, 'a.py'))
        with open(target, 'w', encoding='utf-8') as stream:
            stream.write('value = 1\n')
        edit = {'documentChanges': [{'textDocument': {'uri': uri_for(target), 'version': None},
                                     'edits': [{'range': {'start': {'line': 0, 'character': 0},
                                                          'end': {'line': 0, 'character': 5}}, 'newText': 'total'}]}]}
        pending = Manager.store_action(manager, 'rename', 'rename value', edit)
        assert pending.revisions[target] == hashlib.sha256(b'value = 1\n').hexdigest(), pending.revisions
        with open(target, 'w', encoding='utf-8') as stream:
            stream.write('value = 2\n')
        refusal = Manager.apply_action(manager, pending.token)
        assert refusal.startswith('REFUSED') and 'a.py' in refusal and 'reread' in refusal, refusal
        assert open(target, encoding='utf-8').read() == 'value = 2\n', 'a refused action must not write'
        try:
            Manager.apply_action(manager, pending.token)
            raise AssertionError('a refused token must not be reusable')
        except ValueError:
            pass

        # An unchanged file applies, and the receipt names both revisions.
        pending = Manager.store_action(manager, 'rename', 'rename value', edit)
        applied = Manager.apply_action(manager, pending.token)
        assert open(target, encoding='utf-8').read() == 'total = 2\n', applied
        assert f'a.py: {hashlib.sha256(b"value = 2\n").hexdigest()[:12]} →' in applied, applied

        # Refusals are explicit rather than half-performed.
        for bad, expected in (
            ({'documentChanges': [{'kind': 'rename', 'oldUri': uri_for(target), 'newUri': uri_for(target) + '.x'}]}, 'rename'),
            ({'documentChanges': [{'kind': 'delete', 'uri': uri_for(target)}]}, 'delete'),
            ({'documentChanges': [{'textDocument': {'uri': uri_for(target)},
                                  'edits': [{'range': {'start': {'line': 0, 'character': 0},
                                                       'end': {'line': 0, 'character': 99}}, 'newText': 'x'}]}]}, 'past the end'),
            ({'documentChanges': [{'textDocument': {'uri': 'file:///etc/passwd'},
                                  'edits': [{'range': {'start': {'line': 0, 'character': 0},
                                                       'end': {'line': 0, 'character': 0}}, 'newText': 'x'}]}]}, 'outside the workspace'),
        ):
            try:
                Manager.store_action(manager, 'code_actions', 'bad', bad)
                raise AssertionError(f'{expected} must be refused')
            except ValueError as error:
                assert expected in str(error), error
        with open(target, encoding='utf-8') as stream:
            assert stream.read() == 'total = 2\n'

        # A file the action wants to create must still be absent.
        fresh = os.path.realpath(os.path.join(scratch, 'new.py'))
        create = {'documentChanges': [{'kind': 'create', 'uri': uri_for(fresh)}]}
        pending = Manager.store_action(manager, 'code_actions', 'create file', create)
        assert pending.revisions[fresh] is None, pending.revisions
        assert 'absent' in '\n'.join(Manager.offer_actions(manager, 'code_actions', [{'title': 'create', 'edit': create}]))
        Manager.apply_action(manager, pending.token)
        assert os.path.isfile(fresh) and open(fresh, encoding='utf-8').read() == ''
        # A server command is never run and a command-only action offers no token.
        listed = Manager.offer_actions(manager, 'code_actions', [{'title': 'run it', 'command': 'do.thing'}])
        assert listed == ["run it: server command 'do.thing' — never run here; apply it yourself with revision-checked file edits"], listed
        assert len(manager.actions) == 1, manager.actions

        # Text edits land where the server's UTF-16 positions point, and the
        # file modes a rename must not lose survive.
        unicode_file = os.path.realpath(os.path.join(scratch, 'emoji.py'))
        with open(unicode_file, 'w', encoding='utf-8') as stream:
            stream.write('a😀b = 1\r\nsecond\n')
        os.chmod(unicode_file, 0o640)
        emoji_edit = {'changes': {uri_for(unicode_file): [
            {'range': {'start': {'line': 0, 'character': 3}, 'end': {'line': 0, 'character': 4}}, 'newText': 'c'}]}}
        Manager.apply_action(manager, Manager.store_action(manager, 'rename', 'emoji', emoji_edit).token)
        assert open(unicode_file, 'rb').read() == 'a😀c = 1\r\nsecond\n'.encode('utf-8')
        assert oct(os.stat(unicode_file).st_mode & 0o777) == oct(0o640)
        # Overlapping edits are refused, not half-applied.
        overlap = {'changes': {uri_for(unicode_file): [
            {'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 3}}, 'newText': 'x'},
            {'range': {'start': {'line': 0, 'character': 1}, 'end': {'line': 0, 'character': 4}}, 'newText': 'y'}]}}
        try:
            Manager.store_action(manager, 'rename', 'overlap', overlap)
            raise AssertionError('overlapping edits must be refused')
        except ValueError as error:
            assert 'overlap' in str(error), error
        assert open(unicode_file, 'rb').read() == 'a😀c = 1\r\nsecond\n'.encode('utf-8')
        link = os.path.join(scratch, 'link.py')
        outside = os.path.realpath(os.path.join(scratch, 'outside.py'))
        with open(outside, 'w', encoding='utf-8') as stream:
            stream.write('kept\n')
        os.symlink(outside, link)
        # A server naming the link itself (uri_for resolves links, so build it raw).
        link_uri = 'file://' + urllib.parse.quote(link)
        try:
            Manager.store_action(manager, 'rename', 'link', {'changes': {link_uri: [
                {'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 4}}, 'newText': 'gone'}]}})
            raise AssertionError('a symlink must not be edited through')
        except ValueError as error:
            assert 'symlink' in str(error), error
        assert open(outside, encoding='utf-8').read() == 'kept\n'

    def probe(code):
        process = subprocess.Popen(
            [sys.executable, '-c', code],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        server = LanguageServer.__new__(LanguageServer)
        server.proc = process
        server.buffer = b''
        return server

    # Both incomplete headers and bodies must obey the caller's budget even
    # while the peer keeps stdout open.
    for code in (
        "import sys,time; sys.stdout.buffer.write(b'Content-Length: 20\\r\\n'); sys.stdout.flush(); time.sleep(4)",
        "import sys,time; sys.stdout.buffer.write(b'Content-Length: 100\\r\\n\\r\\n{'); sys.stdout.flush(); time.sleep(4)",
    ):
        server = probe(code)
        started = time.monotonic()
        try:
            server.read_message(0.1)
            raise AssertionError('partial frame should time out')
        except TimeoutError:
            assert time.monotonic() - started < 0.6, 'partial frame exceeded its deadline'
        finally:
            server._stop(wait_timeout=0.05)

    # Bytes following one frame belong to the next frame, even when one pipe
    # read delivers both together.
    first = frame({'jsonrpc': '2.0', 'id': 1, 'result': {'n': 1}})
    second = frame({'jsonrpc': '2.0', 'id': 2, 'result': {'n': 2}})
    code = f"import sys; sys.stdout.buffer.write(bytes.fromhex('{(first + second).hex()}')); sys.stdout.flush()"
    server = probe(code)
    try:
        assert server.read_message(1)['id'] == 1
        assert server.read_message(1)['id'] == 2
    finally:
        server._stop(wait_timeout=0.05)

    # A timeout does not discard a partially consumed body; the next read can
    # finish the same frame when the server resumes writing.
    payload = frame({'jsonrpc': '2.0', 'id': 3, 'result': {'continued': True}})
    split = len(payload) - 5
    code = (
        'import sys,time; '
        f'sys.stdout.buffer.write(bytes.fromhex(\'{payload[:split].hex()}\')); '
        'sys.stdout.flush(); time.sleep(0.25); '
        f'sys.stdout.buffer.write(bytes.fromhex(\'{payload[split:].hex()}\')); '
        'sys.stdout.flush()'
    )
    server = probe(code)
    try:
        try:
            server.read_message(0.1)
            raise AssertionError('incomplete body should time out')
        except TimeoutError:
            pass
        assert server.read_message(1)['result'] == {'continued': True}
    finally:
        server._stop(wait_timeout=0.05)

    # A stream of unrelated notifications cannot extend request() beyond its
    # deadline merely because each individual read succeeds.
    server = probe(
        "import json,sys\n"
        "msg=json.dumps({'jsonrpc':'2.0','method':'$/progress','params':{}})+'\\n'\n"
        "while True:\n sys.stdout.write(msg); sys.stdout.flush()\n"
    )
    server.next_id = 0
    server.stateless = False
    started = time.monotonic()
    try:
        try:
            server.request('initialize', {}, 0.15)
            raise AssertionError('notification flood should time out')
        except (TimeoutError, RuntimeError):
            assert time.monotonic() - started < 0.6, 'notification flood exceeded request deadline'
    finally:
        server._stop(wait_timeout=0.05)

    # diagnostics_for must drain complete notifications already buffered by
    # an earlier read, even when the child pipe is no longer readable.
    server = probe('import time; time.sleep(4)')
    server.timeout_s = 0.2
    server.debounce_s = 0.01
    server.diagnostics = {}
    server.versions = {'file:///buffered.py': 1}
    server.buffer = frame({
        'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics',
        'params': {'uri': 'file:///buffered.py', 'diagnostics': [{'message': 'buffered'}]},
    })
    try:
        rows = server.diagnostics_for('file:///buffered.py')
        assert rows == [{'message': 'buffered'}], rows
    finally:
        server._stop(wait_timeout=0.05)

    # An initialization timeout must not orphan the child that was just
    # spawned. Its PID file makes the assertion independent of process lists.
    with tempfile.TemporaryDirectory(prefix='gc-lsp-selftest-') as scratch:
        pid_file = os.path.join(scratch, 'pid')
        code = (
            'import os,time,pathlib; '
            f'pathlib.Path({pid_file!r}).write_text(str(os.getpid())); '
            'time.sleep(10)'
        )
        try:
            LanguageServer('.test', {'command': [sys.executable, '-c', code], 'timeout_s': 0.1}, os.getcwd())
            raise AssertionError('silent server initialization should time out')
        except TimeoutError:
            pass
        child = int(open(pid_file, encoding='ascii').read())
        try:
            os.kill(child, 0)
        except ProcessLookupError:
            pass
        else:
            raise AssertionError('failed initialization left its server process alive')
    print('PASS lsp extension selftest')


if __name__ == '__main__':
    if '--selftest' in sys.argv:
        selftest()
    else:
        main()
