import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, extname } from "node:path";
const root=resolve("dist"),streams=new Set(),uploads=[];
const commands=JSON.parse(await readFile("test/fixtures/commands.json","utf8")).commands;
const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC","base64");
let attachments=[],todo=[{id:"1",title:"Read project files",status:"completed"},{id:"2",title:"Update light mode tokens",status:"completed"},{id:"3",title:"Implement dark mode tokens",status:"in_progress"},{id:"4",title:"Register the theme toggle",status:"pending"},{id:"5",title:"Run lint and production build",status:"pending"}];
const records=()=>[
 {k:"user",t:"Update our dark-mode tokens, register a reusable theme toggle, and verify the build."},
 {k:"call",id:"search",name:"web_search",args:JSON.stringify({query:"accessible theme toggles"})},
 {k:"tool",id:"search",name:"web_search",t:JSON.stringify({results:[{title:"Accessible controls",url:"https://www.w3.org/WAI/"},{title:"Browser documentation",url:"https://developer.mozilla.org/en-US/"}]})},
 {k:"custom",c:"state/todo",d:JSON.stringify({tasks:todo})},
 {k:"assistant",t:'The theme selection now persists through reloads.\n\n```tsx file=theme-toggle.tsx\nconst nextTheme = theme === "dark" ? "light" : "dark";\ndocument.documentElement.classList.toggle("dark", nextTheme === "dark");\nlocalStorage.setItem("theme", nextTheme);\n```\n\n```diff file=theme.css\n--- a/theme.css\n+++ b/theme.css\n-old: white;\n+new: var(--surface);\n```'}];
