#!/usr/bin/env python3
"""Lazy ACP/DAP client. No adapter process starts during the hello handshake.
Only named executables in the explicit local adapters.json configuration can start.
Long requests return tickets; poll drains bounded events without blocking the TUI.
"""
import collections
import json
import os
import select
import signal
import subprocess
import sys
import time

CAP = 1024 * 1024
MUTATING_DAP = {'launch', 'attach', 'evaluate', 'setVariable', 'setExpression', 'continue', 'next', 'stepIn', 'stepOut', 'restart', 'terminate', 'goto', 'restartFrame', 'pause', 'setBreakpoints', 'setExceptionBreakpoints', 'configurationDone'}
DAP = MUTATING_DAP | {'threads', 'stackTrace', 'scopes', 'variables', 'setBreakpoints', 'setExceptionBreakpoints', 'configurationDone', 'pause', 'disconnect', 'source', 'loadedSources', 'modules'}
ACP = {'session/new', 'session/load', 'session/resume', 'session/list', 'session/prompt', 'session/cancel', 'session/set_mode', 'session/close', 'authenticate'}


def config():
    path = os.environ.get('GC_ADAPTER_CONFIG') or os.path.join(os.environ.get('XDG_CONFIG_HOME') or os.path.expanduser('~/.config'), 'glass-core', 'adapters.json')
    with open(path, encoding='utf-8') as source:
        value = json.loads(source.read(65537))
    if not isinstance(value, dict):
        raise ValueError('Adapter configuration must be an object')
    return value


