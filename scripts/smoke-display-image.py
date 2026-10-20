#!/usr/bin/env python3
"""Native display tool and text-only recovery through real provider requests."""
import base64,http.server,importlib.util,json,os,subprocess,tempfile,threading
from pathlib import Path
spec=importlib.util.spec_from_file_location('image_fixture',Path(__file__).with_name('smoke-image.py'))
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY','target/debug/glass-core')).resolve()
class Server(http.server.ThreadingHTTPServer): daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
        request=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(request)
        raw=json.dumps(request);messages=request['messages']
        names=[item['function']['name'] for item in request['tools']]
        assert 'display_image' in names,names
        has_image='data:image/' in raw
        if self.server.mode=='recover' and has_image:
            self.send_response(400);self.send_header('Content-Type','application/json');self.end_headers()
            self.wfile.write(json.dumps({'error':{'message':'Beam supports text input only. This conversation contains an image.'}}).encode());return
        assert not has_image, 'Text-only provider received pixels'
        if self.server.mode=='recover':
            assert 'view_image' not in names
            assert 'visual contents' in raw.lower()
            delta={'content':'TEXT_ONLY_RECOVERED'}
        elif not any(m.get('role')=='tool' and m.get('name')=='display_image' for m in messages):
            assert 'view_image' not in names
            delta={'tool_calls':[{'index':0,'id':'share','function':{'name':'display_image','arguments':json.dumps({'path':'ui #1.png'})}}]}
        else:
            assert '[display image]' in raw and 'ui%20%231.png' in raw
            assert not (self.server.root/'data/glass-core/images').exists(), 'Display unexpectedly created vision artifacts'
            delta={'content':'![Image](ui%20%231.png)\n\nShared the file; I have not inspected its contents.'}
        payload=('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers();self.wfile.write(payload)
for mode in ['display','recover']:
    with tempfile.TemporaryDirectory(prefix='gc-display-image-') as tmp:
        root=Path(tmp);(root/'ui #1.png').write_bytes(f.PNG);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        server=Server(('127.0.0.1',0),Endpoint);server.mode=mode;server.root=root;server.requests=[]
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock-text"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\nimage_input={str(mode=="recover").lower()}\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        args=[str(BIN),'run','--json']+(['--attach','ui #1.png'] if mode=='recover' else [])+['Share the image file without inspecting it.']
        try:
            result=subprocess.run(args,cwd=root,env=env,capture_output=True,text=True,timeout=30)
            assert result.returncode==0,(result.stdout,result.stderr)
            assert len(server.requests)==2,len(server.requests)
            if mode=='display':assert '![Image]' in result.stdout,result.stdout
            else:assert 'TEXT_ONLY_RECOVERED' in result.stdout,result.stdout
            print('PASS display-only provider wire' if mode=='display' else 'PASS single text-only rejection recovery preserves attachments without resending pixels')
        finally:server.shutdown();server.server_close()
