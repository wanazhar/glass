#!/usr/bin/env python3
"""Isolated release CLI lifecycle and actual PTY rendering checks."""
import json, os, pathlib, pty, select, subprocess, tempfile, fcntl, struct, termios
binary = str(pathlib.Path(os.environ.get('GLASS_CORE_TEST_BINARY', 'target/release/glass-core')).resolve())
with tempfile.TemporaryDirectory(prefix='gc-neuralnets-') as tmp:
    env = dict(os.environ, XDG_DATA_HOME=tmp+'/data', XDG_CONFIG_HOME=tmp+'/config')
    def run(*args, ok=True):
        p=subprocess.run([binary,*args],cwd=tmp,env=env,text=True,capture_output=True)
        assert (p.returncode==0)==ok, (args,p.stdout,p.stderr)
        return json.loads(p.stdout) if ok else p.stderr
    p=run('neuralnets','propose','preference','Use bounded reads','--json')
    entry=p['entries'][0]; ident=entry['id']; revision=p['revision']
    assert entry['status']=='pending' and entry['source']=='human CLI proposal'
    p=run('neuralnets','approve',ident,str(revision),'--json')
    assert p['entries'][0]['status']=='confirmed'
    run('neuralnets','forget',ident,str(revision),'--json',ok=False)
    p=run('neuralnets','forget',ident,str(p['revision']),'--json')
    run('neuralnets','propose','preference','Use bounded reads','--json',ok=False)
    p=run('neuralnets','restore',ident,str(p['revision']),'--json')
    assert p['entries'][0]['status']=='pending' and len(p['entries'])==1
    assert run('neuralnets','list','32','--json')['entries']==[]
    for topic in ['files','background','agents','context','learning','workspace','extensions','recovery','ui']:
        assert run('guidance','read',topic,'--json')['topic']==topic
    assert run('guidance','search','neuralnets','--json')['commands']
    for cols,rows in [(20,8),(40,24),(80,30)]:
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',rows,cols,0,0))
        proc=subprocess.Popen([binary,'guidance','read','learning'],cwd=tmp,env=env,stdin=slave,stdout=slave,stderr=slave)
        os.close(slave); output=b''
        while True:
            if select.select([master],[],[],5)[0]:
                try: chunk=os.read(master,65536)
                except OSError: break
                if not chunk: break
                output+=chunk
            elif proc.poll() is not None: break
            else: proc.kill(); raise AssertionError('PTY timeout')
        assert proc.wait(timeout=5)==0 and b'learning' in output.lower(),output
        os.close(master)
    store=next(pathlib.Path(tmp+'/data/glass-core/neuralnets').glob('*.json'))
    assert store.stat().st_mode & 0o077 == 0
    store.write_text('{corrupt')
    run('neuralnets','propose','fact','Never replace corrupt data','--json',ok=False)
    assert store.read_text()=='{corrupt'
print('neuralnets: lifecycle, stale review, restore, pagination, 9 recipes, 3 PTY sizes, privacy, corrupt-store preservation passed')
