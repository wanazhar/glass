#!/usr/bin/env python3
"""Offline regressions for web extraction, deadlines, caching and network policy."""
import importlib.util
import http.server
import json
import os
import tempfile
import threading
import time
from pathlib import Path
spec = importlib.util.spec_from_file_location('gc_web',Path(__file__).resolve().parents[1]/'extensions/web/web.py')
w = importlib.util.module_from_spec(spec);spec.loader.exec_module(w)
class Fixture(http.server.BaseHTTPRequestHandler):
    hits = 0
    auth = []
    target = None
    def log_message(self,*_): pass
    def do_GET(self):
        type(self).hits += 1
        type(self).auth.append((self.path,self.headers.get("Authorization")))
        if self.path == "/cross":
            self.send_response(302);self.send_header("Location",self.target+"/page");self.end_headers();return
        if self.path == '/redirect':
            self.send_response(302);self.send_header('Location','/page');self.end_headers();return
        if self.path == '/binary':
            self.send_response(200);self.send_header('Content-Type','image/png');self.end_headers();self.wfile.write(b'png');return
        self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers()
        if self.path == '/slow':
            try:
                for _ in range(30): self.wfile.write(b'x');self.wfile.flush();time.sleep(.05)
            except (BrokenPipeError,ConnectionResetError): pass
        else:
            self.wfile.write(('<p><a href="/guide">Guide</a></p><pre>if ready:\n    run()\n    done()</pre><p>'+('hello '*1000)+'</p>').encode())
def main():
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    base=f'http://127.0.0.1:{server.server_port}'
    other=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture)
    threading.Thread(target=other.serve_forever,daemon=True).start()
    Fixture.target=f'http://127.0.0.1:{other.server_port}'
    with tempfile.TemporaryDirectory() as temp:
        path=Path(temp)/'web.json';os.environ['GC_WEB_CONFIG']=str(path)
        path.write_text('{}')
        assert 'private/local destination blocked' in w.web_fetch(base+'/page')
        path.write_text(json.dumps({'allow_private':True}))
        text=w.web_fetch(base+'/redirect',1000)
        assert base+'/guide' in text and '\n    run()\n    done()' in text,text
        assert base+'/page' in text and '[next:' in text,text
        ref=text.split(' · ')[1]
        hits=Fixture.hits
        more=w.web_fetch(ref,1000,offset=1000)
        assert 'chars 1000' in more and Fixture.hits==hits,more
        assert 'No match' in w.web_fetch(ref,1000,find='absent')
        assert 'unsupported content type image/png' in w.web_fetch(base+'/binary')
        assert 'credentials in URLs' in w.web_fetch('http://user:password@example.com/')
        assert w.web_fetch('file:///etc/passwd').startswith('error:')
        assert w.handle_run_tool({'name':'web_fetch','arguments':'[]'}).startswith('error:')
        path.write_text(json.dumps({'allow_private':True,'search':'searxng','searxng_url':base,'api_key':'fixture-token'}))
        Fixture.auth=[]
        assert not w.web_fetch(base+'/cross',refresh=True).startswith('error:')
        assert Fixture.auth==[('/cross','Bearer fixture-token'),('/page',None)],Fixture.auth
        Fixture.auth=[]
        w.web_fetch(Fixture.target+'/page',refresh=True)
        assert Fixture.auth==[('/page',None)],Fixture.auth
        path.write_text(json.dumps({'allow_private':True,'search':'searxng','searxng_url':base,'api_key':'sensitive-fixture\nheader'}))
        invalid=w.web_fetch(base+'/page',refresh=True)
        assert invalid.startswith('error:') and 'sensitive-fixture' not in invalid,invalid
        path.write_text(json.dumps({'allow_private':True}))
        w.FETCH_TIMEOUT=.25
        start=time.monotonic();slow=w.web_fetch(base+'/slow')
        assert slow.startswith('error:') and time.monotonic()-start<1.0,slow
        parser=w.DuckResults();parser.feed('<a class="result__a" href="https://example.com">A <span>nested</span> title</a><div class="result__snippet">Start <span>middle</span> end</div>')
        assert parser.results[0]['title']=='A nested title' and parser.results[0]['snippet']=='Start middle end',parser.results
    server.shutdown()
    other.shutdown()
    print('PASS web reliability: relative links, code, redirects, references, pagination, cache, content types, destination policy, malformed arguments, nested snippets, total deadline')
if __name__=='__main__': main()
