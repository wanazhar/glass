#!/usr/bin/env python3
"""Opt-in loopback companion: authenticated browser client of GC's Unix protocol.
No provider or tools run here. The serving GC process owns history and effects.
"""
import collections
import hmac
import http.server
import json
import secrets
import select
import socket
import sys
import threading
import urllib.parse

HTML = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>glass-core</title><style>
:root{color-scheme:dark;font:16px/1.6 ui-monospace,monospace;background:#191b27;color:#e2e4f0}body{max-width:880px;margin:auto;padding:16px}header{display:flex;gap:16px;align-items:center;color:#aab5ff;position:sticky;top:0;background:#191b27}button{border:1px solid #5c68aa;border-radius:8px;background:#292d47;color:#dddfff;padding:8px 12px;cursor:pointer}article{white-space:pre-wrap;overflow-wrap:anywhere;padding:14px;border-left:3px solid #525a85;margin:12px 0;background:#202333;border-radius:6px}.user{border-color:#64d6ff;color:#8ddfff}.tool,.tool_end{border-color:#b79dff;color:#c4afff}.error,.provider_error{border-color:#ff7189;color:#ff98ab}form{position:sticky;bottom:0;background:#191b27;padding:12px 0;display:flex;gap:8px}textarea{flex:1;min-width:0;background:#25293c;color:#f0efff;border:1px solid #5c68aa;border-radius:8px;padding:10px;font:inherit}small{color:#a9abc4}#state{flex:1}button:focus-visible,textarea:focus-visible{outline:2px solid #65d8ff}</style><header><b>◆ glass-core</b><small id="state">Connecting…</small><button id="claim">Claim control</button></header><main id="chat" aria-live="polite"></main><form><textarea aria-label="Message" placeholder="Send a follow-up" rows="2"></textarea><button>Send</button><button type="button" id="cancel">Cancel</button></form><script>
const token=location.hash.slice(1);history.replaceState(null,'',location.pathname);let cursor=0;let busy=false;const chat=document.querySelector('#chat'),state=document.querySelector('#state');
async function api(path,data){const r=await fetch('/api/'+path,{method:data?'POST':'GET',headers:{Authorization:'Bearer '+token,'Content-Type':'application/json'},body:data?JSON.stringify(data):undefined});const j=await r.json();if(!r.ok)throw Error(j.error||r.status);return j;}
function row(role,text){const a=document.createElement('article');a.className=role;a.textContent=text;chat.append(a);while(chat.children.length>500)chat.firstChild.remove();}
async function poll(){if(busy)return;busy=true;try{const data=await api('events?since='+cursor);if(data.reset){chat.replaceChildren();row('error','Some live events were omitted. Reconnect replay or gc export retains conversation history.');}const follow=innerHeight+scrollY>=document.body.scrollHeight-120;for(const e of data.events){cursor=e.cursor;const m=e.message;if(m.t==='replay'){for(const x of m.entries||[])row(x.role,x.text);state.textContent=m.can_write?'Control owned':'Read-only observer';}else if(m.t==='control')state.textContent=m.can_write?'Control owned':'Read-only observer';else if(m.t==='text')row('text',m.delta||'');else if(['tool','tool_end','error','provider_error','done','retrying'].includes(m.t))row(m.t,m.command||m.summary||m.message||(m.t==='done'?'Turn '+m.stop:JSON.stringify(m)));}if(follow)scrollTo(0,document.body.scrollHeight);}catch(e){state.textContent=e.message;}finally{busy=false;}}
document.querySelector('#claim').onclick=()=>api('send',{t:'control',action:'claim'}).catch(e=>state.textContent=e.message);document.querySelector('#cancel').onclick=()=>api('send',{t:'cancel'}).catch(e=>state.textContent=e.message);document.querySelector('form').onsubmit=async e=>{e.preventDefault();const input=document.querySelector('textarea');if(!input.value.trim())return;try{await api('send',{t:'prompt',text:input.value,request_id:crypto.randomUUID()});row('user',input.value);input.value='';}catch(e){state.textContent=e.message;}};setInterval(poll,250);poll();</script></html>'''


def main():
    if len(sys.argv) not in (2, 3):
        raise SystemExit('gc companion SOCKET [PORT] · loopback browser observer; claim input explicitly')
    path = sys.argv[1]; port = int(sys.argv[2]) if len(sys.argv) == 3 else 8765
    stream = socket.socket(socket.AF_UNIX); stream.connect(path); stream.setblocking(False)
    token = secrets.token_urlsafe(32)
    lock = threading.Lock(); events = collections.deque(); seq = 0; buffered = b''; retained = 0
    def send(message):
        wire = (json.dumps(message)+'\n').encode()
        if len(wire) > 65536: raise ValueError('Input exceeds 64 KiB')
        at = 0
        while at < len(wire):
            if not select.select([], [stream], [], 1)[1]: raise TimeoutError('GC input stalled')
            at += stream.send(wire[at:])
    send({'t': 'attach', 'protocol_version': 1, 'observe': True})
    def drain():
        nonlocal buffered, seq, retained
        for _ in range(64):
            if b'\n' in buffered:
                line, buffered = buffered.split(b'\n', 1)
                value = json.loads(line); seq += 1
                size = len(line)
                while events and (len(events) >= 256 or retained + size > 1024*1024):
                    _, _, old = events.popleft(); retained -= old
                events.append((seq, value, size)); retained += size
                continue
            if not select.select([stream], [], [], 0)[0]: break
            chunk = stream.recv(16384)
            if not chunk: raise ConnectionError('GC disconnected; restart companion to replay retained history')
            buffered += chunk
            if len(buffered) > 2*1024*1024: raise ValueError('GC frame exceeds companion bound')
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def respond(self, code, body, kind='application/json'):
            data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
            self.send_response(code); self.send_header('Content-Type', kind); self.send_header('Content-Length', str(len(data))); self.send_header('Cache-Control', 'no-store'); self.send_header('X-Content-Type-Options', 'nosniff'); self.send_header('Referrer-Policy', 'no-referrer'); self.end_headers(); self.wfile.write(data)
        def authorized(self):
            origin = self.headers.get('Origin')
            allowed = {f'http://127.0.0.1:{self.server.server_port}', f'http://localhost:{self.server.server_port}'}
            return (origin is None or origin in allowed) and hmac.compare_digest(self.headers.get('Authorization', ''), 'Bearer '+token)
        def do_GET(self):
            if self.path == '/': return self.respond(200, HTML, 'text/html; charset=utf-8')
            if not self.authorized(): return self.respond(403, {'error': 'Authorization required'})
            if not self.path.startswith('/api/events?'): return self.respond(404, {'error': 'Not found'})
            try:
                since = int(urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query).get('since', ['0'])[0])
                with lock:
                    drain(); result = [{'cursor': i, 'message': m} for i, m, _ in events if i > since]
                    reset = bool(events and since and since < events[0][0]-1)
                self.respond(200, {'events': result, 'reset': reset})
            except Exception as error: self.respond(503, {'error': str(error)[:500]})
        def do_POST(self):
            if not self.authorized(): return self.respond(403, {'error': 'Authorization required'})
            if self.path != '/api/send': return self.respond(404, {'error': 'Not found'})
            try:
                length = int(self.headers.get('Content-Length', '-1'))
                if not 0 <= length <= 65536: raise ValueError('Input exceeds 64 KiB')
                value = json.loads(self.rfile.read(length))
                if value.get('t') not in ('prompt', 'cancel', 'control'): raise ValueError('Unsupported message')
                with lock: send(value)
                self.respond(202, {'status': 'sent_to_gc', 'note': 'GC acknowledgment determines acceptance'})
            except Exception as error: self.respond(400, {'error': str(error)[:500]})
    server = http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler)
    print(f'glass-core companion · http://127.0.0.1:{server.server_port}/#{token}', flush=True)
    print('Private access link; share only through an explicitly configured SSH tunnel. Ctrl-C closes this client, not GC.', flush=True)
    try: server.serve_forever()
    except KeyboardInterrupt: pass
    finally: server.server_close(); stream.close()

if __name__ == '__main__': main()
