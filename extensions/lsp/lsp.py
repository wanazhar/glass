#!/usr/bin/env python3
"""glass lsp extension: diagnostics from a real language server after edits.

The compiler is the cheapest reviewer there is, and opencode proved that
putting its verdict in the tool result — before the model reads it — is
worth the plumbing. This extension manages language servers per workspace,
opens the file the model just wrote, waits for `publishDiagnostics`, and
appends the result:

    [lsp] src/app.py:12:5 error: undefined name 'valu'

Config at `~/.config/glass-core/lsp.json` (or `GC_LSP_CONFIG`):

    {
      ".py": {"command": ["pylsp"], "root": ".", "timeout_s": 20, "debounce_s": 0.6},
      ".rs": {"command": ["rust-analyzer"], "timeout_s": 30}
    }

Deliberately: servers start lazily on the first write of that language, stay
warm for the session, and are shut down on exit. A server that cannot start
is reported once and then ignored — a missing language server must never
break the edit loop. The `/lsp` command shows what is running; the `lsp`
tool asks for diagnostics explicitly.
"""

import json
import os
import select
import subprocess
import sys
import tempfile
import time
import urllib.parse

PROTOCOL = 1
WRITE_MARKER = '[wrote '
DIAG_CAP = 20
OUTPUT_CAP = 4096


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


def format_diagnostics(path, diagnostics, workspace):
    """`[lsp] path:line:col severity message`, errors first."""
    severity = {1: 'error', 2: 'warning', 3: 'info', 4: 'hint'}
    ordered = sorted(
        diagnostics,
        key=lambda d: (d.get('severity', 3), d.get('range', {}).get('start', {}).get('line', 0)),
    )
    rel = os.path.relpath(path, workspace)
    rows = []
    for d in ordered[:DIAG_CAP]:
        start = d.get('range', {}).get('start', {})
        line = start.get('line', 0) + 1
        col = start.get('character', 0) + 1
        text = ' '.join(str(d.get('message', '')).split())[:200]
        level = severity.get(d.get('severity'), 'info')
        rows.append(f'{rel}:{line}:{col} {level}: {text}')
    if len(diagnostics) > DIAG_CAP:
        rows.append(f'… {len(diagnostics) - DIAG_CAP} more diagnostics elided')
    return '\n'.join(rows)


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
            self.request(
                'initialize',
                {
                    'processId': os.getpid(),
                    'rootUri': uri_for(self.root),
                    'capabilities': {'textDocument': {'publishDiagnostics': {}}},
                },
                self.timeout_s,
            )
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
            self.buffer += read_chunk()
        header, _, rest = self.buffer.partition(b'\r\n\r\n')
        length = None
        for line in header.decode('ascii', 'replace').split('\r\n'):
            if line.lower().startswith('content-length:'):
                length = int(line.split(':', 1)[1].strip())
        if length is None:
            raise RuntimeError('no Content-Length in frame')
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
        if uri:
            self.diagnostics[uri] = params.get('diagnostics') or []

    def open_file(self, path):
        uri = uri_for(path)
        try:
            with open(path, encoding='utf-8', errors='replace') as fh:
                text = fh.read()
        except OSError:
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

    def diagnostics_for(self, uri):
        """Wait until the server has published for `uri`, then return them."""
        self.diagnostics.pop(uri, None)
        deadline = time.monotonic() + self.timeout_s
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


class Manager:
    def __init__(self, workspace):
        self.workspace = workspace
        self.config = load_config()
        self.servers = {}
        self.reported = set()

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

    def check(self, path, header):
        server, problem = self.server_for(path)
        if problem:
            return problem
        if server is None:
            return None
        uri = server.open_file(path)
        if uri is None:
            return None
        diagnostics = server.diagnostics_for(uri)
        if not diagnostics:
            return f'{header}\n[lsp] no diagnostics'
        return f'{header}\n[lsp] ' + '\n[lsp] '.join(
            format_diagnostics(path, diagnostics, self.workspace).splitlines()
        )

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

    def handle_tool_result(self, body):
        if not self.config:
            return body
        for candidate in reversed(self.written_paths(body)):
            path = os.path.realpath(os.path.join(self.workspace, candidate))
            if not path.startswith(os.path.realpath(self.workspace) + os.sep):
                continue
            if not os.path.isfile(path):
                continue
            addition = self.check(path, '')
            if addition:
                addition = addition.lstrip('\n')
                block = f'[lsp] {addition[len("[lsp] "):]}' if addition.startswith('[lsp] ') else addition
                body = f'{body}\n{block[:OUTPUT_CAP]}'
            break
        return body

    def shutdown(self):
        for server in self.servers.values():
            server.shutdown()


MANIFEST = {
    'name': 'lsp',
    'version': '1.0.0',
    'system_prompt': (
        'After file writes, configured language servers may report diagnostics '
        'as `[lsp] path:line:col severity: message` appended to the tool '
        'result. Treat them as authoritative compiler feedback; fix errors '
        'before building further.'
    ),
    'tools': [
        {
            'name': 'lsp',
            'description': 'Language-server diagnostics for one file (or a running-server status). Use after writing code when no diagnostics were appended.',
            'schema': {
                'type': 'object',
                'properties': {
                    'path': {'type': 'string', 'description': 'File to check, relative to the workspace.'},
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
                path = (args.get('path') or '').strip() if isinstance(args, dict) else ''
                if not path:
                    output = 'lsp: give a path to check'
                else:
                    full = os.path.realpath(os.path.join(manager.workspace, path))
                    if not full.startswith(os.path.realpath(manager.workspace) + os.sep):
                        output = 'lsp: path escapes the workspace'
                    else:
                        output = manager.check(full, f'checking {path}') or 'lsp: no server configured for this file type'
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
        except TimeoutError:
            assert time.monotonic() - started < 0.6, 'notification flood exceeded request deadline'
    finally:
        server._stop(wait_timeout=0.05)

    # diagnostics_for must drain complete notifications already buffered by
    # an earlier read, even when the child pipe is no longer readable.
    server = probe('import time; time.sleep(4)')
    server.timeout_s = 0.2
    server.debounce_s = 0.01
    server.diagnostics = {}
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