const emit=events=>{for(const response of streams)response.write(`data: ${JSON.stringify({events})}\n\n`);};
const server=createServer(async(request,response)=>{
 const url=new URL(request.url,"http://localhost"),path=url.pathname;
 if(path==="/api/events"){response.writeHead(200,{"Content-Type":"text/event-stream","Cache-Control":"no-cache"});streams.add(response);response.write(`data: ${JSON.stringify({reset:true,control:{can_write:true,web_workflows:1,commands},live:[],active:false})}\n\n`);request.on("close",()=>streams.delete(response));return;}
 if(path==="/api/upload"){for await(const chunk of request){}uploads.push(()=>{response.writeHead(200,{"Content-Type":"application/json"});response.end(JSON.stringify({token:"a".repeat(64)}));});return;}
 if(path==="/api/file"){response.writeHead(200,{"Content-Type":"text/plain"});response.end("export const Button = () => null;");return;}
 if(path.startsWith("/api/")){
  let value={};
  if(path==="/api/status")value={workspace:"/tmp/project-sea",profile:"workspace-edit",web_workflows:1};
  if(path==="/api/threads")value={threads:[{session:"fixture",title:"Theme controls",workspace:"/tmp/project-sea",workspace_assigned:true,workspace_available:true,model:"Local model"}],projects:[]};
  if(path==="/api/history")value={records:records().map((record,index)=>({index,record})),before:0};
  if(path==="/api/context")value={attachments};
  if(path==="/api/commands")value={commands};
  if(path==="/api/browse"){const directory=url.searchParams.get("path")||"/tmp/project-sea";value={path:directory,parent:"/tmp",entries:directory.endsWith("/src")?[{name:"button.tsx",kind:"file",bytes:40},{name:"button.module.css",kind:"file",bytes:25}]:[{name:"src",kind:"directory",bytes:0},{name:"README.md",kind:"file",bytes:100}],next:null};}
  if(path==="/api/send"){let raw="";for await(const chunk of request)raw+=chunk;const message=JSON.parse(raw).message;if(message.t==="command"){if(message.name==="attach")attachments=[{path:"/tmp/uploads/"+"a".repeat(64),source:`Uploaded ${message.values.upload_name} · fixture`,bytes:png.length,mediaType:"image/png"}];if(message.name==="context"&&message.values.action==="remove")attachments=[];emit([{t:"command_result",request_id:message.request_id,result:{format:"text",body:"Saved"}}]);}}
  response.writeHead(200,{"Content-Type":"application/json"});response.end(JSON.stringify(value));return;
 }
 try{const file=resolve(root,"."+(path==="/"?"/index.html":path));if(!file.startsWith(root+"/"))throw Error("Invalid path");const bytes=await readFile(file);response.writeHead(200,{"Content-Type":({".js":"text/javascript",".css":"text/css",".html":"text/html",".svg":"image/svg+xml",".woff2":"font/woff2"})[extname(file)]||"application/octet-stream"});response.end(bytes);}catch{response.writeHead(404).end();}
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
const browser=await chromium.launch({executablePath:process.env.GC_BROWSER_BINARY||(existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")?"/snap/chromium/current/usr/lib/chromium-browser/chrome":undefined),headless:true,args:["--no-sandbox","--disable-dev-shm-usage"]});
try{for(const width of [390,768,1440])for(const colorScheme of ["light","dark"]){
 attachments=[];todo[2].status="in_progress";
 const context=await browser.newContext({viewport:{width,height:1000},colorScheme,serviceWorkers:"block",reducedMotion:"reduce"});
 await context.addInitScript(()=>{window.SpeechRecognition=class{start(){window.testRecognition=this;}stop(){this.onend?.();}};});
 const page=await context.newPage(),errors=[];page.on("pageerror",error=>errors.push(String(error)));
 await page.goto(`http://127.0.0.1:${server.address().port}/?thread=fixture`);
 await page.waitForFunction(()=>document.querySelector("#ownership").textContent==="Release control");
 await page.getByText("3 steps left",{exact:true}).waitFor();
 assert.equal(await page.locator('.task-card-row[data-status="completed"]').count(),2);
 assert.equal(await page.locator(".code-filename").filter({hasText:"theme-toggle.tsx"}).count(),1);
 assert.equal(await page.locator(".diff-counts .added").textContent(),"+1");assert.equal(await page.locator(".diff-counts .removed").textContent(),"−1");
 assert.match(await page.locator(".code-line-numbers").first().textContent(),/^1\n2\n3/);
 await page.locator('.tool-row[data-call-id="search"] > summary').click();assert.equal(await page.locator(".tool-source").count(),2);
 assert.equal(await page.locator('.tool-source[href="https://www.w3.org/WAI/"]').count(),1);
 await page.locator(".task-card-heading").click();assert.equal(await page.locator(".task-card").evaluate(n=>n.open),false);
 todo[2].status="completed";emit([{t:"tool",call_id:"todo-change",name:"state"},{t:"tool_end",call_id:"todo-change",output:"Checklist updated",summary:"updated"}]);
 await page.getByText("2 steps left",{exact:true}).waitFor();assert.equal(await page.locator(".task-card").evaluate(n=>n.open),false);await page.locator(".task-card-heading").click();
 emit([{t:"tool",call_id:"live-search",name:"web_search",command:"theme accessibility"}]);await page.waitForFunction(()=>document.querySelector("#activity").dataset.phase==="search");
 await page.waitForFunction(()=>document.querySelector(".agent-elapsed").textContent!=="");assert.equal(await page.locator(".agent-sigil i").count(),9);assert.equal(await page.locator(".agent-sigil i").first().evaluate(n=>getComputedStyle(n).animationName),"none");
 const prompt=page.locator("#prompt");await prompt.fill("Keep this draft");await page.locator("#voice-input").click();
 await page.evaluate(()=>{window.testRecognition.onresult({results:[Object.assign([{transcript:"and dictated text"}],{isFinal:true})]});window.testRecognition.onend();});assert.equal(await prompt.inputValue(),"Keep this draft and dictated text");
 await page.locator("#attachment-input").setInputFiles({name:"image.png",mimeType:"image/png",buffer:png});await page.locator("#upload-previews .attachment-thumbnail").waitFor();await page.locator("#upload-previews progress").waitFor();
 while(!uploads.length)await new Promise(resolve=>setTimeout(resolve,20));uploads.shift()();await page.getByRole("button",{name:"Remove attachment image.png"}).waitFor();assert.equal(await page.locator("#context-chips .attachment-thumbnail").count(),1);assert.equal(await prompt.inputValue(),"Keep this draft and dictated text");
 await page.getByRole("button",{name:"Remove attachment image.png"}).click();await page.getByRole("button",{name:"Remove attachment image.png"}).waitFor({state:"detached"});
 await page.locator("#files-toggle").click();const tree=page.getByRole("tree",{name:"Workspace files"});await tree.waitFor();const src=tree.getByRole("treeitem",{name:"src",exact:true});await src.focus();await src.press("ArrowRight");
 const file=tree.getByRole("treeitem",{name:"button.tsx",exact:true});await file.waitFor();await src.press("ArrowRight");assert.equal(await file.evaluate(n=>n===document.activeElement),true);await file.press("ArrowLeft");assert.equal(await src.evaluate(n=>n===document.activeElement),true);await src.press("ArrowLeft");assert.equal(await src.getAttribute("aria-expanded"),"false");await src.press("End");assert.equal(await tree.getByRole("treeitem",{name:"README.md",exact:true}).evaluate(n=>n===document.activeElement),true);
 await page.screenshot({path:`/tmp/glass-agentic-tree-${width}-${colorScheme}.png`});await page.locator("#details-close").click();emit([{t:"tool_end",call_id:"live-search",output:"Finished search",summary:"exit 0"},{t:"done"}]);await page.waitForFunction(()=>document.querySelector("#activity").dataset.phase==="idle");assert.equal(await page.locator(".agent-elapsed").textContent(),"");
 await page.locator("#conversation").evaluate(n=>n.scrollTop=n.scrollHeight);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),"Horizontal overflow");await page.screenshot({path:`/tmp/glass-agentic-chat-${width}-${colorScheme}.png`});assert.deepEqual(errors,[]);await context.close();console.log(`PASS agentic chat ${width}px ${colorScheme}: checklist, code, sources, voice draft, upload/removal and keyboard tree`);
}}finally{for(const release of uploads)release();await browser.close();for(const response of streams)response.end();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
