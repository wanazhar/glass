#!/usr/bin/env python3
"""Real global settings HTTP API in an empty isolated host; never contacts a provider."""
import concurrent.futures,hashlib,hmac,http.server,json,os,subprocess,tempfile,threading,time,urllib.request,urllib.error
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).parent))
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
class Provider(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):self.server.calls+=1;self.send_response(500);self.end_headers()
def main():
 with tempfile.TemporaryDirectory(prefix='gc-universal-settings-') as folder:
  root=Path(folder);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
  provider=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);provider.calls=0;threading.Thread(target=provider.serve_forever,daemon=True).start()
  endpoint=f'http://127.0.0.1:{provider.server_port}/v1'
  original=f'# retained header\nprovider="openai"\nmodel="fixture-model" # retained comment\napi_key_env=""\nbase_url="{endpoint}"\nsystem_prompt="""first\nsecond""" # instructions\n\n[custom]\nkeep="preserved"\n'
  (cfg/'config.toml').write_text(original)
  env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_ASSETS=str(Path(__file__).resolve().parents[1]/'web/dist'))
  process=subprocess.run([str(BIN),'connect','--local'],cwd=root,env=env,capture_output=True,text=True,timeout=20)
  assert process.returncode==0,process.stderr
  state=json.loads((root/'data/glass-core/connect.json').read_text());origin=f"http://127.0.0.1:{state['port']}";cap=hmac.new(state['token'].encode(),b'glass-connect-owner-access-v1\0'+origin.encode(),hashlib.sha256).hexdigest()
  def request(change=None,bad_origin=False,auth=True):
   headers={'Origin':'http://wrong.invalid' if bad_origin else origin,'Content-Type':'application/json'}
   if auth:headers.update({'X-Glass-Owner-Capability':cap,'Authorization':'Bearer '+state['token']})
   req=urllib.request.Request(origin+'/api/settings',headers=headers,data=None if change is None else json.dumps(change).encode())
   try:
    with urllib.request.urlopen(req,timeout=12) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  try:
   deadline=time.monotonic()+10
   while True:
    try:status,data=request();break
    except urllib.error.URLError:
     assert time.monotonic()<deadline,{'http':state['http'],'host':state.get('http_host')}
     time.sleep(.05)
   assert status==200 and data['scope']=='global' and data['defaults']['model']=='fixture-model',data
   assert request(auth=False)[0]==403
   assert request({'action':'preference','key':'model','value':'changed','revision':data['revision']},bad_origin=True)[0]==403
   def save(action,**values):
    nonlocal data
    status,result=request({'action':action,'revision':data['revision'],**values});assert status==200,result;data=result;return result
   old=data['revision'];save('preference',key='model',value='new-global-model')
   assert data['defaults']['model']=='new-global-model';text=(cfg/'config.toml').read_text();assert '# retained comment' in text and 'keep="preserved"' in text,text
   before=(cfg/'config.toml').read_bytes();assert request({'action':'preference','key':'max_tokens','value':0,'revision':data['revision']})[0]==400;assert (cfg/'config.toml').read_bytes()==before
   assert request({'action':'preference','key':'model','value':'stale','revision':old})[0]==400
   save('preference',key='system_prompt',value='new\nmultiline instructions');assert data['defaults']['system_prompt']=='new\nmultiline instructions'
   save('preference',key='cost_limit',value=.25);assert data['defaults']['cost_limit']==.25
   save('preference',key='cost_limit',value=None);assert data['defaults']['cost_limit'] is None
   save('preference',key='adaptive_pacing',value=False);assert data['defaults']['adaptive_pacing'] is False
   secret='ONLY_THE_FAKE_CREDENTIAL_STORE_MAY_CONTAIN_THIS'
   save('connection-add',provider='openai',endpoint=endpoint,model='account-model',label='Fixture account',secret=secret)
   assert secret not in json.dumps(data);assert secret in (cfg/'auth.json').read_text();account=data['active_connection'];assert account
   save('connection-model',id=account,model='account-model-2');assert data['connections'][0]['model']=='account-model-2'
   save('connection-use',id='');assert data['active_connection'] is None
   save('connection-use',id=account);assert data['active_connection']==account
   save('profile-create',name='A workspace preset');assert data['profiles'][0]['name']=='A workspace preset'
   save('profile',name='A workspace preset',model='profile-model',instructions='profile instructions');assert data['profiles'][0]['instructions']=='profile instructions'
   save('profile-assign',name='A workspace preset',workspace=str(root));assert data['profiles'][0]['workspaces']==[str(root)]
   save('profile-delete',name='A workspace preset',confirmed=True);assert not data['profiles']
   save('extension',id='todo',enabled=True);assert next(b for b in data['extensions'] if b['id']=='todo')['enabled'] is True
   save('extension',id='todo',enabled=False);assert next(b for b in data['extensions'] if b['id']=='todo')['enabled'] is False
   marker=root/'extension-must-not-run'
   custom=json.dumps(['python3','-c',f'from pathlib import Path; Path({str(marker)!r}).write_text("spawned")'])
   save('extension',id=custom,enabled=True);assert next(b for b in data['extensions'] if b['id']==custom)['enabled'] is True
   save('extension',id=custom,enabled=False);assert next(b for b in data['extensions'] if b['id']==custom)['enabled'] is False
   if data['available_mods']:
    save('preference',key='mods',value=[data['available_mods'][0]]);assert data['defaults']['mods']==[data['available_mods'][0]]
   revision=data['revision']
   with concurrent.futures.ThreadPoolExecutor(2) as pool:results=list(pool.map(lambda name:request({'action':'preference','key':'model','value':name,'revision':revision}),['concurrent-one','concurrent-two']))
   assert sorted(status for status,_ in results)==[200,400],results
   _,data=request();save('connection-remove',id=account,confirmed=True);assert not data['connections'] and secret not in (cfg/'auth.json').read_text()
   if os.environ.get('GC_SETTINGS_BROWSER')=='1':
    browser_env=dict(os.environ,GC_SETTINGS_TEST_ORIGIN=origin,GC_SETTINGS_TEST_TOKEN=state['token'])
    browser=subprocess.run(['node',str(Path(__file__).resolve().parents[1]/'web/test/settings-host.mjs')],cwd=Path(__file__).resolve().parents[1]/'web',env=browser_env,text=True,capture_output=True,timeout=60)
    assert browser.returncode==0,browser.stdout+browser.stderr
    print(browser.stdout.strip())
    _,data=request();assert data['defaults']['model']=='browser-default-model'
   assert not marker.exists(),'Changing an extension default executed its command'
   assert provider.calls==0,provider.calls
   assert not list((root/'data/glass-core').glob('*.jsonl')),'Settings created a conversation'
   print('PASS universal settings: reads/writes with zero conversations, no provider calls, defaults persist, comments/multiline TOML preserved, stale/concurrent edits rejected, connections/credentials and profiles managed globally, extension defaults without spawning agents; origin/auth gates enforced')
  finally:
   subprocess.run([str(BIN),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=15);provider.shutdown();provider.server_close()
if __name__=='__main__':main()
