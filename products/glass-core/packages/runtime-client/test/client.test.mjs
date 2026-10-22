import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {RuntimeClient,RuntimeError} from '../dist/index.js';
test('actual runtime: typed session controls, errors, cursor and resume',async()=>{
  const binary=process.env.GLASS_CORE_TEST_BINARY;
  assert.ok(binary,'GLASS_CORE_TEST_BINARY must identify the release');
  const root=mkdtempSync(join(tmpdir(),'gc-ts-runtime-'));
  mkdirSync(join(root,'config','glass-core'),{recursive:true});
  writeFileSync(join(root,'config','glass-core','config.toml'),'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\n');
  const client=new RuntimeClient(binary,{cwd:root,env:{...process.env,XDG_CONFIG_HOME:join(root,'config'),XDG_DATA_HOME:join(root,'data')}});
  try{
    const init=await client.initialize(join(root,'sessions'));
    assert.equal(init.protocolVersion,1);
    const {sessionId}=await client.newSession({cwd:root,toolMode:'host',tools:[]});
    const page=await client.entries(sessionId);
    assert.deepEqual(page.entries,[]);
    await client.request('session/name',{sessionId,name:'TypeScript SDK'});
    await client.request('session/close',{sessionId});
    assert.equal((await client.resumeSession(sessionId)).sessionId,sessionId);
    assert.equal((await client.request('session/describe',{sessionId})).name,'TypeScript SDK');
    await assert.rejects(client.request('unsupported',{}),e=>e instanceof RuntimeError && e.code===-32601 && e.data.kind==='unsupported_method');
    await client.shutdown();
  }finally{
    const closed=new Promise(resolve=>client.child.once('close',resolve));
    client.close();
    if(client.child.exitCode===null)await closed;
    rmSync(root,{recursive:true,force:true});
  }
});
