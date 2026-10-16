#!/usr/bin/env python3
"""Isolated fake-Tailscale lifecycle and real native gateway authorization checks."""
import json,os,socket,subprocess,tempfile,urllib.request,urllib.error,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',ROOT/'target/debug/glass-core')).resolve()
with tempfile.TemporaryDirectory(prefix='gc-connect-lifecycle-')as tmp:
 root=Path(tmp);tools=root/'bin';tools.mkdir();cfg=root/'config/glass-core';cfg.mkdir(parents=True);data=root/'data/glass-core';data.mkdir(parents=True)
 routes=root/'routes.json';initial={'TCP':{'443':{'HTTPS':True}},'Web':{'fixture.tail.test:443':{'Handlers':{'/':{'Proxy':'http://127.0.0.1:8123'}}}}};routes.write_text(json.dumps(initial))
 script='''#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
p=Path(os.environ['GC_FAKE_ROUTES']);a=sys.argv[1:];r=json.loads(p.read_text())
if a==['status','--json']:print(json.dumps({'BackendState':'Running','Self':{'DNSName':'fixture.tail.test.','UserID':1},'User':{'1':{'LoginName':'owner@example.test'}}}))
elif a==['serve','status','--json']:print(json.dumps(r))
elif a[0]=='serve':
 port=next(x.split('=')[1]for x in a if x.startswith('--https='));key='fixture.tail.test:'+port
 if a[-1]=='off':r['TCP'].pop(port,None);r['Web'].pop(key,None)
 else:r['TCP'][port]={'HTTPS':True};r['Web'][key]={'Handlers':{'/':{'Proxy':a[-1]}}}
 p.write_text(json.dumps(r))
else:raise SystemExit('Unexpected tailscale command')
'''
 (tools/'tailscale').write_text(script);(tools/'tailscale').chmod(0o755)
 (cfg/'config.toml').write_text('provider="openai"\nmodel="mock"\nbase_url="http://127.0.0.1:1/v1"\napi_key_env=""\n')
 env=dict(os.environ,PATH=str(tools)+':'+os.environ['PATH'],GC_FAKE_ROUTES=str(routes),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_ASSETS=str(ROOT/'web/dist'))
 def command(*args):
  p=subprocess.run([str(BIN),*args],cwd=root,env=env,capture_output=True,text=True,timeout=15);assert p.returncode==0,(args,p.stdout,p.stderr);return p.stdout
 def state():return json.loads((data/'connect.json').read_text())
 def request(settings,login='owner@example.test',origin=None,host=None):
  headers={'Host':host or f"fixture.tail.test:{settings['https']}",'Origin':origin or f"https://fixture.tail.test:{settings['https']}",'Tailscale-User-Login':login}
  req=urllib.request.Request(f"http://127.0.0.1:{settings['port']}/api/status",headers=headers)
  try:
   with urllib.request.urlopen(req,timeout=5)as r:return r.status,json.load(r)
  except urllib.error.HTTPError as e:return e.code,json.load(e)
 occupied=socket.socket()
 try:occupied.bind(('0.0.0.0',8443));occupied.listen()
 except OSError:occupied.close();occupied=None
 try:
  command('--restricted','connect');s=state();first_port=s['https'];assert first_port in (8444,8445);assert request(s)[0]==200;assert request(s)[1]['security']['profile']=='workspace-edit';assert request(s,login='other@example.test')[0]==403;assert request(s,origin='https://evil.test')[0]==403;assert request(s,host='evil.test')[0]==403
  assert json.loads(routes.read_text())['Web']['fixture.tail.test:443']==initial['Web']['fixture.tail.test:443']
  command('connect');assert state()['pid']==s['pid']
  command('connect','stop');assert json.loads(routes.read_text())==initial
  command('connect');s=state();changed=json.loads(routes.read_text());changed['Web'][f"fixture.tail.test:{first_port}"]['Handlers']['/']['Proxy']='http://127.0.0.1:9999';routes.write_text(json.dumps(changed));command('connect','stop');assert json.loads(routes.read_text())==changed
  command('connect');s=state();second_port=s['https'];assert second_port!=first_port;os.kill(s['pid'],15);time.sleep(.15);command('connect');assert state()['https']==second_port
  command('connect','stop');assert json.loads(routes.read_text())==changed
  print('PASS: owner identity, host/origin rejection, explicit restriction inheritance, route preservation, existing gateway reuse, changed-route protection and stale-route recovery')
 finally:
  if occupied:occupied.close()
  if(data/'connect.json').exists():subprocess.run([str(BIN),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=10)
