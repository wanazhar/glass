import {chooseOption} from './select-helper.mjs';
import {chromium,webkit} from 'playwright-core';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {resolve,extname} from 'node:path';
const root=resolve('dist'),initial=JSON.parse(await readFile('test/fixtures/settings.json','utf8')),commands=JSON.parse(await readFile('test/fixtures/commands.json','utf8')).commands;
if(!commands.some(c=>c.name==='desktop'))commands.push({name:'desktop',description:'Shared desktop',portable:false,group:'work',primary:false,aliases:[]});
let state,requests=[],posts=[],streams=new Set(),hold=false,fail=false,deliver;
const copy=v=>JSON.parse(JSON.stringify(v));
async function section(page,title){const trigger=page.getByRole('button',{name:'Settings sections',exact:true});if(await trigger.isVisible())await trigger.click();await page.getByRole('link',{name:title,exact:true}).click();}
function apply(change){
 assert.equal(change.revision,state.revision);assert(!('sessionId' in change)&&!('thread' in change));
 if(change.action==='preference')state.defaults[change.key]=change.value;
 if(change.action==='connection-model')state.connections.find(c=>c.id===change.id).model=change.model;
 if(change.action==='connection-use')state.active_connection=change.id||null;
 if(change.action==='connection-add'){assert(change.secret);state.connections.push({id:'new-account',name:change.label,provider:change.provider,endpoint:change.endpoint,model:change.model,has_credential:true});state.active_connection='new-account';}
 if(change.action==='connection-remove'){state.connections=state.connections.filter(c=>c.id!==change.id);if(state.active_connection===change.id)state.active_connection=null;}
 if(change.action==='extension'){const existing=state.extensions.find(b=>b.id===change.id);if(existing)existing.enabled=change.enabled;else state.extensions.push({id:change.id,name:'Custom extension',description:change.id,enabled:change.enabled});}
 if(change.action==='profile'){const p=state.profiles.find(p=>p.name===change.name);if(change.model)p.model=change.model;if(change.instructions!==undefined)p.instructions=change.instructions;}
 if(change.action==='profile-create')state.profiles.push({name:change.name,model:state.defaults.model,instructions:'',workspaces:[]});
 if(change.action==='profile-delete')state.profiles=state.profiles.filter(p=>p.name!==change.name);
 if(change.action==='profile-assign')state.profiles.find(p=>p.name===change.name).workspaces.push(change.workspace);
 state.revision='settings-'+(Number(state.revision.split('-')[1])+1);
 return state;
}
const server=createServer(async(req,res)=>{
 const u=new URL(req.url,'http://localhost'),path=u.pathname;
 if(path==='/api/events'){res.writeHead(200,{'Content-Type':'text/event-stream'});streams.add(res);res.write(`data: ${JSON.stringify({reset:true,control:{can_write:true,web_workflows:1,commands},active:false,live:[]})}\n\n`);req.on('close',()=>streams.delete(res));return;}
 if(path==='/api/agent-events'){res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('data: {"revision":0}\n\n');return;}
 if(path==='/api/settings'){
  if(req.method==='GET'){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(state));return;}
  let raw='';for await(const chunk of req)raw+=chunk;const change=JSON.parse(raw);posts.push(change);
  const send=()=>{if(fail){res.writeHead(400,{'Content-Type':'application/json'});res.end(JSON.stringify({error:'Fixture save rejected'}));return;}const next=apply(change);res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(next));};if(hold)deliver=send;else send();return;
 }
 if(path.startsWith('/api/')){
  let value={};
  if(path==='/api/status')value={workspace:'/tmp/project',profile:'workspace-edit',web_workflows:1};
  if(path==='/api/threads')value={threads:[{session:'fixture',title:'Settings fixture',workspace:'/tmp/project',workspace_assigned:true,workspace_available:true},{session:'fixture2',title:'Another conversation',workspace:'/tmp/project',workspace_assigned:true,workspace_available:true}],projects:[]};
  if(path==='/api/history')value={records:Array.from({length:30},(_,i)=>({index:i,record:{k:'assistant',t:`Retained message ${i}\n\nA conversation does not own universal settings.`}})),before:0};
  if(path==='/api/commands')value={commands};if(path==='/api/agents')value=[];
  if(path==='/api/command')value={format:'json',body:{tasks:[]}};
  if(path==='/api/desktop/status')value={available:false};
  res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(value));return;
 }
 try{const file=resolve(root,'.'+(path==='/'?'/index.html':path));if(!file.startsWith(root+'/'))throw Error();res.writeHead(200,{'Content-Type':({'.js':'text/javascript','.css':'text/css','.html':'text/html','.woff2':'font/woff2','.svg':'image/svg+xml'})[extname(file)]||'application/octet-stream'});res.end(await readFile(file));}catch{res.writeHead(404).end();}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const origin=`http://127.0.0.1:${server.address().port}`;
