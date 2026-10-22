#!/usr/bin/env python3
"""Actual release CLI and small-screen modal checks; isolated files/mock provider."""
import http.server, json, os, subprocess, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path

BINARY=os.environ.get('GLASS_CORE_TEST_BINARY',str(core_binary_path('release')))
class Endpoint(http.server.BaseHTTPRequestHandler):
    calls=0
    def log_message(self,*args):pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']));Endpoint.calls+=1
        delta={'content':'Fixture answer.\n\n| Item | State |\n| --- | --- |\n| Native | Ready |'}
        data=('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

with tempfile.TemporaryDirectory(prefix='gc-roadmap-acceptance-') as tmp:
    root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);threading.Thread(target=server.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    def cli(*args,ok=True):
        p=subprocess.run([BINARY,*args],cwd=root,env=env,text=True,capture_output=True,timeout=15)
        assert (p.returncode==0)==ok,(args,p.stdout,p.stderr)
        return p
    try:
        session=json.loads(cli('run','--json','fixture request').stdout)['session'];calls=Endpoint.calls
        handoff=root/'handoff.json';cli('handoff',session,'--save',str(handoff),'--json')
        assert handoff.stat().st_mode&0o777==0o600
        assert json.loads(cli('handoff','import',str(handoff),'--json').stdout)['created'] is False
        imported=json.loads(cli('handoff','import',str(handoff),'--apply','--json').stdout)
        assert imported['created'] and not imported['executed']
        assert imported['session']!=session
        page=json.loads(cli('history',session,'--range','0','1','--json').stdout)
        assert page['entry_count']==1 and page['next_entry']==1
        cli('history',session,'--range','0','257',ok=False)
        cli('handoff',session,'--save',str(handoff),ok=False)
        cli('advisor',session,'cadence','2');cli('advisor',session,'cadence','off')
        cli('advisor',session,'cadence','0',ok=False)
        hub=json.loads(cli('hub','--json').stdout)
        assert any(t['session']==imported['session'] for t in hub['threads'])
        assert hub['history_loaded'] is False
        cli('pr','--help')
        findings=root/'findings.json';findings.write_text(json.dumps([{'severity':'medium','path':'sample.py','line':1,'title':'Fixture finding','evidence':'Observed fixture'}]))
        finding=json.loads(cli('workspace','findings','import',str(findings),'--json').stdout)['findings'][0]
        assert json.loads(cli('workspace','findings','show',str(finding),'--json').stdout)['title']=='Fixture finding'
        task=json.loads(cli('workspace','task','add','Fixture','--accept','Explicit check','--json').stdout)['task']
        transfer=root/'transfer.json';transfer.write_text(json.dumps({'agent':'fixture','task':task,'changed_files':[],'checks':['fixture'],'findings':[],'questions':['Review this?'],'artifacts':[]}))
        cli('workspace','handoff','transfer.json')
        assert json.loads(cli('workspace','handoff','show',str(task),'--json').stdout)['questions']==['Review this?']

        subprocess.run(['git','init','-q',str(root)],check=True)
        subprocess.run(['git','-C',str(root),'add','config/glass-core/config.toml'],check=True)
        subprocess.run(['git','-C',str(root),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture'],check=True)
        worktree=cli('worktree','new','named-task','--branch','task/acceptance').stdout
        assert 'task/acceptance' in worktree,worktree
        body=root/'body.md';body.write_text('Reviewed body')
        preview=json.loads(cli('pr','preview','--title','Fixture','--body-file',str(body),'--json').stdout)
        assert len(preview['preview_fingerprint'])==64 and not preview['published']
        cli('pr','create','--publish','--expect-hash','wrong','--title','Fixture','--body-file',str(body),ok=False)
        for size in [(80,30),(40,24),(20,8)]:
            for command,title in [('/advisor','Advisor'),('/handoff','Portable handoff'),('/hub','Workspace'),('/pr','Pull request'),('/quote','Quote')]:
                terminal=Terminal(root,env,size,args=('resume',session))
                try:
                    terminal.idle();terminal.send(command+'\r');terminal.wait(title)
                    terminal.send('\x1b');terminal.collect(.2)
                    assert terminal.p.poll() is None,(command,size,terminal.vt.text())
                finally:terminal.close()
        assert Endpoint.calls==calls,'Navigation/import/preview must not call the provider'
        # Opt-in cadence launches one read-only reviewer and deduplicates the patch.
        with (cfg/'config.toml').open('a') as output:output.write('# reviewed tracked change\n')
        cli('workspace','role','set','reviewer','mock','20000')
        cli('advisor',session,'cadence','1')
        terminal=Terminal(root,env,(40,24),args=('resume',session))
        try:
            terminal.idle();terminal.send('Milestone fixture\r')
            deadline=time.monotonic()+15
            while True:
                agents=json.loads(cli('agent','--session',session,'list','--json').stdout)
                if agents and agents[0]['status']=='idle':break
                assert time.monotonic()<deadline,(agents,terminal.vt.text())
                time.sleep(.05)
            assert len(agents)==1 and agents[0]['name'].startswith('review-'),agents
            cli('advisor',session,'request')
            assert len(json.loads(cli('agent','--session',session,'list','--json').stdout))==1
            assert Endpoint.calls==calls+2,'One foreground request and one advisor request expected'
        finally:terminal.close()

    finally:server.shutdown()
print('PASS: handoff preview/import/private create-only export, history ranges, advisor cadence, metadata hub, finding/handoff inspect, named worktree, PR fingerprint gate, advisor launch/dedup, 15 modal PTYs at 80x30/40x24/20x8; no navigation model calls')
