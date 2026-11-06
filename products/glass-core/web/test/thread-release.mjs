import {chromium} from 'playwright-core';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,extname} from 'node:path';
const root=resolve('dist'),streams=new Set(),opens=[],closes=[];
let releaseClose,failClose=false;
const server=createServer(async(req,res)=>{
 const url=new URL(req.url,'http://localhost'),path=url.pathname;
 if(path==='/api/events'){
  res.writeHead(200,{'Content-Type':'text/event-stream'});streams.add(res);
  res.write(`data: ${JSON.stringify({reset:true,control:{can_write:true,web_workflows:1,commands:[]},live:[],active:false})}\n\n`);
  req.on('close',()=>streams.delete(res));return;
 }
 if(path.startsWith('/api/')){
  let value={};
  if(path==='/api/status')value={workspace:'/tmp',profile:'workspace-edit'};
  if(path==='/api/threads')value={threads:['one','two','three'].map(session=>({session,title:'Thread '+session,workspace:'/tmp',workspace_assigned:true,workspace_available:true})),projects:[]};
  if(path==='/api/history')value={records:[{index:0,record:{k:'assistant',t:'Messages from '+url.searchParams.get('id')}}],before:0};
  if(path==='/api/agents')value={agents:[]};
  if(path==='/api/open'||path==='/api/close'){
   let raw='';for await(const part of req)raw+=part;const body=JSON.parse(raw);
   if(path==='/api/open'){opens.push(body.id);value={connected:true};}
   else{closes.push(body.id);if(failClose&&body.id==='one'){res.writeHead(409,{'Content-Type':'application/json'});res.end(JSON.stringify({error:'Previous conversation is working',code:'session_active_elsewhere'}));return;}if(body.id==='one')await new Promise(r=>releaseClose=r);value={closed:true};}
  }
  res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(value));return;
 }
 try{const file=resolve(root,'.'+(path==='/'?'/index.html':path));if(!file.startsWith(root+'/'))throw Error();const body=await readFile(file);res.setHeader('Content-Type',({'.js':'text/javascript','.css':'text/css','.html':'text/html','.svg':'image/svg+xml','.woff2':'font/woff2'})[extname(file)]||'application/octet-stream');res.end(body);}catch{res.writeHead(404).end();}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({executablePath:'/snap/chromium/current/usr/lib/chromium-browser/chrome',args:['--no-sandbox']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`http://127.0.0.1:${server.address().port}/?thread=one`);
 await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');
 await page.getByRole('button',{name:'Thread two',exact:false}).click();
 await page.getByText('Messages from two',{exact:true}).waitFor({timeout:3000});
 assert.deepEqual(closes,['one']);assert(releaseClose,'Previous release is still deliberately blocked');
 await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');
 await page.getByRole('button',{name:'Thread one',exact:false}).click();
 await page.waitForTimeout(150);assert.equal(opens.filter(id=>id==='one').length,1,'Do not reopen an actor still being released');
 const release=releaseClose;releaseClose=null;release();
 await page.getByText('Messages from one',{exact:true}).waitFor();
 assert.equal(opens.filter(id=>id==='one').length,2);assert(closes.includes('two'));
 await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');
 failClose=true;await page.getByRole('button',{name:'Thread three',exact:false}).click();
 await page.getByText('Messages from three',{exact:true}).waitFor();
 await page.getByText('The previous conversation is still working in the background; the new conversation is open.',{exact:true}).waitFor();
 assert.equal(await page.locator('#thread-name').textContent(),'Thread three');
 assert.deepEqual(errors,[]);
 console.log('PASS new thread renders during delayed previous release; reopening a releasing actor waits safely');
}finally{releaseClose?.();await browser.close();for(const res of streams)res.end();server.closeAllConnections();await new Promise(r=>server.close(r));}