const engine=process.env.GC_SETTINGS_ENGINE||'chromium';
const browser=engine==='webkit'?await webkit.launch({headless:true}):await chromium.launch({executablePath:process.env.GC_BROWSER_BINARY||(existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome')?'/snap/chromium/current/usr/lib/chromium-browser/chrome':undefined),headless:true,args:['--no-sandbox','--disable-dev-shm-usage']});
try{for(const width of [320,390,768,1440].filter(w=>!process.env.GC_SETTINGS_WIDTH||w===Number(process.env.GC_SETTINGS_WIDTH)))for(const colorScheme of ['light','dark']){
 state=copy(initial);requests=[];posts=[];hold=false;fail=false;
 const context=await browser.newContext({viewport:{width,height:950},colorScheme,isMobile:width<1000,hasTouch:width<1000,serviceWorkers:'block'}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));const pageRequests=requests;page.on('request',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/'))pageRequests.push(path);});
 await page.goto(origin+'/?settings=general&thread=must-not-open');await page.getByLabel('Default model',{exact:true}).waitFor();await page.evaluate(()=>document.fonts.ready);assert(await page.evaluate(()=>document.fonts.check('13px "Glass Sans"')));
 if(width<=700)assert(await page.getByLabel('Default model',{exact:true}).evaluate(n=>{const s=getComputedStyle(n);return parseFloat(s.fontSize)*(parseFloat(s.zoom)||1)<=13.1;}),'Settings values use compact visual type');
 assert.equal(await page.getByLabel('Default model',{exact:true}).inputValue(),'current-model');assert.equal(await page.getByLabel('Maximum output tokens',{exact:true}).inputValue(),'64000');
 assert.equal(await page.locator('#settings-page #command-form').count(),0);assert.equal(await page.locator('dialog[open]').count(),0);
 assert(!requests.some(p=>['/api/open','/api/history','/api/events','/api/command','/api/send'].includes(p)),requests.join(','));
 assert.equal(requests.filter(p=>p==='/api/settings').length,1);
 assert.equal(await page.locator('.sidebar').count(),1);assert.equal(await page.locator('.settings-navigation').count(),0);
 const connectionSelect=page.getByRole('combobox',{name:'Default connection',exact:true});
 assert.match(await connectionSelect.evaluate(n=>getComputedStyle(n).fontFamily),/Glass Sans/);
 await connectionSelect.click();const dropdown=page.getByRole('listbox',{name:'Default connection',exact:true});
 await dropdown.waitFor();await page.screenshot({animations:'disabled',path:`/tmp/glass-select-${engine}-${width}-${colorScheme}.png`});assert.match(await dropdown.evaluate(n=>getComputedStyle(n).fontFamily),/Glass Sans/);
 assert(await dropdown.evaluate(n=>{const r=n.getBoundingClientRect();return r.x>=0&&r.right<=innerWidth&&r.bottom<=innerHeight;}));
 await page.keyboard.press('End');assert(await connectionSelect.getAttribute('aria-activedescendant'));
 await page.keyboard.press('Escape');assert.equal(await dropdown.count(),0);assert(await page.locator('#settings-page').isVisible());
 assert.equal(posts.length,0,'Dismissing the menu must not save a setting');
 assert(await page.locator('.preference-control input:not([type=checkbox]),.preference-control textarea,.ui-select-trigger').evaluateAll(nodes=>nodes.every(n=>getComputedStyle(n).fontFamily.includes('Glass Sans'))),'Every setting value uses the bundled chat font');

 const menu=page.getByRole('button',{name:'Settings sections',exact:true});
 if(await menu.isVisible()){
  await menu.click();assert.equal(await menu.getAttribute('aria-expanded'),'true');
  await page.screenshot({animations:'disabled',path:`/tmp/glass-unified-navigation-${engine}-${width}-${colorScheme}.png`});assert.equal(await page.locator('#settings-page').evaluate(n=>n.inert),true);assert.equal(await page.locator('.sidebar').count(),1);assert.equal(await page.locator('.settings-navigation').count(),0);
  for(let i=0;i<16;i++)await page.keyboard.press('Tab');assert(await page.locator('#sidebar').evaluate(n=>n.contains(document.activeElement)));
  await page.keyboard.press('Escape');assert.equal(await menu.getAttribute('aria-expanded'),'false');
  assert.equal(await page.locator('#settings-page').evaluate(n=>n.inert),false);
 }
 assert(await page.locator('#settings-page').evaluate(n=>n.scrollWidth===n.clientWidth),'Settings must fit the viewport');
 assert(await page.locator('.settings-switch-target').first().evaluate(n=>{const r=n.getBoundingClientRect();return r.width>=44&&r.height>=44;}),'Switch touch targets must be at least 44 pixels');

 await page.locator('#settings-section-title').focus();await page.evaluate(()=>document.fonts.ready);await page.screenshot({animations:'disabled',path:`/tmp/glass-universal-settings-${width}-${colorScheme}.png`});
 assert(await page.locator('#settings-page').evaluate(n=>{const r=n.getBoundingClientRect(),sidebar=document.querySelector('#sidebar');const left=innerWidth>700?sidebar.getBoundingClientRect().width:0;return r.x===left&&r.y===0&&r.width===innerWidth-left&&r.height===innerHeight;}));
 assert(await page.evaluate(()=>{const html=document.documentElement,oldH=html.style.getPropertyValue('--app-height'),oldT=html.style.getPropertyValue('--viewport-top');html.style.setProperty('--app-height','520px');html.style.setProperty('--viewport-top','12px');const r=document.querySelector('#settings-page').getBoundingClientRect();html.style.setProperty('--app-height',oldH);html.style.setProperty('--viewport-top',oldT);return r.height===520&&r.top===12;}));
 // Review every section in both themes and keep controls within the content column.
 for(const title of ['Connections','Tools & skills','Instructions','Appearance','Advanced','General']){
  await section(page,title);
  assert(await page.locator('#settings-body').evaluate(n=>n.scrollWidth<=n.clientWidth),'Section overflow: '+title);
  await page.screenshot({animations:'disabled',path:`/tmp/glass-settings-${title.toLowerCase().replace(/[^a-z]+/g,'-')}-${width}-${colorScheme}.png`});
 }
 // Values are already present; editing never runs a slash command or opens a chat.
 await page.getByLabel('Default model',{exact:true}).fill('updated-model');await page.getByRole('button',{name:'Save Default model',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.connections[0].model,'updated-model');
 await page.getByRole('switch',{name:'Adaptive pacing'}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.defaults.adaptive_pacing,false);
 await page.getByLabel('Default spending limit',{exact:true}).fill('0.25');await page.getByRole('button',{name:'Save Default spending limit',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.defaults.cost_limit,.25);
 await page.getByLabel('Maximum output tokens',{exact:true}).fill('77777');await section(page,'Instructions');await page.getByText('Discard unsaved changes?',{exact:true}).waitFor();await page.getByRole('button',{name:'Keep editing',exact:true}).click();
 hold=true;await page.getByRole('button',{name:'Save Maximum output tokens',exact:true}).click();await page.waitForFunction(()=>document.querySelector('#settings-page').getAttribute('aria-busy')==='true');assert(await page.getByLabel('Maximum output tokens',{exact:true}).isDisabled());deliver();hold=false;await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.defaults.max_tokens,77777);
 await page.getByLabel('Maximum output tokens',{exact:true}).fill('88888');fail=true;await page.getByRole('button',{name:'Save Maximum output tokens',exact:true}).click();await page.getByText('Fixture save rejected',{exact:true}).waitFor();assert.equal(await page.getByLabel('Maximum output tokens',{exact:true}).inputValue(),'88888');fail=false;
 await page.getByRole('button',{name:'Refresh values',exact:true}).click();await page.getByText('Values refreshed. Unsaved edits are retained.',{exact:true}).waitFor();assert.equal(await page.getByLabel('Maximum output tokens',{exact:true}).inputValue(),'88888');
 await section(page,'Instructions');await page.getByRole('button',{name:'Discard changes',exact:true}).click();await page.getByLabel('Instructions',{exact:true}).fill('Global instructions\nMore guidance');await page.getByRole('button',{name:'Save Instructions',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.defaults.system_prompt,'Global instructions\nMore guidance');
 await page.getByLabel('New profile name',{exact:true}).fill('Browser preset');await page.getByRole('button',{name:'Create profile',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();
 let preset=page.locator('.connection-card').filter({has:page.getByRole('heading',{name:'Browser preset',exact:true})});await preset.getByLabel('Profile model',{exact:true}).fill('edited-profile');await preset.getByRole('button',{name:'Save Profile model',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.profiles.find(p=>p.name==='Browser preset').model,'edited-profile');
 preset=page.locator('.connection-card').filter({has:page.getByRole('heading',{name:'Browser preset',exact:true})});await preset.getByRole('button',{name:'Delete profile',exact:true}).click();await preset.locator('.connection-confirm').getByRole('button',{name:'Delete profile',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert(!state.profiles.some(p=>p.name==='Browser preset'));

 await page.goBack();await page.getByLabel('Default model',{exact:true}).waitFor();await page.goForward();await page.getByLabel('Instructions',{exact:true}).waitFor();
 await section(page,'Tools & skills');await page.getByRole('switch',{name:'todo',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert(state.extensions[0].enabled);
 await page.getByLabel('Custom extension command',{exact:true}).fill('node /tmp/fixture-extension.js');await page.getByRole('button',{name:'Add extension',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert(state.extensions.some(b=>b.id==='node /tmp/fixture-extension.js'&&b.enabled));

 await section(page,'Connections');await page.getByRole('button',{name:'Add connection',exact:true}).click();await page.locator('.connection-form').getByRole('button',{name:'Cancel',exact:true}).click();assert.equal(await page.locator('.connection-form').count(),0);await page.getByRole('button',{name:'Add connection',exact:true}).click();await page.getByLabel('Connection name',{exact:true}).fill('New account');await page.getByLabel('Model',{exact:true}).last().fill('new-account-model');await page.getByLabel('API key',{exact:true}).fill('FAKE_BROWSER_SECRET');await page.getByRole('button',{name:'Refresh values',exact:true}).click();await page.getByText('Values refreshed. Unsaved edits are retained.',{exact:true}).waitFor();assert.equal(await page.getByLabel('API key',{exact:true}).inputValue(),'FAKE_BROWSER_SECRET');
 await page.getByRole('button',{name:'Save connection',exact:true}).click();await page.locator('#settings-feedback').getByText('Saved',{exact:true}).waitFor();assert.equal(state.connections.length,2);assert(!page.url().includes('FAKE_BROWSER_SECRET'));assert(!await page.evaluate(()=>JSON.stringify(localStorage).includes('FAKE_BROWSER_SECRET')));
 await section(page,'Appearance');await page.getByRole('radio',{name:'Dark',exact:true}).check();assert(await page.getByRole('radio',{name:'Dark',exact:true}).isChecked());await page.screenshot({animations:'disabled',path:`/tmp/glass-settings-appearance-${width}-${colorScheme}.png`});await chooseOption(page,page.getByLabel('Activity animation',{exact:true}),'none');await chooseOption(page,page.getByLabel('Tool cards',{exact:true}),'compact');
 await page.reload();await page.getByRole('radio',{name:'Dark',exact:true}).waitFor();assert.equal(await page.locator('#preference-browser-animation').inputValue(),'none');assert.equal(await page.locator('#preference-browser-tools').inputValue(),'compact');
 const searchToggle=page.getByRole('button',{name:'Search settings',exact:true});if(await searchToggle.isVisible())await searchToggle.click();await page.getByRole('searchbox',{name:'Search settings',exact:true}).fill('pacing');await page.getByRole('switch',{name:'Adaptive pacing'}).waitFor();assert(!await page.getByLabel('Default model',{exact:true}).isVisible());await page.getByRole('searchbox',{name:'Search settings',exact:true}).fill('');
 assert(!requests.some(p=>['/api/open','/api/history','/api/events','/api/send'].includes(p)),requests.join(','));
 // A chat is opened only when explicitly returning to it. Existing drafts stay mounted.
 await page.goto(origin+'/?thread=fixture');await page.waitForFunction(()=>document.querySelector('#ownership').textContent==='Release control');await page.locator('#prompt').fill('Keep my draft');await page.locator('#conversation').evaluate(n=>n.scrollTop=120);const top=await page.locator('#conversation').evaluate(n=>n.scrollTop),historyReads=requests.filter(p=>p==='/api/history').length;
 await page.locator('#settings-sections a[data-section=general]').evaluate(n=>n.click());await page.getByLabel('Default model',{exact:true}).waitFor();assert.equal(requests.filter(p=>p==='/api/history').length,historyReads);await page.getByRole('button',{name:'Back to conversation',exact:true}).click();assert.equal(await page.locator('#prompt').inputValue(),'Keep my draft');assert.equal(await page.locator('#conversation').evaluate(n=>n.scrollTop),top);
 await page.locator('#prompt').fill('/desktop');await page.locator('#composer').evaluate(n=>n.requestSubmit());await page.locator('#desktop-title').waitFor();await page.keyboard.press('Escape');
 // Sidebar identity, dirty guards and explicit chat selection share one navigation.
 await page.locator('#prompt').fill('Keep my draft');
 const sidebar=await page.locator('#sidebar').elementHandle();
 if(await page.locator('#menu').isVisible())await page.locator('#menu').click();
 await page.locator('#settings-sections a[data-section=general]').click();await page.getByLabel('Default model',{exact:true}).waitFor();
 await page.getByLabel('Default model',{exact:true}).fill('unsaved-navigation-draft');
 const opens=requests.filter(p=>p==='/api/open').length;
 async function chooseOther(){if(await page.locator('#settings-menu').isVisible())await page.locator('#settings-menu').click();const project=page.locator('.project-group').first();if(!await project.evaluate(n=>n.open))await project.locator('summary').click();await page.getByRole('button',{name:'Another conversation',exact:false}).click();}
 await chooseOther();await page.getByText('Discard unsaved changes?',{exact:true}).waitFor();
 assert.equal(requests.filter(p=>p==='/api/open').length,opens,'A pending navigation must not open the target chat');
 await page.getByRole('button',{name:'Keep editing',exact:true}).click();assert.equal(await page.getByLabel('Default model',{exact:true}).inputValue(),'unsaved-navigation-draft');
 await chooseOther();await page.getByRole('button',{name:'Discard changes',exact:true}).click();
 await page.locator('#settings-page').waitFor({state:'hidden'});await page.waitForURL(/thread=fixture2/);
 assert.equal(requests.filter(p=>p==='/api/open').length,opens+1);assert(await sidebar.evaluate(n=>n===document.querySelector('#sidebar')));
 assert.equal(await page.locator('.sidebar').count(),1);
 // Browser Back through Settings must restore the conversation named in its URL.
 await page.goBack();await page.getByLabel('Default model',{exact:true}).waitFor();
 await page.getByRole('button',{name:'Back to conversation',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('#thread-name').textContent==='Settings fixture'&&!new URL(location.href).searchParams.has('settings'));
 assert.equal(new URL(page.url()).searchParams.get('thread'),'fixture');assert.equal(await page.locator('#prompt').inputValue(),'Keep my draft');
 assert(await sidebar.evaluate(n=>n===document.querySelector('#sidebar')));
 assert.deepEqual(errors,[]);await context.close();console.log(`PASS universal Settings ${engine} ${width}px ${colorScheme}: direct values, one global snapshot, zero chat loading, saved defaults/connections/instructions/tools, validation and refresh, appearance, routes/search, draft/scroll, Computer`);
}}finally{await browser.close();server.closeAllConnections();await new Promise(r=>server.close(r));}
