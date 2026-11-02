#!/usr/bin/env python3
"""Real TUI desktop lifecycle in an isolated manager/profile; no provider calls."""
import json,os,socket,subprocess,tempfile,time,urllib.request
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
with tempfile.TemporaryDirectory(prefix='gc-desktop-tui-') as tmp:
    root=Path(tmp);config=root/'config/glass-core';config.mkdir(parents=True)
    (config/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_ASSETS=str(Path(__file__).resolve().parents[1]/'web/dist'))
    state_path=root/'data/glass-core/connect.json';t=Terminal(root,env,(100,30))
    def command(text,expected,timeout=90):
        t.send(text+'\r');t.wait(expected,timeout);t.send('\x1b');t.collect(.15);t.send('\x1b');t.collect(.15)
    def status():
        state=json.loads(state_path.read_text());url=f"http://127.0.0.1:{state['port']}/api/desktop/status"
        with urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+state['token']}),timeout=10) as r:return json.load(r)
    try:
        t.idle();t.send('/desktop\r');t.wait('Reusable workspace',10);t.collect(.2);assert 'Quick desktop' in t.vt.text(),t.vt.text();t.send('\x1b');t.collect(.15);t.send('\x1b');t.collect(.15)
        assert not state_path.exists(),'opening picker must not start a manager'
        command('/desktop status','No desktop is running');assert not state_path.exists()
        command('/desktop stop','No desktop is running');assert not state_path.exists()
        command('/desktop quick','Environment: XFCE');first=status();assert first['lifecycle']=='disposable'
        command('/desktop reusable','Another desktop is running');assert status()['sessionId']==first['sessionId']
        # Stale stop requests cannot terminate the current desktop.
        client=socket.socket(socket.AF_UNIX);client.settimeout(10);client.connect(str(root/'data/glass-core/desktop-agent.sock'))
        client.sendall(b'{"action":"stop","sessionId":"stale-session"}\n');reply=json.loads(client.makefile().read());client.close();assert 'error' in reply
        assert status()['sessionId']==first['sessionId']
        command('/desktop attach','Environment: XFCE');assert status()['sessionId']==first['sessionId']
        command('/desktop status','Lifecycle: disposable')
        command('/desktop stop','Desktop stopped');assert status()['sessionId'] is None
        print('PASS TUI: picker, inactive status/stop, quick XFCE, attach, status, explicit switching and stale-stop protection')
        command('/desktop reusable','Environment: GNOME');gnome=status();assert gnome['lifecycle']=='reusable'
        profile=root/'data/glass-core/desktop/profiles/default/home';marker=profile/'tui-persistence-fixture';marker.write_text('retained')
        command('/desktop stop','Desktop stopped');assert marker.read_text()=='retained' and status()['sessionId'] is None
        command('/desktop reusable','Environment: GNOME');assert status()['sessionId']!=gnome['sessionId'] and marker.read_text()=='retained'
        command('/desktop stop','Desktop stopped');assert marker.read_text()=='retained'
        Path('/tmp/gc-desktop-tui.ansi').write_text(t.seen)
        print('PASS TUI: reusable GNOME, stop/reopen, saved profile preservation; recording /tmp/gc-desktop-tui.ansi')
    finally:
        if state_path.exists():
            state=json.loads(state_path.read_text())
            try:
                value=status()
                if value.get('sessionId'):
                    request=urllib.request.Request(f"http://127.0.0.1:{state['port']}/api/desktop/session",data=b'{"action":"stop"}',headers={'Authorization':'Bearer '+state['token'],'Origin':f"http://127.0.0.1:{state['port']}",'Content-Type':'application/json'})
                    urllib.request.urlopen(request,timeout=30).close()
            finally:subprocess.run([str(BIN),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=15)
        t.close()
