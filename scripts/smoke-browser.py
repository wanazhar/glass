#!/usr/bin/env python3
"""Browser contract checks; optional real Chromium via GC_BROWSER_TEST_EXECUTABLE.
Default uses an injected Playwright fixture, so CI needs no browser dependency.
"""
import contextlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import types

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('browser_extension', ROOT/'extensions/browser/browser.py')
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)


def main():
    for bad in ('https://127.0.0.1:9222', 'http://example.org:9222', 'http://user@127.0.0.1:9222', 'file:///tmp/x'):
        try:
            b.endpoint(bad)
            raise AssertionError(bad)
        except ValueError: pass
    assert b.endpoint('http://127.0.0.1:9222')
    with tempfile.TemporaryDirectory(prefix='gc-browser-') as tmp:
        root=Path(tmp)
        class Page:
            url='http://127.0.0.1/example'
            first=None
            def __init__(self): self.first=self
            def on(self,*_): pass
            def goto(self,*_,**__): return types.SimpleNamespace(status=200)
            def get_by_text(self,*_,**__): return self
            def count(self): return 1
            def screenshot(self,**_): return b'fixture image'
        class Context:
            closed=False
            def new_page(self): return Page()
            def close(self): self.closed=True
        context=Context()
        class Browser:
            def new_context(self,**_): return context
            def close(self): raise AssertionError('existing browser must never be closed')
        @contextlib.contextmanager
        def driver(): yield types.SimpleNamespace(chromium=types.SimpleNamespace(connect_over_cdp=lambda *a,**k: Browser()))
        sys.modules['playwright.sync_api']=types.SimpleNamespace(sync_playwright=driver)
        os.environ['GC_BROWSER_CDP']='http://127.0.0.1:9222'
        result=json.loads(b.verify(root,{'url':'http://127.0.0.1/example','expected_text':'hello'}))
        assert result['passed'] and context.closed
        assert Path(result['screenshot']).stat().st_mode & 0o777 == 0o600
        assert Path(result['report']).stat().st_mode & 0o777 == 0o600
        assert (root/'.gc-artifacts').stat().st_mode & 0o777 == 0o700
        # Symlinked artifact storage cannot escape the workspace.
        linked=root/'linked'; linked.mkdir(); (linked/'.gc-artifacts').symlink_to(root/'.gc-artifacts',target_is_directory=True)
        try:
            b.verify(linked,{'url':'http://127.0.0.1/example'})
            raise AssertionError('artifact symlink accepted')
        except ValueError as error: assert 'real workspace directory' in str(error)
        del sys.modules['playwright.sync_api']
        executable=os.environ.get('GC_BROWSER_TEST_EXECUTABLE')
        if executable:
            class PageServer(http.server.BaseHTTPRequestHandler):
                def log_message(self,*_): pass
                def do_GET(self):
                    data=b'<h1>Browser verification ready</h1><script>console.log("fixture console")</script>'
                    self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),PageServer)
            threading.Thread(target=server.serve_forever,daemon=True).start()
            with socket.socket() as port_socket:
                port_socket.bind(('127.0.0.1',0)); port=port_socket.getsockname()[1]
            os.environ['GC_BROWSER_CDP']=f'http://127.0.0.1:{port}'
            with open(root/'browser.log','w') as log:
                process=subprocess.Popen([executable,'--headless','--no-sandbox','--disable-dev-shm-usage',f'--remote-debugging-port={port}',f'--user-data-dir={root / "profile"}','about:blank'],stdout=log,stderr=log)
                try:
                    deadline=time.monotonic()+10
                    while True:
                        try:
                            with socket.create_connection(('127.0.0.1',port),timeout=.2): break
                        except OSError:
                            assert process.poll() is None and time.monotonic()<deadline,(process.poll(),(root/'browser.log').read_text()[-2000:])
                            time.sleep(.1)
                    result=json.loads(b.verify(root,{'url':f'http://127.0.0.1:{server.server_port}','expected_text':'Browser verification ready'}))
                    assert result['passed'] and result['console'],result
                    assert Path(result['screenshot']).read_bytes().startswith(b'\x89PNG')
                    assert process.poll() is None,'verification closed the existing browser'
                    print('PASS real Chromium: page, expected text, screenshot, console, existing browser preserved')
                finally:
                    process.terminate()
                    try: process.wait(timeout=5)
                    except subprocess.TimeoutExpired: process.kill();process.wait()
                    server.shutdown()
    print('PASS browser: endpoint limits, isolated context cleanup, private artifacts, symlink refusal')


if __name__=='__main__':main()
