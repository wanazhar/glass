#!/usr/bin/env python3
"""Deterministic release workflows, ownership/recovery checks and small-screen PTYs."""
import argparse, concurrent.futures, hashlib, json, os, subprocess, sys
from pathlib import Path
from core_binary import core_binary_path
parser=argparse.ArgumentParser(description='Run roadmap acceptance against one unchanged optimized binary. Requires pyte.')
parser.add_argument('--binary',type=Path,default=core_binary_path('release'))
parser.add_argument('--report',type=Path,default=Path('roadmap-release-report.json'))
options=parser.parse_args()
root=Path(__file__).resolve().parents[1]
binary=options.binary.resolve()
log_dir=options.report.resolve().with_suffix('.logs')
log_dir.mkdir(parents=True,exist_ok=True)
env=dict(os.environ, GLASS_CORE_TEST_BINARY=str(binary))
env.pop('NO_COLOR',None)
env.pop('GC_DEFER_TOOLS',None)
python=sys.executable
tests=['smoke-recap.py','smoke-stream-search.py','smoke-reader.py','smoke-navigation.py','smoke-retry-fresh.py','smoke-pipes-search.py','smoke-attention-jobs.py','smoke-direct-files.py','smoke-neuralnets.py','smoke-neuralnets-wire.py','smoke-transaction-recovery.py','smoke-tooling-foundation.py','smoke-native-files-jobs.py','smoke-turn-recovery.py','smoke-egress.py','smoke-state-receipts.py','smoke-checkpoint-rewind.py','smoke-wait.py','smoke-hooks-shell.py','smoke-recovery.py','smoke-cli.py','smoke-roadmap-completion.py','smoke-companion.py','smoke-presentation.py','smoke-blocks.py','smoke-scroll-submit.py','smoke-context.py','smoke-inbox.py','smoke-loop.py','smoke-worktree.py','smoke-workflow-next.py','smoke-plugin-integrity.py','smoke-memory.py','smoke-agent-limits.py','smoke-lsp.py','smoke-serve.py','smoke-adapter-bounds.py','smoke-tool-repair.py','smoke-queue.py','smoke-workflows.py','smoke-job-unicode.py','smoke-security.py','smoke-permissions-pty.py']
initial=hashlib.sha256(binary.read_bytes()).hexdigest()
def run(name, extra=None):
    log=log_dir/(name.removesuffix('.py')+('-'+extra['GC_PTY_SIZE'] if extra else '')+'.log')
    with log.open('w') as output:
        try:
            p=subprocess.run([python,str(root/'scripts'/name)],cwd=root,env=dict(env,**(extra or {})),stdout=output,stderr=subprocess.STDOUT,timeout=180)
            result={'test':name,'size':(extra or {}).get('GC_PTY_SIZE'),'code':p.returncode,'log':str(log)}
        except subprocess.TimeoutExpired:
            result={'test':name,'code':'timeout','log':str(log)}
    print(json.dumps(result),flush=True)
    return result
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    results=list(pool.map(run,tests))
for size in ['80x30','40x24','20x8']:
    results.append(run('smoke-ext-steer-pty.py',{'GC_PTY_SIZE':size}))
assert hashlib.sha256(binary.read_bytes()).hexdigest()==initial,'binary changed during verification'
options.report.write_text(json.dumps({'binary_sha256':initial,'results':results},indent=2))
sys.exit(any(r['code'] != 0 for r in results))
