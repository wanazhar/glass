import { createHash } from "node:crypto";
import { createWriteStream, chmodSync, readFileSync, renameSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import https from "node:https";
import path from "node:path";
import { fileURLToPath } from "node:url";

if (process.env.GLASS_SKIP_DOWNLOAD === "1") process.exit(0);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const version = process.env.GLASS_VERSION ?? "v0.2.0";
const platform = `${process.platform}-${process.arch}`;
const names = {
  "linux-x64": "glass-linux-x86_64",
  "darwin-x64": "glass-macos-x86_64",
  "darwin-arm64": "glass-macos-aarch64",
};
const artifact = names[platform];
if (!artifact) throw new Error(`Glass has no release artifact for ${platform}`);
const base = `https://github.com/wanazhar/glass/releases/download/${version}`;
const target = path.join(root, "bin", "glass-native" + (process.platform === "win32" ? ".exe" : ""));
await mkdir(path.dirname(target), { recursive: true });
const temp = `${target}.${process.pid}.tmp`;
await download(`${base}/${artifact}`, temp);
const checksum = await downloadText(`${base}/${artifact.replace(/\.exe$/, "")}.sha256`);
const expected = checksum.trim().split(/\s+/)[0];
const actual = createHash("sha256").update(readFileSync(temp)).digest("hex");
if (expected && expected !== actual) throw new Error("Glass release checksum mismatch");
renameSync(temp, target);
if (process.platform !== "win32") chmodSync(target, 0o755);

function download(url, file) {
  return new Promise((resolve, reject) => {
    const output = createWriteStream(file);
    https.get(url, (response) => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        output.close();
        return download(new URL(response.headers.location, url), file).then(resolve, reject);
      }
      if (response.statusCode !== 200) return reject(new Error(`Glass download failed: HTTP ${response.statusCode}`));
      response.pipe(output).on("finish", () => output.close(resolve));
    }).on("error", reject);
  });
}
function downloadText(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (response) => {
      if (response.statusCode !== 200) return reject(new Error(`Glass checksum download failed: HTTP ${response.statusCode}`));
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => { body += chunk; });
      response.on("end", () => resolve(body));
    }).on("error", reject);
  });
}
