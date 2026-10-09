#!/usr/bin/env python3
"""Offline native helper throughput and default-write subprocess regression check."""
import hashlib,json,os,statistics,subprocess,tempfile,time
from pathlib import Path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',Path(__file__).resolve().parents[1]/'target/release/glass-core')).resolve()
report={}
with tempfile.TemporaryDirectory(prefix='gc-native-perf-') as tmp:
    root=Path(tmp);env=dict(os.environ,GC_FILE_JSON='1',XDG_CONFIG_HOME=str(root/'config'))
    for size in [100_000,1_000_000,8_000_000]:
        source=('unchanged line with content\n'*(size//28)).encode();(root/'sample.txt').write_bytes(source);samples=[]
        for _ in range(7):
            start=time.perf_counter();p=subprocess.run([str(BIN),'file','read','sample.txt','1','20'],cwd=root,env=env,capture_output=True,text=True);samples.append((time.perf_counter()-start)*1000);assert p.returncode==0,p.stderr
            v=json.loads(p.stdout);assert v['revision']==hashlib.sha256(source).hexdigest() and len(v['text'].encode())<24576
        report[f'read_{len(source)}_bytes_median_ms']=statistics.median(samples)
        assert statistics.median(samples)<1000
    source='head\n'+'unchanged line\n'*70_000+'tail\n';(root/'sample.txt').write_text(source);revision=hashlib.sha256(source.encode()).hexdigest();samples=[]
    for n in range(10):
        payload=json.dumps({'path':'sample.txt','expected':revision,'edits':[{'start':1,'end':1,'text':f'head{n}'}]})
        start=time.perf_counter();p=subprocess.run([str(BIN),'file','edit'],cwd=root,env=env,input=payload,capture_output=True,text=True);samples.append((time.perf_counter()-start)*1000);assert p.returncode==0,p.stderr
        v=json.loads(p.stdout);revision=v['revision'];assert len(v['preview'])<4096
    report['edit_1mb_median_ms']=statistics.median(samples);assert statistics.median(samples)<1000
    # A normal Python write must not eagerly launch a Python checker.
    fake=root/'bin';fake.mkdir();marker=root/'parser-was-launched';(fake/'python3').write_text(f'#!/bin/sh\ntouch "{marker}"\nexit 1\n');(fake/'python3').chmod(0o755)
    p=subprocess.run([str(BIN),'file','write'],cwd=root,env=dict(env,PATH=str(fake)),input=json.dumps({'path':'plain.py','expected':None,'text':'x = 1\n'}),capture_output=True,text=True);assert p.returncode==0,p.stderr;assert not marker.exists()
    report['ordinary_python_write_spawns_checker']=False
print(json.dumps(report,indent=2))
print('PASS: bounded native file throughput and no unsolicited checker process')
