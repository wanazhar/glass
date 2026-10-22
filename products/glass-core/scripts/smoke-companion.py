#!/usr/bin/env python3
"""Actual release companion authentication, observer reconnect and frame fixture."""
import json, os, select, signal, socket, subprocess, tempfile, threading, time, urllib.error, urllib.request
from pathlib import Path
from core_binary import core_binary_path
BINARY=os.environ.get('GLASS_CORE_TEST_BINARY',str(core_binary_path('release')))
with tempfile.TemporaryDirectory(prefix='gc-companion-') as tmp:
    root=Path(tmp);path=root/'fixture.sock';listener=socket.socket(socket.AF_UNIX);listener.bind(str(path));listener.listen();listener.settimeout(.2)
    stopped=threading.Event();messages=[];clients=[]
    def handle(connection):
        try:
            with connection:
                source=connection.makefile('rb')
                for line in source:
                    message=json.loads(line);messages.append(message)
                    if message['t']=='attach':
                        connection.sendall((json.dumps({'t':'replay','entries':[{'role':'user','text':'fixture'}],'can_write':False})+'\n').encode())
                    elif message['t']=='control':
                        connection.sendall(b'{"t":"control","can_write":true}\n')
        except (OSError, ValueError):pass
    def accept():
        while not stopped.is_set():
            try:connection,_=listener.accept()
            except socket.timeout:continue
            except OSError:return
            clients.append(connection);threading.Thread(target=handle,args=(connection,),daemon=True).start()
    threading.Thread(target=accept,daemon=True).start()
    process=subprocess.Popen([BINARY,'companion',str(path),'0'],cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True)
    try:
        assert select.select([process.stdout],[],[],10)[0], 'Companion did not start'
        line=process.stdout.readline();link=line[line.index('http://'):].strip();base,token=link.split('#')
        def request(endpoint,body=None,authorized=True):
            headers={'Authorization':'Bearer '+token} if authorized else {}
            data=None if body is None else json.dumps(body).encode()
            return urllib.request.urlopen(urllib.request.Request(base+endpoint,data=data,headers=headers),timeout=5)
        with request('') as response:html=response.read().decode()
        assert 'streaming.append' in html and 'Display preview capped' in html and 'Reconnect' in html
        script=root/'ui.js';script.write_text(html.split('<script>',1)[1].split('</script>',1)[0])
        subprocess.run(['node','--check',str(script)],check=True,capture_output=True)
        try:request('api/events?since=0',authorized=False)
        except urllib.error.HTTPError as error:assert error.code==403
        else:raise AssertionError('Unauthenticated API accepted')
        with request('api/events?since=0') as response:events=json.load(response)
        assert events['events'][0]['message']['can_write'] is False
        with request('api/send',{'t':'control','action':'claim'}) as response:assert response.status==202
        with request('api/reconnect',{}) as response:assert json.load(response)['status']=='reconnected_as_observer'
        deadline=time.monotonic()+5
        while True:
            with request('api/events?since=0') as response:events=json.load(response)
            if events['events'] and events['events'][-1]['cursor']>1 and events['events'][-1]['message'].get('can_write') is False:break
            assert time.monotonic()<deadline,events
            time.sleep(.02)
        assert sum(m['t']=='attach' and m.get('observe') is True for m in messages)==2
        try:request('api/send',{'t':'stop'})
        except urllib.error.HTTPError as error:assert error.code==400
        else:raise AssertionError('Unsupported effect accepted')
    finally:
        os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=5)
        stopped.set();listener.close()
        for client in clients:
            try:client.shutdown(socket.SHUT_RDWR)
            except OSError:pass
print('PASS: actual release companion private API, observer replay, explicit claim/reconnect, unsupported-effect refusal and JavaScript syntax/coalescing/cap disclosure')
