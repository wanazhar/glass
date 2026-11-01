// Real private Connect/Xvfb benchmark, invoked by benchmark-desktop-updates.py.
const { webkit } = await import(process.env.GC_PLAYWRIGHT_MODULE || "playwright-core");
import { readFile, writeFile } from "node:fs/promises";
import { createServer, request } from "node:http";
import assert from "node:assert/strict";
const config = JSON.parse(await readFile(process.argv[2], "utf8"));
assert(config.captureRaw, "Benchmark requires the raw latest-frame capture build");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let lastUpdate = null;
const upstreamDurations=[];
let offset = null, updateBytes = 0, updates = 0, regions = 0, full = 0, inputs = 0, legacy = 0;
const server = createServer((req, res) => {
  const requestedAt=Date.now();
  const headers = { ...req.headers, host: `127.0.0.1:${config.port}`, authorization: `Bearer ${config.token}` };
  if (req.method === "POST") headers.origin = `http://127.0.0.1:${config.port}`;
  const upstream = request({ hostname: "127.0.0.1", port: config.port, path: req.url, method: req.method, headers }, (response) => {
    if (req.url.startsWith("/api/events")) { res.writeHead(response.statusCode, response.headers);response.pipe(res); return; }
    const chunks = [];
    response.on("data", (data) => chunks.push(data));
    response.on("end", async () => {
      const body = Buffer.concat(chunks);
      if (req.url.startsWith("/api/desktop/status") && response.statusCode === 200) {
        const status = JSON.parse(body);
        if (Number.isFinite(status.frameTimeMs)) offset = Date.now() - status.frameTimeMs - status.frameAgeMs;
      }
      const isUpdate = req.url === "/api/desktop/updates";
      if (isUpdate && response.statusCode === 200) {
        upstreamDurations.push(Date.now()-requestedAt);
        const value = JSON.parse(body); lastUpdate = {tile:value.tiles?.[0]?{index:value.tiles[0].index,x:value.tiles[0].x,y:value.tiles[0].y,width:value.tiles[0].width,height:value.tiles[0].height,hashLength:value.tiles[0].hash?.length,dataLength:value.tiles[0].data?.length}:null,version:value.version, session:!!value.sessionId, tileSize:value.tileSize, columns:value.columns, hashes:value.hashes?.length, tiles:value.tiles?.length, fullBytes:value.full?.length, seq:value.frameSeq}; updates += 1; updateBytes += body.length;
        regions += value.tiles.length; full += Boolean(value.full);
      }
      if (req.url.startsWith("/api/desktop/input") && response.statusCode === 200) inputs += 1;
      if (/^\/api\/desktop\/(stream|frame)(\?|$)/.test(req.url)) legacy += 1;
      const outgoing = { ...response.headers }; delete outgoing["transfer-encoding"]; delete outgoing.connection;
      if (req.url.startsWith("/api/")) await sleep(50);
      res.writeHead(response.statusCode, outgoing);
      const rate = Number(process.env.GC_BENCH_KIBPS || 128) * 1024;
      for (let index = 0; index < body.length && !res.destroyed; index += 8192) {
        const chunk = body.subarray(index, index + 8192);
        if (isUpdate) await sleep(chunk.length * 1000 / rate);
        res.write(chunk);
      }
      res.end();
    });
  });
  upstream.on("error", () => { if (!res.headersSent) res.writeHead(502);res.end(); });
  req.pipe(upstream);res.on("close", () => upstream.destroy());
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
let browser;
try {
  if (process.env.GC_BENCH_NO_BROWSER) {
    const base=`http://127.0.0.1:${server.address().port}`;
    const api=async(path,value)=>{ const response=await fetch(base+path,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(value)});assert(response.ok);return response.status===204?null:response.json(); };
    let frame=await api("/api/desktop/updates",{sessionId:config.sessionId,after:0});
    const initialBytes=frame.full.length;
    const acks=[],paints=[], packets=[];
    for(let i=0;i<12;i++) {
      const at=Date.now();
      await api("/api/desktop/input",{action:"pointer-move",x:0.2+i*0.03,y:0.3+i*0.02});
      acks.push(Date.now()-at);
      let painted=false;
      while(Date.now()-at<6000) {
        const next=await api("/api/desktop/updates",{sessionId:config.sessionId,after:frame.frameSeq});
        if(!next)continue;
        frame=next; packets.push({full:!!frame.full,tiles:frame.tiles.length,bytes:JSON.stringify(frame).length});
        if(frame.full || frame.tiles.some(t=>t.index===21)){painted=true;break;}
      }
      if(!painted) console.log(JSON.stringify({sourceFailure:true,step:i,acks,paints,upstreamDurations:upstreamDurations.slice(-20),packets:packets.slice(-20),lastUpdate}));
      assert(painted,"source marker did not change");paints.push(Date.now()-at);
    }
    const p95=(values)=>[...values].sort((a,b)=>a-b)[Math.floor(values.length*0.95)];
    console.log(JSON.stringify({sourceOnly:true,captureRaw:config.captureRaw,captureLowDelay:config.captureLowDelay,initialBase64Bytes:initialBytes,inputAckP95Ms:p95(acks),inputToPixelsP95Ms:p95(paints),samples:paints,upstreamP95Ms:p95(upstreamDurations),packets:packets.filter(p=>p.tiles).slice(-12)}));
  } else {
  browser = await webkit.launch(process.env.GC_WEBKIT_BINARY ? { executablePath: process.env.GC_WEBKIT_BINARY } : {});
  const page = await browser.newPage({ viewport: { width: 1024, height: 1366 }, deviceScaleFactor: 2, hasTouch: true, serviceWorkers: "block" });
  const errors = [];page.on("pageerror", (error) => errors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.bringToFront();
  await page.mouse.move(0,0);
  await page.locator("#desktop-toggle").click();
  try {
    const deadline=Date.now()+30000;
    while (!await page.evaluate(()=>document.querySelector("#desktop-regions")?.hidden===false)) {
      if (Date.now()>=deadline) throw Error("Region canvas did not appear");
      await sleep(50);
    }
  } catch(error) {
    console.log(JSON.stringify({errors, updates, lastUpdate, legacy, ui:await page.evaluate(()=>({status:document.querySelector("#desktop-status")?.textContent,imageWidth:document.querySelector("#desktop-stream")?.naturalWidth,canvas:!!document.querySelector("#desktop-regions"),hidden:document.querySelector("#desktop-regions")?.hidden,data:{...document.querySelector("#desktop-viewport")?.dataset}}))}));
    throw error;
  }
  await sleep(2000);
  const start = Date.now(), startBytes = updateBytes, startInputs = inputs;
  // Measure inside the browser. Repeated automation round trips and GPU
  // readbacks can themselves stall headless WebKit on a shared ARM host.
  const measured = await page.evaluate(async ({duration,uiInput}) => {
    const viewport=document.querySelector("#desktop-viewport"), canvas=document.querySelector("#desktop-regions");
    const context=canvas.getContext("2d"), samples=[], inputLatencies=[], inputAcks=[];
    const color=()=>[...context.getImageData(96,96,1,1).data].join(",");
    const started=performance.now();
    for(let step=0; performance.now()-started<duration*1000; step++) {
      const before=color(), at=performance.now();
      let observer,timer;
      const painted=new Promise((resolve,reject)=>{
        observer=new MutationObserver(()=>{
          if(color()!==before)resolve(performance.now()-at);
        });
        observer.observe(canvas,{attributes:true,attributeFilter:["data-paint-seq"]});
        timer=setTimeout(()=>reject(Error("Visible input response exceeded 4 seconds")),4000);
      });
      try {
        const x=0.2+(step%6)*0.1,y=0.3+(step%3)*0.1;
        if(uiInput) {
          const box=viewport.getBoundingClientRect();
          const width=Math.min(box.width,box.height*1.6),height=width/1.6;
          viewport.dispatchEvent(new PointerEvent("pointermove",{bubbles:true,pointerType:"mouse",pointerId:1,isPrimary:true,
            clientX:box.x+(box.width-width)/2+width*x,clientY:box.y+(box.height-height)/2+height*y}));
        } else {
          const response=await fetch("/api/desktop/input",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({action:"pointer-move",x,y})});
          if(!response.ok)throw Error("Test input rejected");
          await response.json();
        }
        inputAcks.push(performance.now()-at);
        inputLatencies.push(await painted);
        samples.push({at:Date.now(),time:Number(viewport.dataset.frameTimeMs),fresh:viewport.dataset.frameFresh==="true",
          width:canvas.width,height:canvas.height,visible:!canvas.hidden,status:document.querySelector("#desktop-status").textContent});
      } finally {observer.disconnect();clearTimeout(timer);}
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    return {samples,inputLatencies,inputAcks,visibility:document.visibilityState, resources:performance.getEntriesByType("resource").filter(e=>e.name.includes("/api/desktop/")).slice(-12).map(e=>({path:new URL(e.name).pathname,ms:Math.round(e.duration)}))};
  }, {duration:config.duration,uiInput:Boolean(process.env.GC_BENCH_UI_INPUT)});
  const samples=measured.samples.map(value=>({...value,age:Math.max(0,value.at-offset-value.time)}));
  const inputLatencies=measured.inputLatencies;
  const ages = samples.map((sample) => sample.age).sort((a,b) => a-b);
  const percentile = (fraction) => Math.round(ages[Math.min(ages.length - 1, Math.floor(ages.length * fraction))]);
  const result = { transport: "regions", inputMode:process.env.GC_BENCH_UI_INPUT ? "viewer-pointer" : "api",visibility:measured.visibility,inputAckP95Ms:[...measured.inputAcks].sort((a,b)=>a-b)[Math.floor(measured.inputAcks.length*.95)],resources:measured.resources, captureRaw:config.captureRaw,captureLowDelay: config.captureLowDelay, kilobytesPerSecond: Math.round((updateBytes-startBytes)/(Date.now()-start)),
    upstreamP95Ms:[...upstreamDurations].sort((a,b)=>a-b)[Math.floor(upstreamDurations.length*.95)],requests: updates, changedTiles: regions, fullFrames: full, inputRequests: inputs-startInputs, legacyRequests: legacy,
    samples: samples.length, p50Ms: percentile(0.5), p95Ms: percentile(0.95), maxMs: percentile(1),
    inputToPaintP95Ms: [...inputLatencies].sort((a,b)=>a-b)[Math.floor(inputLatencies.length*0.95)],
    freshPercent: Math.round(100*samples.filter((s) => s.fresh).length/samples.length),
    width: samples.at(-1)?.width, height: samples.at(-1)?.height, status: samples.at(-1)?.status };
  console.log(JSON.stringify(result));
  if(process.env.GC_BENCH_NATIVE_INPUT) {
    const box=await page.locator("#desktop-viewport").boundingBox();
    const width=Math.min(box.width,box.height*1.6),height=width/1.6;
    const x=box.x+(box.width-width)/2+width*.75,y=box.y+(box.height-height)/2+height*.75;
    for(const mode of ["mouse","trackpad","direct-touch"]) {
      if(mode!=="mouse") await page.locator(mode==="trackpad"?"#desktop-trackpad":"#desktop-touch-mode").evaluate(button=>button.click());
      await sleep(300);
      const before=await page.evaluate(()=>[...document.querySelector("#desktop-regions").getContext("2d").getImageData(96,96,1,1).data].join(","));
      const count=inputs,at=Date.now();
      if(mode==="mouse") await page.mouse.move(x,y);else await page.touchscreen.tap(x,y);
      await page.waitForFunction(before=>[...document.querySelector("#desktop-regions").getContext("2d").getImageData(96,96,1,1).data].join(",")!==before,before,{timeout:3000,polling:50});
      assert(inputs>count,mode+" did not reach the desktop");
      console.log(JSON.stringify({nativeInput:mode,inputToPixelsMs:Date.now()-at}));
    }
  }
  assert(samples.length > 20);assert.equal(legacy,0);assert(inputs-startInputs>10);
  assert(samples.every((sample) => sample.visible && sample.width===1280 && sample.height===800));
  assert(result.p95Ms < 1000, "desktop display latency exceeded 1 second under throttling");
  assert(result.freshPercent > 95);assert(result.inputToPaintP95Ms < 1000);assert.deepEqual(errors,[]);
  }
} finally {
  if (browser) await browser.close();
  server.closeAllConnections();await new Promise((resolve) => server.close(resolve));
}
