#!/usr/bin/env python3
"""Isolated read-only suite API/CLI; never contacts a provider."""
import hashlib,hmac,json,os,subprocess,tempfile,time,urllib.request,urllib.error
from pathlib import Path
binary=Path(os.environ.get('GLASS_CORE_TEST_BINARY','target/core-release/glass-core')).resolve()
with tempfile.TemporaryDirectory(prefix='gc-suite-report-') as folder:
 root=Path(folder);cfg=root/'config/glass-core';cfg.mkdir(parents=True);journal=root/'.glass';journal.mkdir();secret='PRIVATE_DIAGNOSTIC_SENTINEL'
 (cfg/'config.toml').write_text(f'api_key_env=""\nsystem_prompt="{secret}"\nbase_url="http://127.0.0.1:9/{secret}"\n')
 (journal/'diagnostics.jsonl').write_text(json.dumps({'component':'glass-runtime-host','errorClass':'timeout','timestampUnixMs':42,'summary':secret,'toolName':secret,'backend':'native','sessionHash':'a'*64,'correlationHash':'b'*64})+'\n')
 before=(cfg/'config.toml').read_bytes();env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_ASSETS=str(Path('products/glass-core/web/dist').resolve()))
 try:
  subprocess.run([str(binary),'connect','--local'],cwd=root,env=env,check=True,capture_output=True,timeout=20)
  state=json.loads((root/'data/glass-core/connect.json').read_text());origin=f"http://127.0.0.1:{state['port']}";cap=hmac.new(state['token'].encode(),b'glass-connect-owner-access-v1\0'+origin.encode(),hashlib.sha256).hexdigest()
  def get(path='/api/diagnostics',auth=True):
   headers={'X-Glass-Owner-Capability':cap,'Authorization':'Bearer '+state['token']} if auth else {}
   try:
    with urllib.request.urlopen(urllib.request.Request(origin+path,headers=headers),timeout=12) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  deadline=time.monotonic()+10
  while True:
   try:status,report=get();break
   except urllib.error.URLError:
    assert time.monotonic()<deadline;time.sleep(.05)
  assert status==200 and report['schemaVersion']==1,report
  assert secret not in json.dumps(report)
  assert report['glass']['journal']['events'][0]['errorClass']=='timeout'
  assert report['session']['status']=='not_selected'
  assert get(auth=False)[0]==403
  assert get('/api/diagnostics?id=../outside')[0]==400
  (journal/'diagnostics.jsonl').unlink();(journal/'diagnostics.jsonl').symlink_to(cfg/'config.toml')
  assert get()[1]['glass']['journal']['status']=='partial'
  cli=subprocess.run([str(binary),'diagnostics','--suite','--json'],cwd=root,env=env,check=True,capture_output=True,text=True,timeout=12)
  assert json.loads(cli.stdout)['collection']['providerProbes'] is False
  assert secret not in cli.stdout and (cfg/'config.toml').read_bytes()==before
  assert not list((root/'data').rglob('*.jsonl'))
  print('PASS suite API/CLI, authentication, redaction, symlink rejection, unchanged config and zero conversations')
 finally:subprocess.run([str(binary),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=15)
