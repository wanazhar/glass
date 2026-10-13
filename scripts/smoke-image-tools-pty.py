#!/usr/bin/env python3
"""Release PTY checks for native vision and image attachment removal; mock model."""
import http.server
import json
import os
import tempfile
import threading
from pathlib import Path
from pty_vt import Terminal
from importlib.util import spec_from_file_location, module_from_spec
spec=spec_from_file_location('image_fixture',Path(__file__).with_name('smoke-image.py'))
f=module_from_spec(spec);spec.loader.exec_module(f)
class Endpoint(http.server.BaseHTTPRequestHandler):
    viewed=False
    webstep=0
    requests=[]
    def log_message(self,*_): pass
    def do_GET(self):
        if self.path.startswith('/doc') or self.path.startswith('/results'):
            payload=(b'<p>Readable web fixture <a href="/guide">Guide</a></p><pre>if ready:\n    run()</pre>' if self.path.startswith('/doc') else b'<a class="result__a" href="https://example.com/docs">Example docs</a><div class="result__snippet">A useful source.</div>')
            self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload);return
        payload=json.dumps({'data':[{'id':'mock','name':'Mock vision'}]}).encode()
        self.send_response(200);self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        type(self).requests.append(body)
        messages=body['messages'];raw=json.dumps(messages)
        if '__WEB__' in raw:
            step=type(self).webstep;type(self).webstep+=1
            if step<2:
                name='web_fetch' if step==0 else 'web_search'
                args={'url':f'http://127.0.0.1:{self.server.server_port}/doc'} if step==0 else {'query':'fixture docs'}
                delta={'tool_calls':[{'index':0,'id':f'web_call_{step}','function':{'name':name,'arguments':json.dumps(args)}}]}
            else:
                assert '[web source' in raw and '[web search' in raw,raw
                delta={'content':'WEB_TOOLS_OK'}
        elif '__VIEW__' in raw and not type(self).viewed:
            type(self).viewed=True
            assert 'view_image' in json.dumps(body['tools']),body['tools']
            delta={'tool_calls':[{'index':0,'id':'image_call','function':{'name':'view_image','arguments':json.dumps({'path':'shot.png'})}}]}
        elif '__VIEW__' in raw:
            assert any(m.get('role')=='tool' and '[image ' in str(m.get('content')) for m in messages),messages
            assert 'data:image/jpeg;base64,' in raw, messages
            delta={'content':'NATIVE_IMAGE_OK'}
        else:
            delta={'content':f'IMAGE_COUNT={raw.count("data:image/jpeg;base64,")}'}
        data=('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
def main():
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    try:
        for size in [(80,24),(34,16),(20,8)]:
            Endpoint.viewed=False;Endpoint.webstep=0;Endpoint.requests=[]
            with tempfile.TemporaryDirectory(prefix='gc-image-pty-') as temp:
                root=Path(temp);(root/'shot.png').write_bytes(f.PNG)
                cfg=root/'config/glass-core';cfg.mkdir(parents=True)
                (cfg/'addons.json').write_text(json.dumps({'enabled':['web','browser']}))
                (root/'web.json').write_text(json.dumps({'allow_private':True,'ddg_url':f'http://127.0.0.1:{server.server_port}/results'}))
                (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
                env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_CONFIG=str(root/'web.json'))
                terminal=Terminal(root,env,size)
                try:
                    terminal.idle()
                    for name in ['web','browser']:
                        bundled=cfg/'extensions/bundled'/f'{name}.py'
                        expected=Path(__file__).resolve().parents[1]/'extensions'/name/f'{name}.py'
                        assert bundled.read_bytes()==expected.read_bytes(),f'{name} embedded source is stale'

                    terminal.send('/attach shot.png\r');terminal.collect(.3);terminal.send('\r');terminal.collect(.3)
                    terminal.send('ATTACH_CHECK\r');terminal.wait('IMAGE_COUNT=1');terminal.collect(.3)
                    terminal.send('/context\r');terminal.collect(.3)
                    terminal.send('Remove');terminal.collect(.2);terminal.send('\r');terminal.collect(.3)
                    terminal.send('\x1b');terminal.collect(.3);terminal.send('\x1b');terminal.collect(.3)
                    terminal.send('ATTACH_CHECK\r');terminal.wait('IMAGE_COUNT=0');terminal.collect(.3)
                    terminal.send('__VIEW__\r');terminal.wait('NATIVE_IMAGE_OK');terminal.collect(.3)
                    assert 'view_image' in terminal.seen and 'base64' not in terminal.vt.text(),terminal.vt.text()
                    terminal.send('\x1b[5~');terminal.collect(.2)
                    terminal.send('\x1b[6~');terminal.collect(.2)
                    terminal.send('\x1b');terminal.collect(.2)
                    terminal.send('__WEB__\r');terminal.wait('WEB_TOOLS_OK');terminal.collect(.3)
                    assert terminal.p.poll() is None
                    print(f'PASS release PTY {size[0]}×{size[1]}: attachment removal, native image pixels, cards, scroll return, web source/search tools')
                finally:
                    terminal.close()
    finally:
        server.shutdown()
if __name__=='__main__':main()
