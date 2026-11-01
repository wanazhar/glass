const { chromium, webkit } = await import(process.env.GC_PLAYWRIGHT_MODULE || "playwright-core");
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";

let fixture, count = 0, mode = "valid";
const server = createServer(async (req, res) => {
  if (req.url === "/api/desktop/updates") {
    count += 1;
    let body = "";
    for await (const chunk of req) body += chunk;
    const checkpoint = JSON.parse(body);
    assert.equal(checkpoint.sessionId, "fixture");
    if (mode === "auth") { res.writeHead(403).end(); return; }
    const seq = checkpoint.after + 1;
    const hashes = Array(260).fill("0".repeat(16));
    if (seq >= 2) hashes[0] = "1".repeat(16);
    const tile = { index: 0, x: 0, y: 0, width: 64, height: 64, hash: "1".repeat(16), data: fixture.tile };
    const value = { version: 1, sessionId: mode === "session" ? "other" : "fixture",
      width: 1280, height: 800, tileSize: 64, columns: 20,
      frameSeq: seq, frameTimeMs: seq * 200, hashes: seq === 1 ? hashes : null,
      full: seq === 1 ? fixture.full : null, tiles: seq === 2 ? [tile] : seq === 3 ? [{...tile,index:1,x:64,mime:"image/jpeg",data:fixture.jpegTile}] : [] };
    if (mode === "duplicate" && seq === 2) value.tiles.push(tile);
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(value));
    return;
  }
  if (req.url === "/") { res.writeHead(200, { "Content-Type": "text/html" }).end("<!doctype html><body>Desktop updates test</body>"); return; }
  const name = req.url.slice(1);
  if (!["desktop-updates.js", "owner-auth.js"].includes(name)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "Content-Type": "text/javascript" });
  res.end(await readFile(new URL("../src/" + name, import.meta.url)));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
try {
  for (const engine of [chromium, webkit]) {
    const browser = await engine.launch(engine === chromium ? {
      executablePath: "/snap/chromium/current/usr/lib/chromium-browser/chrome",
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    } : (process.env.GC_WEBKIT_BINARY ? { executablePath: process.env.GC_WEBKIT_BINARY } : {}));
    try {
      const page = await browser.newPage();
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      fixture = await page.evaluate(() => {
        const canvas = document.createElement("canvas"); canvas.width = 1280; canvas.height = 800;
        const context = canvas.getContext("2d"); context.fillStyle = "#0000ff"; context.fillRect(0, 0, 1280, 800);
        const full = canvas.toDataURL("image/jpeg", 0.9).split(",")[1];
        canvas.width = 64; canvas.height = 64;
        context.fillStyle = "#ff0000"; context.fillRect(0, 0, 64, 64);
        return { full, tile: canvas.toDataURL("image/png").split(",")[1], jpegTile:canvas.toDataURL("image/jpeg",0.85).split(",")[1] };
      });
      mode = "valid"; count = 0;
      const result = await page.evaluate(async () => {
        const { consumeDesktopUpdates } = await import("/desktop-updates.js");
        const controller = new AbortController();
        const emitted = [];
        const canvas = document.createElement("canvas");
        await consumeDesktopUpdates({ canvas, session: "fixture", signal: controller.signal, current: () => true,
          emit: (frame) => { emitted.push(frame); if (frame.seq === 4) controller.abort(); } });
        const context = canvas.getContext("2d");
        return { count: emitted.length, red: [...context.getImageData(20, 20, 1, 1).data],
          blue: [...context.getImageData(300, 300, 1, 1).data],
          jpegRed: [...context.getImageData(90,20,1,1).data],
          sameBlob: emitted[1].blob === emitted[3].blob, unchanged: emitted[3].unchanged };
      });
      assert.equal(result.count, 4); assert(result.red[0] > 250 && result.red[2] < 3);
      assert(result.blue[2] > 250 && result.blue[0] < 3);
      assert(result.sameBlob && result.unchanged);assert(result.jpegRed[0]>245 && result.jpegRed[2]<10);
      const stopped = count; await new Promise((resolve) => setTimeout(resolve, 300)); assert.equal(count, stopped);
      for (mode of ["session", "auth", "duplicate"]) {
        count = 0;
        const error = await page.evaluate(async () => {
          const { consumeDesktopUpdates } = await import("/desktop-updates.js");
          const controller = new AbortController();
          const canvas = document.createElement("canvas");
          try { await consumeDesktopUpdates({ canvas, session: "fixture", signal: controller.signal, current: () => true, emit: () => {} }); }
          catch (error) { return { error: error.streamFailureReason || error.message,
            pixel: [...canvas.getContext("2d").getImageData(20,20,1,1).data] }; }
          finally { controller.abort(); }
        });
        assert.match(error.error, mode === "auth" ? /authorization/ : /Invalid desktop/);
        if (mode === "duplicate") assert(error.pixel[2] > 250 && error.pixel[0] < 3, "a rejected delta must not partially change the displayed frame");
        assert.equal(count, mode === "duplicate" ? 2 : 1);
      }
      console.log(`${engine.name()}: desktop regions reconstruct correctly, unchanged frames stay local, cancellation/auth/session bounds pass`);
    } finally { await browser.close(); }
  }
} finally { server.closeAllConnections(); await new Promise((resolve) => server.close(resolve)); }
