import {chromium} from 'playwright-core';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {resolve,extname} from 'node:path';
const root=resolve('dist'),commands=JSON.parse(await readFile('test/fixtures/commands.json','utf8')).commands,streams=new Set(),sent=[];
if(!commands.some(c=>c.name==="desktop"))commands.push({name:"desktop",description:"Open shared desktop",group:"work",portable:false,primary:false,aliases:[]});
let fail=false,hold=false,release,deny=false;
const result=name=>({format:'json',body:name==='todo'?{tasks:[]}:{setting:name,current:'fixture value'}});
const server=createServer(async(req,res)=>{
 const u=new URL(req.url,'http://localhost'),path=u.pathname;
 if(path==='/api/events'){res.writeHead(200,{'Content-Type':'text/event-stream'});streams.add(res);res.write(`data: ${JSON.stringify({reset:true,control:{can_write:!deny,web_workflows:1,commands},active:false,live:[]})}\n\n`);req.on('close',()=>streams.delete(res));return;}
 if(path==='/api/agent-events'){res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('data: {"revision":0}\n\n');req.on('close',()=>{});return;}
 if(path.startsWith('/api/')){
  let data={};
  if(path==='/api/status')data={workspace:'/tmp/project',profile:'workspace-edit',web_workflows:1};
  if(path==='/api/threads')data={threads:[{session:'fixture',title:'Settings fixture',workspace:'/tmp/project',workspace_assigned:true,workspace_available:true}],projects:[]};
  if(path==='/api/history')data={records:Array.from({length:35},(_,i)=>({index:i,record:{k:'assistant',t:`Message ${i}\n\nRetained transcript content for scrolling.`}})),before:0};
  if(path==='/api/commands')data={commands};
  if(path==='/api/agents')data=[];
  if(path==='/api/desktop/status')data={available:false};
  if(path==='/api/command'||path==='/api/send'){
   let raw='';for await(const chunk of req)raw+=chunk;const body=JSON.parse(raw);
   if(path==='/api/command')data=result(body.name);
   else if(body.message.t==='command'){
    const m=body.message;sent.push(m);
    const deliver=()=>{for(const stream of streams)stream.write(`data: ${JSON.stringify({events:[{t:'command_result',request_id:m.request_id,...(fail?{error:'Fixture save rejected'}:{result:result(m.name)})}]})}\n\n`);};
    if(hold)release=deliver;else deliver();
   }
  }
  res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(data));return;
 }
 try{const file=resolve(root,'.'+(path==='/'?'/index.html':path));if(!file.startsWith(root+'/'))throw Error();res.writeHead(200,{'Content-Type':({'.js':'text/javascript','.css':'text/css','.html':'text/html','.woff2':'font/woff2'})[extname(file)]||'application/octet-stream'});res.end(await readFile(file));}catch{res.writeHead(404).end();}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const origin=`http://127.0.0.1:${server.address().port}`,browser=await chromium.launch({executablePath:process.env.GC_BROWSER_BINARY||(existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome')?'/snap/chromium/current/usr/lib/chromium-browser/chrome':undefined),headless:true,args:['--no-sandbox','--disable-dev-shm-usage']});
const routes={login:'connections',logout:'connections',model:'models',budget:'models',pacing:'models',mods:'integrations',extensions:'integrations',profiles:'profiles',neuralnets:'memory',skills:'memory','skill-doctor':'memory',security:'permissions',advisor:'permissions','settings-info':'effective'};
try{for(const width of [390,768,1440].filter(w=>!process.env.GC_SETTINGS_WIDTH||w===Number(process.env.GC_SETTINGS_WIDTH)))for(const colorScheme of ['light','dark']){
 const context=await browser.newContext({viewport:{width,height:900},colorScheme,hasTouch:width<1000,serviceWorkers:'block'}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(origin+'/?thread=fixture');await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');
 await page.locator('#prompt').fill('Keep my draft');await page.locator('#conversation').evaluate(n=>{n.scrollTop=120;});const scroll=await page.locator('#conversation').evaluate(n=>n.scrollTop);
 await page.locator('#prompt').fill('/settings');await page.locator('#composer').evaluate(n=>n.requestSubmit());await page.locator('#settings-page h1').filter({hasText:/^Settings$/}).waitFor();
 assert.equal(await page.locator('dialog[open]').count(),0);assert.equal(await page.locator('.app').isVisible(),false);assert.equal(await page.locator('.settings-card').count(),8);
 assert(await page.evaluate(()=>{const root=document.documentElement,p=document.querySelector('#settings-page'),height=root.style.getPropertyValue('--app-height'),top=root.style.getPropertyValue('--viewport-top');root.style.setProperty('--app-height','520px');root.style.setProperty('--viewport-top','12px');const r=p.getBoundingClientRect();root.style.setProperty('--app-height',height);root.style.setProperty('--viewport-top',top);return r.height===520&&r.top===12;}),'Settings follows the shared keyboard viewport');
 await page.screenshot({path:`/tmp/glass-settings-${width}-${colorScheme}.png`});
 assert(await page.locator('#settings-page').evaluate(n=>{const r=n.getBoundingClientRect();return r.x===0&&r.y===0&&Math.abs(r.width-innerWidth)<1&&Math.abs(r.height-innerHeight)<1;}));
 await page.locator('a.settings-card').filter({hasText:'Appearance'}).click();await page.getByLabel('Theme',{exact:true}).selectOption('dark');await page.locator('#settings-page').getByLabel('Working animation').selectOption('none');await page.getByLabel('Tool cards').selectOption('compact');await page.getByText('Preference saved',{exact:true}).waitFor();
 await page.reload();await page.getByLabel('Theme',{exact:true}).waitFor();assert.equal(await page.locator('#settings-page').getByLabel('Working animation').inputValue(),'none');assert(await page.locator('html').evaluate(n=>n.classList.contains('compact-tools')));
 await page.goBack();await page.locator('#settings-page h1').filter({hasText:/^Settings$/}).waitFor();await page.goForward();await page.getByLabel('Theme',{exact:true}).waitFor();
 for(const [name,section] of Object.entries(routes)){
  const count=sent.length;await page.goto(`${origin}/?thread=fixture&settings=${section}&setting=${name}`);
  await page.locator('#settings-page #command-form').waitFor();
  if(name!=='logout'){await page.locator('#settings-page #command-result[aria-busy=false]').waitFor();await page.waitForFunction(()=>document.querySelector('#command-result').textContent.length>0);}else assert.equal(sent.length,count);
  assert.equal(await page.locator('dialog[open]').count(),0,name);assert.equal(await page.locator('#settings-page #command-form').count(),1,name);
  for(const m of sent.slice(count))assert.deepEqual(m.values,{},`Opening ${name} must only inspect`);
 }
 await page.goto(origin+'/?thread=fixture&settings=models');await page.getByRole('button',{name:'Response pacing ›',exact:true}).click();await page.waitForFunction(()=>document.querySelector('#command-result').textContent.length>0);
 await page.getByLabel('Response pacing',{exact:true}).selectOption('maximum');await page.locator('#settings-page header button').click();await page.getByText('You have unsaved changes.',{exact:true}).waitFor();await page.getByRole('button',{name:'Continue editing',exact:true}).click();
 await page.goBack();await page.getByText('You have unsaved changes.',{exact:true}).waitFor();await page.getByRole('button',{name:'Continue editing',exact:true}).click();assert.match(page.url(),/setting=pacing/);
 hold=true;await page.locator('#command-form .command-run').click();await page.waitForFunction(()=>document.querySelector('#command-status').dataset.working==='true');assert.equal(await page.locator('#command-form .command-run').isDisabled(),true);await page.locator('#settings-page header button').click();await page.getByText('Wait for this settings operation to finish.',{exact:true}).waitFor();
 assert.equal(sent.at(-1).name,'pacing');assert.equal(sent.at(-1).values.args,'maximum');release();hold=false;await page.getByText('Completed',{exact:true}).waitFor();
 await page.locator('#settings-page header button').click();assert.equal(await page.locator('.settings-unsaved').count(),0);
 await page.goBack();await page.waitForFunction(()=>document.querySelector('#command-result').textContent.length>0);await page.getByLabel('Response pacing',{exact:true}).selectOption('adaptive');await page.goForward();await page.getByText('You have unsaved changes.',{exact:true}).waitFor();await page.getByRole('button',{name:'Continue editing',exact:true}).click();assert.match(page.url(),/setting=pacing/);
 await page.goForward();await page.getByText('You have unsaved changes.',{exact:true}).waitFor();await page.getByRole('button',{name:'Discard changes',exact:true}).click();await page.locator('#settings-page h1').filter({hasText:/^Models & execution$/}).waitFor();
 await page.goto(origin+'/?thread=fixture&settings=models&setting=pacing');await page.waitForFunction(()=>document.querySelector('#command-result').textContent.length>0);fail=true;await page.getByLabel('Response pacing',{exact:true}).selectOption('adaptive');await page.locator('#command-form .command-run').click();await page.getByText('Fixture save rejected',{exact:true}).waitFor();assert.equal(await page.locator('#command-form .command-run').isDisabled(),false);fail=false;
 await page.locator('#settings-page header button').click();await page.getByRole('button',{name:'Discard changes',exact:true}).click();
 await page.goto(origin+'/?thread=fixture&settings=connections&setting=login');await page.waitForFunction(()=>document.querySelector('#command-result').textContent.length>0);await page.locator('#command-form select[name=action]').selectOption('api');await page.locator('#command-form input[name=secret]').fill('temporary-secret');await page.locator('#settings-page header button').click();await page.getByRole('button',{name:'Discard changes',exact:true}).click();assert.equal(await page.locator('input[type=password]').evaluateAll(ns=>ns.every(n=>n.value==='')),true);
 await page.goto(origin+'/?settings=index');await page.locator('.settings-card').first().waitFor();await page.getByRole('link',{name:/Connections/}).click();await page.getByRole('button',{name:'Manage connections ›'}).click();await page.getByLabel('Settings conversation').waitFor();await page.getByLabel('Settings conversation').selectOption('fixture');await page.locator('#settings-page #command-form').waitFor();
 await page.goto(origin+'/?thread=fixture');await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');await page.locator('#prompt').fill('Keep my draft');await page.locator('#conversation').evaluate(n=>n.scrollTop=120);
 await page.locator('#open-settings').evaluate(n=>n.click());await page.locator('.settings-card').first().waitFor();await page.locator('#settings-page header button').click();assert.equal(await page.locator('#prompt').inputValue(),'Keep my draft');assert.equal(await page.locator('#conversation').evaluate(n=>n.scrollTop),scroll);
 await page.locator('#prompt').fill('/desktop');await page.locator('#composer').evaluate(n=>n.requestSubmit());await page.locator('#desktop-title').waitFor();await page.keyboard.press('Escape');
 assert.deepEqual(errors,[]);await page.screenshot({path:`/tmp/glass-settings-chat-${width}-${colorScheme}.png`});await context.close();console.log(`PASS Settings ${width}px ${colorScheme}: full-page routes, all native settings inspect, save/receipt/error/secret handling, appearance persistence, back/forward, draft/scroll, Computer shortcut`);
}}finally{await browser.close();server.closeAllConnections();await new Promise(r=>server.close(r));}
