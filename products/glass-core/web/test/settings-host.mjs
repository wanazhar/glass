// Actual gateway + actual settings API, supplied by the isolated Python host smoke.
import {chromium} from 'playwright-core';
import assert from 'node:assert/strict';
import {existsSync} from 'node:fs';
const origin=process.env.GC_SETTINGS_TEST_ORIGIN,token=process.env.GC_SETTINGS_TEST_TOKEN;
assert(origin&&token,'Run through smoke-web-settings.py with GC_SETTINGS_BROWSER=1');
const browser=await chromium.launch({executablePath:process.env.GC_BROWSER_BINARY||(existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome')?'/snap/chromium/current/usr/lib/chromium-browser/chrome':undefined),headless:true,args:['--no-sandbox','--disable-dev-shm-usage']});
const context=await browser.newContext({viewport:{width:390,height:950},hasTouch:true,serviceWorkers:'block',extraHTTPHeaders:{Authorization:'Bearer '+token}}),page=await context.newPage(),requests=[],errors=[];
page.on('request',r=>{const u=new URL(r.url());if(u.pathname.startsWith('/api/'))requests.push(u.pathname);});page.on('pageerror',e=>errors.push(e.message));
try{
 await page.goto(origin+'/?settings=general');await page.getByLabel('Default model',{exact:true}).waitFor();
 await page.getByLabel('Default model',{exact:true}).fill('browser-default-model');await page.getByRole('button',{name:'Save Default model',exact:true}).click();await page.getByText('Saved',{exact:true}).waitFor();
 await page.reload();await page.getByLabel('Default model',{exact:true}).waitFor();assert.equal(await page.getByLabel('Default model',{exact:true}).inputValue(),'browser-default-model');
 assert(!requests.some(path=>['/api/threads','/api/open','/api/events','/api/history','/api/send','/api/command'].includes(path)),requests.join(','));
 assert.deepEqual(errors,[]);
 console.log('PASS real browser + real host API: default saved and reloaded with no chat list, session or command transport');
}finally{await context.close();await browser.close();}