class Adapter:
    def __init__(self, name, spec, workspace):
        if not isinstance(spec, dict) or spec.get('protocol') not in ('acp', 'dap'):
            raise ValueError('Configure protocol acp or dap')
        argv = spec.get('command')
        if not isinstance(argv, list) or not argv or len(argv) > 32 or not all(isinstance(v, str) and v and len(v) <= 4096 for v in argv):
            raise ValueError('Adapter command must be an explicit argv array')
        self.name, self.spec, self.workspace = name, spec, workspace
        self.protocol = spec['protocol']
        if self.protocol == 'acp' and spec.get('version', 1) != 1:
            raise ValueError('This adapter supports ACP v1; configure version:1')
        self.seq = 0
        self.buffer = b''
        self.events = collections.deque(maxlen=64)
        self.dropped = 0
        self.pending = set()
        self.ready = False
        self.stopped = False
        self.capabilities = None
        env = {k: v for k, v in os.environ.items() if not any(word in k.upper() for word in ('TOKEN', 'SECRET', 'API_KEY', 'PASSWORD'))}
        # Adapter login uses its own credential store; GC credentials are never forwarded.
        self.child = subprocess.Popen(argv, cwd=workspace, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            os.set_blocking(self.child.stdin.fileno(), False)
            os.set_blocking(self.child.stdout.fileno(), False)
            self.initialize_id = self.send('initialize', {'protocolVersion': spec.get('version', 1), 'clientCapabilities': {}, 'clientInfo': {'name': 'glass-core', 'version': '0.3.14'}} if self.protocol == 'acp' else {'clientID': 'glass-core', 'adapterID': name, 'pathFormat': 'path', 'linesStartAt1': True, 'columnsStartAt1': True, 'supportsRunInTerminalRequest': False})
        except BaseException:
            self.stop()
            raise

    def write(self, value):
        body = json.dumps(value, ensure_ascii=False).encode()
        if len(body) > CAP:
            raise ValueError('Adapter request exceeds 1 MiB')
        wire = body + b'\n' if self.protocol == 'acp' else b'Content-Length: %d\r\n\r\n' % len(body) + body
        deadline = time.monotonic() + 1
        at = 0
        while at < len(wire):
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([], [self.child.stdin], [], remaining)[1]:
                self.stop()
                raise TimeoutError('Adapter input stalled; process stopped to avoid partial request replay')
            try:
                at += os.write(self.child.stdin.fileno(), wire[at:at+4096])
            except BlockingIOError:
                continue

    def send(self, method, params):
        self.seq += 1
        if len(self.pending) >= 32:
            raise ValueError('32 pending adapter requests maximum; poll first')
        value = {'jsonrpc': '2.0', 'id': self.seq, 'method': method, 'params': params} if self.protocol == 'acp' else {'seq': self.seq, 'type': 'request', 'command': method, 'arguments': params}
        self.write(value)
        self.pending.add(self.seq)
        return self.seq

    def request(self, method, params):
        self.drain()
        if not self.ready:
            raise ValueError('Initialization has not succeeded; poll capabilities first')
        if method not in (ACP if self.protocol == 'acp' else DAP):
            raise ValueError('Unsupported adapter operation')
        if self.protocol == 'acp' and method == 'session/prompt' and not self.spec.get('allow_agent_execution', False):
            raise PermissionError('Set allow_agent_execution:true explicitly; external agents may execute their own tools')
        if self.protocol == 'dap' and method in MUTATING_DAP and not self.spec.get('allow_debug_execution', False):
            raise PermissionError('Set allow_debug_execution:true explicitly before attach, evaluation or program execution')
        return self.send(method, params)

    def receive(self, value):
        if not isinstance(value, dict):
            raise ValueError('Adapter frame must be an object')
        # Never execute reverse filesystem, terminal or permission requests.
        if self.protocol == 'acp' and 'method' in value and 'id' in value:
            if value['method'] == 'session/request_permission':
                self.write({'jsonrpc': '2.0', 'id': value['id'], 'result': {'outcome': {'outcome': 'cancelled'}}})
            else:
                self.write({'jsonrpc': '2.0', 'id': value['id'], 'error': {'code': -32601, 'message': 'GC client capability not enabled'}})
        elif self.protocol == 'dap' and value.get('type') == 'request':
            self.seq += 1
            self.write({'seq': self.seq, 'type': 'response', 'request_seq': value.get('seq'), 'command': value.get('command'), 'success': False, 'message': 'Reverse execution is disabled'})
        ticket = value.get('id') if self.protocol == 'acp' else value.get('request_seq')
        if not isinstance(ticket, (int, str, type(None))) or isinstance(ticket, bool):
            raise ValueError("Invalid adapter response ticket")
        if ticket in self.pending:
            self.pending.remove(ticket)
            if ticket == self.initialize_id:
                self.ready = ('result' in value if self.protocol == 'acp' else value.get('success') is True)
                self.capabilities = value.get('result') if self.protocol == 'acp' else value.get('body')
                if self.protocol == 'acp' and self.ready and (not isinstance(self.capabilities, dict) or self.capabilities.get('protocolVersion') != 1):
                    self.ready = False
                if len(json.dumps(self.capabilities)) > 32768:
                    self.capabilities = {'truncated': True, 'message': 'Capability response exceeded 32 KiB'}
        if len(self.events) == self.events.maxlen:
            self.dropped += 1
        raw = json.dumps(value, ensure_ascii=False)
        if len(raw.encode()) > 16384:
            value = {'truncated': True, 'source_bytes': len(raw.encode()), 'preview': raw.encode()[:8192].decode('utf-8', errors='ignore'), 'ticket': ticket}
        while self.events and sum(len(json.dumps(v).encode()) for v in self.events) + len(json.dumps(value).encode()) > 131072:
            self.events.popleft(); self.dropped += 1
        self.events.append(value)

    def drain(self):
        try:
            self._drain()
        except (ValueError, TypeError, OSError):
            self.stop()
            raise

    def _drain(self):
        # Work bounded independently of an adapter that emits without stopping.
        read_bytes = 0
        for _ in range(256):
            if self.protocol == 'acp':
                end = self.buffer.find(b'\n')
                if end >= 0:
                    body, self.buffer = self.buffer[:end], self.buffer[end+1:]
                    self.receive(json.loads(body)); continue
            else:
                end = self.buffer.find(b'\r\n\r\n')
                if end >= 0:
                    headers = dict(line.split(b':', 1) for line in self.buffer[:end].split(b'\r\n'))
                    length = int(next((v for k, v in headers.items() if k.lower() == b'content-length'), b'-1'))
                    if not 0 <= length <= CAP:
                        raise ValueError('Invalid DAP frame length')
                    if len(self.buffer) >= end + 4 + length:
                        body, self.buffer = self.buffer[end+4:end+4+length], self.buffer[end+4+length:]
                        self.receive(json.loads(body)); continue
            if read_bytes >= CAP or not select.select([self.child.stdout], [], [], 0)[0]:
                break
            chunk = os.read(self.child.stdout.fileno(), 16384)
            if not chunk:
                self.stop()
                break
            read_bytes += len(chunk)
            self.buffer += chunk
            if len(self.buffer) > CAP + 8192:
                raise ValueError('Adapter frame buffer exceeded bound')

    def poll(self):
        self.drain()
        events = list(self.events); self.events.clear()
        return {'adapter': self.name, 'protocol': self.protocol, 'ready': self.ready, 'capabilities': self.capabilities, 'pending': sorted(self.pending), 'events': events, 'omitted_events': self.dropped, 'exit_code': self.child.returncode}

    def stop(self):
        if self.stopped:
            return
        self.stopped = True
        # Descendants can outlive an exited adapter; stop the owned group too.
        try:
            os.killpg(self.child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            self.child.wait(timeout=.3)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(self.child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.child.wait()
        self.child.stdin.close(); self.child.stdout.close()


def main():
    adapters = {}; workspace = os.getcwd()
    try:
        for line in sys.stdin:
            req = json.loads(line); kind = req.get('type'); reply = {'id': req.get('id')}
            try:
                if kind == 'hello':
                    workspace = req.get('workspace') or workspace
                    reply.update(protocol=1, name='adapters', version='1.0.0', tools=[{'name': 'adapter', 'description': 'Optional lazy ACP/DAP client: configured names only; start, request, poll, stop. Long operations return tickets. Poll events and initialization capabilities before further requests.', 'concurrency': 'serial', 'schema': {'type': 'object', 'properties': {'action': {'enum': ['list', 'start', 'request', 'poll', 'stop']}, 'name': {'type': 'string'}, 'method': {'type': 'string'}, 'params': {'type': 'object'}}, 'required': ['action']}}], commands=[{'name': 'adapters', 'description': 'Inspect configured external harnesses and debuggers; no processes launched'}])
                elif kind == 'authorize':
                    reply['allow'] = True
                elif kind in ('run_tool', 'command'):
                    args = json.loads(req.get('arguments') or '{}') if kind == 'run_tool' else {'action': 'list'}
                    action, name = args.get('action'), args.get('name')
                    if action == 'list':
                        result = {'configured': {n: {'protocol': s.get('protocol'), 'running': n in adapters} for n, s in config().items()}}
                    elif action == 'start':
                        if name not in adapters:
                            if len(adapters) >= 4: raise ValueError('Four adapters maximum')
                            spec = config().get(name)
                            if spec is None: raise ValueError('Unknown configured adapter')
                            adapters[name] = Adapter(name, spec, workspace)
                        result = adapters[name].poll()
                    elif name not in adapters:
                        raise ValueError('Start the named adapter first')
                    elif action == 'request':
                        result = {'ticket': adapters[name].request(args.get('method'), args.get('params') or {}), 'status': 'pending', 'next': 'poll'}
                    elif action == 'poll': result = adapters[name].poll()
                    elif action == 'stop': adapters.pop(name).stop(); result = {'status': 'stopped'}
                    else: raise ValueError('Unknown adapter action')
                    reply['output'] = json.dumps(result, ensure_ascii=False)
                elif kind in ('goodbye', 'shutdown'): break
                elif kind == 'system_prompt': reply['text'] = ''
                else: pass
            except Exception as error:
                reply['output'] = 'error: ' + str(error)[:2000]
            print(json.dumps(reply), flush=True)
    finally:
        for adapter in adapters.values(): adapter.stop()

if __name__ == '__main__': main()
