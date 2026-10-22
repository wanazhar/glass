#!/usr/bin/env python3
"""Release CLI design and resume-picker regressions through actual PTYs."""
import codecs, errno, fcntl, http.server, json, os, pty, re, select, struct
import subprocess, tempfile, termios, threading, time
from pathlib import Path
from pty_vt import VT

ROOT = Path(__file__).resolve().parents[1]
BIN = str(Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT/'target/release/glass-core')).resolve())
class Provider(http.server.BaseHTTPRequestHandler):
    requests=[]
    def log_message(self,*args): pass
    def do_POST(self):
        self.requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
        body=('data: '+json.dumps({'choices':[{'delta':{'content':'RESUME_OK'}}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
class TTY:
    def __init__(self, cwd, env, args, size):
        self.master,slave=pty.openpty();self.slave=slave;self.closed=False
        self.before=termios.tcgetattr(slave)
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',size[1],size[0],0,0))
        self.vt=VT(*size);self.raw='';self.decoder=codecs.getincrementaldecoder('utf-8')()
        self.process=subprocess.Popen([BIN,*args],cwd=cwd,env=env,stdin=slave,stdout=slave,stderr=slave)
    def read(self, seconds=.1):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            if not select.select([self.master],[],[],min(.05,max(0,end-time.monotonic())))[0]:continue
            try: data=os.read(self.master,65536)
            except OSError as e:
                if e.errno==errno.EIO:break
                raise
            if not data:break
            text=self.decoder.decode(data);self.raw+=text;self.vt.feed(text)
    def wait(self,text,timeout=10):
        end=time.monotonic()+timeout
        while time.monotonic()<end:
            self.read()
            if text in self.vt.text():return
            if self.process.poll() is not None:break
        raise AssertionError((text,self.raw,self.vt.text()))
    def send(self,text):os.write(self.master,text.encode())
    def finish(self):
        end=time.monotonic()+10
        while self.process.poll() is None and time.monotonic()<end:self.read()
        assert self.process.poll() is not None,self.raw
        self.read(.1)
        assert termios.tcgetattr(self.slave)[3]&termios.ICANON == self.before[3]&termios.ICANON
        code=self.process.returncode;os.close(self.master);os.close(self.slave);self.closed=True
        return code,self.raw
    def close(self):
        if self.closed:return
        if self.process.poll() is None:
            self.send('\x03');self.read(.2);self.send('\x04');self.read(.2)
        if self.process.poll() is None:self.process.terminate();self.process.wait(timeout=5)
        os.close(self.master);os.close(self.slave)

def plain(raw):return re.sub(r'\x1b\[[0-9;]*m','',raw).replace('\r','')
with tempfile.TemporaryDirectory(prefix='gc-cli-') as temp:
    root=Path(temp);work=root/'project';work.mkdir();other=root/'other';other.mkdir()
    cfg=root/'config/glass-core';cfg.mkdir(parents=True);data=root/'data/glass-core';data.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='xterm-256color');env.pop('NO_COLOR',None)
    (work/'sample.json').write_text('{}\n')
    (work/'sample.rs').write_text('fn main() {\n    let    value = 1;\n\tprintln!("hello");\n}\n')
    def fixture(id,title,workspace,stamp):
        path=data/(id+'.jsonl');path.write_text(json.dumps({'k':'user','t':id+' prompt'})+'\n'+json.dumps({'k':'assistant','t':'Saved response'})+'\n')
        path.with_suffix('.meta.json').write_text(json.dumps({'workspace':str(workspace),'title':title,'attachments':[]}));os.utime(path,(stamp,stamp))
    fixture('alpha','Alpha task',work,100);fixture('beta','Beta task',work,200);fixture('hidden','Hidden other project',other,300)
    def pipe(*args):return subprocess.run([BIN,*args],cwd=work,env=env,capture_output=True,text=True,timeout=15)
    for args in [('status','--json'),('doctor','--json'),('sessions','--json'),('profiles','--json'),('extensions','--json'),('file','read','sample.rs','--json'),('worktree','list','--json'),('plugin','list','--json'),('context','alpha','--json'),('details','alpha','--json'),('review','alpha','--json')]:
        result=pipe(*args);assert result.returncode==0,(args,result.stderr);json.loads(result.stdout);assert '\x1b' not in result.stdout
    assert pipe('resume').returncode==1
    assert 'terminal' in pipe('resume').stderr
    assert pipe('export','alpha').stdout.startswith('#'),pipe('export','alpha').stdout
    # Literal --help remains a prompt after run's end-of-options marker.
    result=pipe('run','--','--help');assert result.returncode==0 and 'RESUME_OK' in result.stdout,(result.stdout,result.stderr)
    assert any('--help' in json.dumps(request) for request in Provider.requests)
    commands=['resume','status','doctor','sessions','list','search','context','review','details','profiles','extensions','init','export','plugin','worktree','send','serve','attach','stop','respawn','logs','acp','run','agent','file']
    for command in commands:
        result=pipe(command,'--help');assert result.returncode==0 and result.stdout,(command,result.stderr)
        assert pipe('help',command).returncode==0,command
    # All help is available without a usable provider configuration.
    original=(cfg/'config.toml').read_text();(cfg/'config.toml').write_text('not valid toml ===')
    for command in commands:assert pipe(command,'--help').returncode==0,command
    (cfg/'config.toml').write_text(original)
    for size in [(80,30),(40,24),(20,8)]:
        outputs=[['status'],['doctor'],['sessions'],['list'],['search','prompt'],['profiles'],['extensions'],['plugin','list'],['plugin','verify'],['worktree','list'],['file','read','sample.rs'],['file','check','sample.json'],['context','alpha'],['review','alpha'],['details','alpha'],['logs','alpha']]
        for args in outputs+[[c,'--help'] for c in commands]:
            t=TTY(work,env,args,size)
            try:
                code,raw=t.finish();assert code==0,(args,raw)
                assert 'glass-core' in raw,(args,raw)
                if size==(80,30) and args in [['status'],['doctor']]: (Path('/tmp')/('gc-cli-preview-'+args[0]+'.txt')).write_text(plain(raw))
                assert '\x1b[' in raw,(args,raw)
                assert '?1049' not in raw,(args,raw)
                if size==(80,30) and args==['file','read','sample.rs']:
                    assert '2|    let    value' in plain(raw) and '3|    println!' in plain(raw),plain(raw)
                assert all(len(row)<=size[0] for row in plain(raw).splitlines()),(size,args,plain(raw))
            except Exception:t.close();raise
        for command in ['status','doctor']:
            t=TTY(work,env,[command,'--json'],size);code,raw=t.finish();assert code==0 and '\x1b' not in raw;json.loads(raw)
        t=TTY(work,env,['file','read','missing.rs'],size);code,raw=t.finish();assert code==1 and 'Error' in raw and '\x1b[38;5;203m' in raw
        # NO_COLOR still keeps the responsive terminal layout.
        t=TTY(work,dict(env,NO_COLOR='1'),['status'],size);code,raw=t.finish();assert code==0 and '\x1b' not in raw
        before={p.name:p.read_bytes() for p in data.glob('*.jsonl')}
        for cancel in ['\x1b','\x03']:
            requests_before=len(Provider.requests)
            t=TTY(work,env,['resume'],size)
            try:
                t.wait('Conversations');assert 'Hidden' not in t.vt.text();t.send(cancel);code,raw=t.finish();assert code==0
                assert '?1049l' in raw and '?2004l' in raw
                assert before=={p.name:p.read_bytes() for p in data.glob('*.jsonl')}
                assert len(Provider.requests)==requests_before, 'Cancel must not contact a provider'
            except Exception:t.close();raise
        t=TTY(work,env,['resume'],size)
        try:
            t.wait('Conversations');t.send('no-such-thread');t.wait('No matches');t.send('\x1b');assert t.finish()[0]==0
        except Exception:t.close();raise
        t=TTY(work,env,['resume'],size)
        try:
            t.wait('Conversations');t.send('Alpha');t.wait('Alpha');t.send('\r');t.wait('❯');t.send('continue\r')
            end=time.monotonic()+15
            while 'RESUME_OK' not in (data/'alpha.jsonl').read_text() and time.monotonic()<end:t.read()
            assert 'RESUME_OK' in (data/'alpha.jsonl').read_text(),t.raw
            assert 'continue' not in (data/'beta.jsonl').read_text()
            t.read(.3);t.send('\x04');assert t.finish()[0]==0
        except Exception:t.close();raise
        assert any('alpha prompt' in json.dumps(request) for request in Provider.requests)
        print(f'PASS CLI presentation and resume selection/cancel/search {size[0]}x{size[1]}')
    t=TTY(work,env,['init','--write'],(40,24));code,raw=t.finish();assert code==0 and 'wrote' in plain(raw),(code,raw)
    assert (work/'AGENTS.md').read_text().strip()=='RESUME_OK'
    empty=root/'empty';empty.mkdir();t=TTY(empty,env,['resume'],(20,8));t.wait('Conversations');t.wait('All projects');t.send('\x1b');code,raw=t.finish();assert code==0 and '?1049l' in raw
    server.shutdown();server.server_close()
print('PASS all command help, JSON, raw export, non-Git startup, headless guidance and empty project')
