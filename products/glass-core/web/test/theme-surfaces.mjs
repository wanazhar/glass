import {chromium,webkit} from 'playwright-core';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,extname} from 'node:path';
const root=resolve('dist');
const server=createServer(async(req,res)=>{
 const path=new URL(req.url,'http://localhost').pathname;
 if(path.startsWith('/api/')){res.setHeader('Content-Type','application/json');res.end(JSON.stringify(path==='/api/status'?{profile:'workspace-edit',workspace:'/tmp'}:{threads:[],projects:[]}));return;}
 try{const file=resolve(root,'.'+(path==='/'?'/index.html':path));if(!file.startsWith(root+'/'))throw Error();const body=await readFile(file);res.setHeader('Content-Type',({'.js':'text/javascript','.css':'text/css','.html':'text/html','.svg':'image/svg+xml','.woff2':'font/woff2'})[extname(file)]||'application/octet-stream');res.end(body);}catch{res.writeHead(404).end();}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const url=`http://127.0.0.1:${server.address().port}`;
try{for(const engine of ['chromium','webkit']){
 const browser=await (engine==='webkit'?webkit.launch():chromium.launch({executablePath:'/snap/chromium/current/usr/lib/chromium-browser/chrome',args:['--no-sandbox']}));
 try{for(const os of ['light','dark'])for(const saved of ['dark','light','system']){
  const context=await browser.newContext({viewport:{width:390,height:844},isMobile:true,hasTouch:true,colorScheme:os,serviceWorkers:'block'});
  await context.addInitScript(mode=>{if(!localStorage.getItem('glass:theme'))localStorage.setItem('glass:theme',mode);Object.defineProperty(navigator,'standalone',{value:true});},saved);
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
  let release;const gate=new Promise(r=>release=r);
  await page.route('**/style.css*',async route=>{await gate;await route.continue();});
  await page.route('**/boot.js*',async route=>{await gate;await route.continue();});
  await page.goto(url,{waitUntil:'commit'});await page.locator('#launch').waitFor();
  const isLight=saved==='light'||(saved==='system'&&os==='light');
  const expected=isLight?'rgb(247, 248, 246)':'rgb(20, 25, 22)';
  const launch=await page.evaluate(()=>({launch:getComputedStyle(document.querySelector('#launch')).backgroundColor,body:getComputedStyle(document.body).backgroundColor,root:getComputedStyle(document.documentElement).backgroundColor}));
  assert.deepEqual(launch,{launch:expected,body:expected,root:expected},'Stored theme must paint correctly even before app CSS loads');
  assert.equal(await page.locator('meta[name=apple-mobile-web-app-status-bar-style]').count(),0,'No legacy status-bar override at launch');
  release();await page.locator('.app').waitFor();
  assert(await page.locator('html').evaluate(n=>n.classList.contains('standalone')),'Exercise Home Screen code path');
  assert.equal(await page.locator('.toolbar').evaluate(n=>getComputedStyle(n).position),'sticky');
  await page.locator('#menu').click();
  for(let i=0;i<2;i++){
   await page.locator('#theme').click();
   assert(await page.locator('#sidebar').evaluate(n=>n.classList.contains('open')),'Theme switch keeps drawer open');
   const palette=await page.evaluate(()=>({light:document.documentElement.classList.contains('light'),body:getComputedStyle(document.body).backgroundColor,header:getComputedStyle(document.querySelector('.toolbar')).backgroundColor,meta:document.querySelector('meta[name=theme-color]').content,scheme:getComputedStyle(document.documentElement).colorScheme,count:document.querySelectorAll('meta[name=theme-color]').length}));
   const bg=palette.light?'rgb(247, 248, 246)':'rgb(20, 25, 22)';
   const edge=await page.locator('.pwa-status-background').evaluate(n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return {top:r.top,width:r.width,height:r.height,viewport:innerWidth,background:s.backgroundColor,position:s.position,z:Number(s.zIndex),pointer:s.pointerEvents};});
   assert.equal(edge.top,0);assert.equal(edge.width,edge.viewport);assert(edge.height>10&&edge.height<844*.9);assert.equal(edge.background,bg);assert.equal(edge.position,'fixed');assert(edge.z>12);assert.equal(edge.pointer,'none');
   assert.equal(palette.body,bg);assert.equal(palette.header,bg);assert.equal(palette.meta,palette.light?'#f7f8f6':'#141916');assert.equal(palette.scheme,palette.light?'light':'dark');assert.equal(palette.count,1);
  }
  await page.screenshot({path:`/tmp/glass-theme-drawer-${engine}-${os}-${saved}.png`});
  await page.evaluate(()=>{
   const stale=document.createElement('meta');stale.name='apple-mobile-web-app-status-bar-style';stale.content='default';document.head.append(stale);
   window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}));
  });
  assert.equal(await page.locator('meta[name=apple-mobile-web-app-status-bar-style]').count(),0,'Warm resume removes stale native chrome overrides');
  const selected=await page.evaluate(()=>localStorage.getItem('glass:theme'));
  await page.reload();await page.locator('.app').waitFor();
  assert.equal(await page.evaluate(()=>document.documentElement.classList.contains('light')),selected==='light'||(selected==='system'&&os==='light'));
  assert.equal(await page.locator('meta[name=apple-mobile-web-app-status-bar-style]').count(),0);

  assert.deepEqual(errors,[]);
  await page.screenshot({path:`/tmp/glass-theme-${engine}-${os}-${saved}.png`});
  await context.close();console.log(`PASS ${engine}: iOS ${os}, stored ${saved}, pre-CSS startup, sidebar and viewport tint surfaces`);
 }}finally{await browser.close();}
}}finally{server.closeAllConnections();await new Promise(r=>server.close(r));}
