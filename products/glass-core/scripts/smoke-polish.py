#!/usr/bin/env python3
"""Offline PTY and non-TTY daily-use checks, including narrow screens and resize."""
import importlib.util
import http.server
import json
import os
import re
import select
from pathlib import Path
import subprocess
import tempfile
import threading
import time
spec=importlib.util.spec_from_file_location('features',Path(__file__).with_name('smoke-features.py'))
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
class Endpoint(http.server.BaseHTTPRequestHandler):
    mode='normal';requests=[];task_command='';calls=0
    def log_message(self,*_):pass
    def do_GET(self):
        data=json.dumps({'data':[{'id':'mock','name':'Mock model'}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));type(self).requests.append(body);type(self).calls+=1
        if self.mode=='transient':
            type(self).mode='normal';self.send_response(503);self.send_header('Content-Length','0');self.end_headers();return
        if self.mode=='waiting':time.sleep(3)
        messages=body['messages'];results=[m for m in messages if m['role']=='tool']
        mode=self.mode
        if mode in ('tool','recovery','slow'):
            ident='recover2' if mode=='recovery' else ('slow-id-'+str(type(self).calls) if mode=='slow' else 'first1')
            if not any(m.get('tool_call_id')==ident for m in results):
                frames=[{'choices':[{'delta':{'tool_calls':[{'index':0,'id':ident,'function':{'name':'bash','arguments':json.dumps({'command':type(self).task_command})}}]}}]}]
            elif mode=='tool': frames=[{'error':{'message':'simulated failure after completed tool'}}]
            else:frames=[{'choices':[{'delta':{'content':'Recovered answer.'}}]}]
        elif mode=='partial':frames=[{'choices':[{'delta':{'content':'Partial answer retained.'}}]},{'error':{'message':'simulated stream failure'}}]
        else:frames=[{'choices':[{'delta':{'content':'Polished reply.'}}]}]
        if mode=='priced':frames.append({'choices':[],'usage':{'prompt_tokens':100,'completion_tokens':5}})
        data=(''.join('data: '+json.dumps(x)+'\n\n' for x in frames)+'data: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(data)));self.end_headers()
        try:self.wfile.write(data)
        except (BrokenPipeError,ConnectionResetError):pass

def key(t,s):os.write(t.master,s.encode())
def choose(t,s):key(t,s+'\r')
def back(t):
    t.received='';key(t,'\x1b');t.wait('\x1b[?25h')

def wait_wrapped(t, text):
    # Wait for the whole phrase, normalized: wrapping inserts cursor
    # sequences between words, and a pty read can split mid-line, so
    # matching on the first word alone loses the tail.
    deadline = time.monotonic() + 8
    while True:
        plain = re.sub(r"\x1b\[[0-9;?]*[ -/]*[@-~]", "", t.received)
        if text in ' '.join(plain.split()):
            break
        assert time.monotonic() < deadline, (text, plain)
        if select.select([t.master], [], [], 0.05)[0]: t.read()
    t.idle()

def main():
    with tempfile.TemporaryDirectory(prefix='glass-polish-smoke-') as d:
        root=Path(d);repo=root/'repo';repo.mkdir();subprocess.run(['git','init','-q',str(repo)],check=True)
        (repo/'note.txt').write_text('ATTACHED-SNAPSHOT-CONTENT')
        (repo/'AGENTS.md').write_text('PROJECT-INSTRUCTION-CONTENT')
        cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        editor=root/'editor.sh';editor.write_text('#!/bin/sh\nprintf "external editor draft" > "$1"\n');editor.chmod(0o700);env['EDITOR']=str(editor)
        fake_bin=root/'bin';fake_bin.mkdir();opener=fake_bin/'xdg-open';opener.write_text('#!/bin/sh\nexit 0\n');opener.chmod(0o700);env['PATH']=str(fake_bin)+':'+env['PATH']
        def cli(*args,input=None,ok=True,cwd=repo):
            p=subprocess.run([str(f.BINARY),*args],input=input,cwd=cwd,env=env,text=True,capture_output=True,timeout=15)
            assert '\x1b' not in p.stdout and '\x1b' not in p.stderr,p
            if ok:assert p.returncode==0,p
            else:assert p.returncode!=0,p
            return p,json.loads(p.stdout)
        try:
            p,r=cli('run','--json','--attach','note.txt','seeded conversation');sid=r['session'];assert r['status']=='finished'
            assert 'ATTACHED-SNAPSHOT-CONTENT' in json.dumps(Endpoint.requests[-1])
            p,r=cli('run','--json','--resume',sid,input='ssh stdin follow-up');assert r['session']==sid
            assert 'seeded conversation' in json.dumps(Endpoint.requests[-1])
            for command in ['sessions','status','context','review','details']:
                args=[command,'--json']+([] if command in ('sessions','status') else [sid]);cli(*args)
            p,r=cli('export',sid,'--json');assert r['session']==sid and r['written'] is False and r['bytes']>0,r
            outp=repo/'export-check.md';p,r=cli('export',sid,'--out',str(outp),'--thinking','--json');assert r['written'] and outp.exists(),r
            body=outp.read_text();assert body.startswith('# glass-core session '+sid),body;assert 'seeded conversation' in body,body
            p2=subprocess.run([str(f.BINARY),'export',sid],cwd=repo,env=env,text=True,capture_output=True,timeout=15)
            assert p2.returncode==0 and p2.stdout.startswith('# glass-core session'),p2
            _,r=cli('run','--json','--budget','invalid','hi',ok=False);assert r['status']=='error'
            other=root/'other';other.mkdir();_,r=cli('sessions','--json',cwd=other);assert not r['sessions']
            cli('run','--json','--resume',sid,'hi',cwd=other,ok=False)
            for size in [(80,24),(40,12),(24,10),(20,8)]:
                t=f.Terminal(repo,env,size=size)
                try:
                    t.idle();key(t,'x');t.visible('❯ x');key(t,'\x03');t.idle()
                    # Escape must erase the managed palette, not merely repaint
                    # the composer below a stale modal in the middle of the screen.
                    for query in ('/', '/not-a-command'):
                        key(t,query);t.visible('Search actions');back(t);t.idle();assert ('❯ '+query) not in t.screen.text(), t.screen.text()
                        assert 'Search actions' not in t.screen.text(), t.screen.text()
                        assert 'No matching commands' not in t.screen.text(), t.screen.text()
                        key(t,'\x03');t.idle()
                    key(t,'/');t.visible('Search actions');key(t,'context');t.visible('/context');key(t,'\r');t.visible('Context');t.visible('Inspect')
                    key(t,'\r');t.visible('Context details');back(t);t.visible('Inspect');back(t)
                    # A palette-dispatched modal restores its parent palette;
                    # a separate Escape closes that palette back to the draft.
                    t.visible('Search actions');back(t);t.idle()
                    # A palette-dispatched modal keeps a non-command draft.
                    key(t,'draft');key(t,'\x10');t.visible('Search actions');key(t,'status\r');t.visible('Connection');t.dismiss();t.visible('❯ draft');key(t,'\x03');t.idle()
                    # File/path picker and snapshot attachment stay usable on a small screen.
                    t.send('/attach');t.visible('Files');key(t,'note.txt');t.visible('› note.txt');key(t,'\x1bOP');t.visible('Selection details');back(t);t.visible('Files');key(t,'\r');t.visible('Attach context');key(t,'\r');t.wait('Attached note.txt');t.idle()
                    t.send('attachment check');t.wait('Polished reply.');t.idle();assert 'ATTACHED-SNAPSHOT-CONTENT' in json.dumps(Endpoint.requests[-1])
                    t.send('/context');t.visible('Context');t.received='';choose(t,'Remove');t.wait('\x1b[2J');t.visible('Context');back(t);t.visible('Search actions');back(t);t.idle();t.send('attachment removed');t.wait('Polished reply.');t.idle();assert 'ATTACHED-SNAPSHOT-CONTENT' not in json.dumps(Endpoint.requests[-1])
                    t.send('/review');t.visible('Task review');key(t,'\x1b[F');t.dismiss();t.idle()
                    key(t,'\x12');t.visible('Prompt history');choose(t,'attachment check');t.visible('❯ attachment');key(t,'\x03');t.idle()
                    # Paste newlines and edit without submitting, even past the viewport.
                    key(t,'\x1b[200~one\ntwo\nthree\nfour\nfive\nsix\x1b[201~');t.visible('six');assert t.screen.saved is not None
                    key(t,'\x03');t.idle()
                    t.send('/model');t.visible('Provider');back(t);t.visible('Search actions');back(t);t.idle()
                    t.send('/login');t.visible('Connect a provider');choose(t,'Continue with ChatGPT');t.visible('sign-in' if size[0]<50 else 'Continue with ChatGPT');key(t,'l');t.visible('ChatGPT login link');back(t);t.visible('sign-in' if size[0]<50 else 'Continue with ChatGPT');key(t,'p');t.visible('Paste the final');t.dismiss();t.idle()
                    key(t,'\x07');t.visible('following');key(t,'\x1b[H');t.visible('attachment');t.visible('check');t.dismiss();t.idle()
                    assert not t.screen.invalid_positions,(size,t.screen.invalid_positions)
                finally:t.close()
            t=f.Terminal(repo,env)
            try:
                t.idle();key(t,'resize draft');t.visible('❯ resize draft');t.resize(20,8);t.visible('❯ resize draft');t.received='';key(t,'\x10');t.visible('Search actions');t.resize(40,12);t.visible('Search actions');t.dismiss();t.visible('❯ resize draft');key(t,'\x03');t.idle()
                key(t,'\x18');t.visible('external editor');key(t,'\x03');t.idle()
                # Interactive session actions: preview, rename, resume and branch.
                t.send('/sessions');t.visible('Conversations');choose(t,'seeded conversation');t.visible('Conversation');key(t,'p');t.visible('Log');back(t);t.visible('Conversation')
                key(t,'n');t.visible('Rename conversation');key(t,'\x15Renamed seed\r');t.visible('Conversations');choose(t,'Renamed seed');t.visible('Status: finished · model: mock');key(t,'r');t.wait('Resumed Renamed seed');t.idle()
                t.send('/sessions');t.visible('Conversations');choose(t,'Renamed seed');t.visible('Conversation');key(t,'f');t.visible('Fork conversation');key(t,'\r');t.wait('Resumed Renamed seed (branch)');t.idle()
                t.send('/sessions');t.visible('Conversations');choose(t,sid);t.visible('Conversation');key(t,'d');t.visible('Delete conversation?');choose(t,'Delete permanently');t.visible('Conversations');back(t);t.visible('Search actions');back(t);t.idle();assert not (root/'data/glass-core'/f'{sid}.jsonl').exists()
                Endpoint.mode='transient';before=Endpoint.calls;t.send('transient test');t.wait('retry 1');t.wait('Polished reply.');t.idle();assert Endpoint.calls==before+2
                Endpoint.mode='partial';t.send('partial test');wait_wrapped(t,'simulated stream failure');t.idle()
                count=Endpoint.calls;Endpoint.mode='normal';t.send('/retry');t.visible('Recover response');choose(t,'Retry response')
                # A repaint can re-show earlier text, so wait for the retry
                # request itself before trusting any streamed reply.
                deadline=time.monotonic()+8
                while Endpoint.calls==count: assert time.monotonic()<deadline,'retry request never sent'; time.sleep(.02)
                t.wait('Polished reply.');t.idle();assert 'Partial answer retained.' in json.dumps(Endpoint.requests[-1])
                # A repeated completed action under a new call ID is served from its prior result.
                counter=root/'counter';Endpoint.task_command=f"python3 -c 'from pathlib import Path; p=Path(\"{counter}\"); p.write_text(p.read_text()+\"x\" if p.exists() else \"x\")'"
                Endpoint.mode='tool';t.send('recovery test');wait_wrapped(t,'simulated failure after completed tool');t.idle();assert counter.exists(),Endpoint.requests[-1];assert counter.read_text()=='x'
                Endpoint.mode='recovery';t.send('/retry');t.visible('Recover response');choose(t,'Retry response');t.wait('Recovered answer.');t.idle();assert counter.read_text()=='x'
                t.send('/details');t.visible('Outputs');key(t,'\x1b[B');key(t,'\r');t.visible('Shell · exit 0');t.visible('python3 -c');back(t);t.visible('Outputs');back(t);t.visible('Search actions');back(t);t.idle()
                Endpoint.mode='waiting';t.send('wait cancellation');t.wait('Waiting for provider');key(t,'queued follow-up\x1b\r');t.wait('queued');start=time.monotonic();key(t,'\x03');t.wait('Cancelled');t.idle();assert time.monotonic()-start<2
                script=root/'slow.sh';pid=root/'owned.pid';script.write_text(f'#!/bin/sh\necho $$ > "{pid}"\nexec sleep 10\n');script.chmod(0o700)
                Endpoint.mode='slow';Endpoint.task_command=str(script);t.send('tool cancellation');t.wait('bash running');deadline=time.monotonic()+3
                while not pid.exists():assert time.monotonic()<deadline;time.sleep(.01)
                child=int(pid.read_text());key(t,'\x03');t.wait('Cancelled');t.idle();assert not f.process_exists(child)
                assert not t.screen.invalid_positions,t.screen.invalid_positions
            finally:t.close()
            Endpoint.mode='partial';_,failed=cli('run','--json','headless partial recovery',ok=False)
            assert failed['text']=='Partial answer retained.'
            Endpoint.mode='normal';_,recovered=cli('run','--json','--resume',failed['session'],'--retry','--model','mock')
            assert recovered['status']=='finished' and recovered['session']==failed['session']
            assert 'Partial answer retained.' in json.dumps(Endpoint.requests[-1])
            cli('run','--json','--retry',ok=False)
            cli('run','--json','--','--resume is part of the prompt')
            # No terminal is required to interrupt an owned shell and preserve clean JSON.
            for sig in [f.signal.SIGINT,f.signal.SIGTERM]:
                pid.unlink(missing_ok=True);Endpoint.mode='slow';Endpoint.task_command=str(script)
                p=subprocess.Popen([str(f.BINARY),'run','--json','signal test'],cwd=repo,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                deadline=time.monotonic()+5
                while not pid.exists():assert time.monotonic()<deadline;time.sleep(.01)
                child=int(pid.read_text());p.send_signal(sig);out,err=p.communicate(timeout=5)
                assert p.returncode==128+sig,(p.returncode,out,err)
                assert json.loads(out)['status']=='cancelled' and '\x1b' not in out
                assert not f.process_exists(child)
            # Metering must finish before headless JSON is emitted, including outside Git.
            Endpoint.mode='priced'
            with (cfg/'config.toml').open('a') as file:file.write('\n[pricing]\ninput=1.0\ncache_read=1.0\ncache_write=1.0\noutput=2.0\n')
            for _ in range(10):
                _,metered=cli('run','--json','metering outside git',cwd=other)
                assert abs(metered['cost_usd']-0.00011)<1e-10,metered
            print('PASS: PTY at 80x24, 40x12, 24x10, 20x8; resize; draft preservation; paste; external editor; history; files/context; session preview/rename/resume/branch/delete; review/details; partial recovery; no repeated completed tools; cancellation before first token and owned shell cleanup; headless JSON/stdin/resume/retry/project isolation/errors/SIGINT/SIGTERM/non-Git metering')
        finally:server.shutdown();server.server_close()
if __name__=='__main__':main()
